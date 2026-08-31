use augecoin_core::account::Account;
use augecoin_core::block::OperationBlock;
use augecoin_core::operation::{Operation, OperationPayload};
use augecoin_storage::checkpoint::AccountSnapshot;
use augecoin_storage::Storage;
use std::collections::{HashMap, VecDeque};
use thiserror::Error;

pub const SYNC_CHECKPOINT_TOPIC: &str = "augecoin/sync/checkpoint";
pub const SYNC_BLOCK_TOPIC: &str = "augecoin/sync/block";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPhase {
    Idle,
    RequestingCheckpoint,
    RestoringCheckpoint,
    ReplayingBlocks,
    Complete,
}

#[derive(Debug, Clone, Error)]
pub enum SyncError {
    #[error("checkpoint deserialization failed: {0}")]
    InvalidCheckpoint(String),
    #[error("block deserialization failed: {0}")]
    InvalidBlock(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("sync already in progress")]
    AlreadySyncing,
    #[error("no checkpoint received yet")]
    NoCheckpoint,
    #[error("block number mismatch: expected {expected}, got {actual}")]
    BlockNumberMismatch { expected: u64, actual: u64 },
}

#[derive(Debug, Clone)]
pub struct SyncState {
    pub phase: SyncPhase,
    pub checkpoint_height: Option<u64>,
    pub current_height: u64,
    pub target_height: u64,
    pub blocks_remaining: usize,
}

#[derive(Debug)]
pub struct SyncManager {
    phase: SyncPhase,
    checkpoint: Option<AccountSnapshot>,
    checkpoint_height: u64,
    block_buffer: VecDeque<OperationBlock>,
    current_height: u64,
    target_height: u64,
    interrupted: bool,
    pre_sync_state: Option<HashMap<u64, Account>>,
}

impl SyncManager {
    pub fn new() -> Self {
        SyncManager {
            phase: SyncPhase::Idle,
            checkpoint: None,
            checkpoint_height: 0,
            block_buffer: VecDeque::new(),
            current_height: 0,
            target_height: 0,
            interrupted: false,
            pre_sync_state: None,
        }
    }

    pub fn state(&self) -> SyncState {
        SyncState {
            phase: self.phase,
            checkpoint_height: if self.checkpoint.is_some() {
                Some(self.checkpoint_height)
            } else {
                None
            },
            current_height: self.current_height,
            target_height: self.target_height,
            blocks_remaining: self.block_buffer.len(),
        }
    }

    pub fn start_sync(&mut self, target_height: u64) -> Result<(), SyncError> {
        if self.phase != SyncPhase::Idle && self.phase != SyncPhase::Complete {
            return Err(SyncError::AlreadySyncing);
        }

        self.phase = SyncPhase::RequestingCheckpoint;
        self.target_height = target_height;
        self.current_height = 0;
        self.checkpoint = None;
        self.checkpoint_height = 0;
        self.block_buffer.clear();
        self.interrupted = false;
        self.pre_sync_state = None;

        Ok(())
    }

    pub fn receive_checkpoint(&mut self, data: &[u8]) -> Result<u64, SyncError> {
        let snapshot = AccountSnapshot::from_bytes(data).map_err(SyncError::InvalidCheckpoint)?;

        let height = snapshot.height();
        self.checkpoint_height = height;
        self.current_height = height;
        self.checkpoint = Some(snapshot);
        self.phase = SyncPhase::RestoringCheckpoint;

        Ok(height)
    }

    pub fn apply_checkpoint(&self, storage: &Storage) -> Result<(), SyncError> {
        let snapshot = self.checkpoint.as_ref().ok_or(SyncError::NoCheckpoint)?;
        storage
            .restore_from_checkpoint(snapshot)
            .map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn begin_replay(&mut self) -> Result<u64, SyncError> {
        if self.phase != SyncPhase::RestoringCheckpoint && self.phase != SyncPhase::Idle {
            return Err(SyncError::AlreadySyncing);
        }

        if self.current_height >= self.target_height {
            self.phase = SyncPhase::Complete;
            return Ok(0);
        }

        self.phase = SyncPhase::ReplayingBlocks;
        Ok(self.current_height + 1)
    }

    pub fn receive_block(&mut self, data: &[u8], storage: &Storage) -> Result<u64, SyncError> {
        let block =
            OperationBlock::from_bytes(data).map_err(|e| SyncError::InvalidBlock(e.to_string()))?;

        let expected_height = self.current_height + 1;
        if block.header.block_number != expected_height {
            return Err(SyncError::BlockNumberMismatch {
                expected: expected_height,
                actual: block.header.block_number,
            });
        }

        if self.pre_sync_state.is_none() && self.current_height == self.checkpoint_height {
            self.pre_sync_state = Some(self.capture_accounts(storage));
        }

        self.apply_single_block(storage, &block)?;

        self.current_height = block.header.block_number;
        if self.current_height >= self.target_height {
            self.phase = SyncPhase::Complete;
        }

        Ok(block.header.block_number)
    }

    pub fn buffer_block(&mut self, data: &[u8]) -> Result<u64, SyncError> {
        let block =
            OperationBlock::from_bytes(data).map_err(|e| SyncError::InvalidBlock(e.to_string()))?;
        let height = block.header.block_number;
        self.block_buffer.push_back(block);
        Ok(height)
    }

    pub fn flush_buffer(&mut self, storage: &Storage) -> Result<usize, SyncError> {
        let mut applied = 0;
        loop {
            let expected_height = self.current_height + 1;
            let idx = self
                .block_buffer
                .iter()
                .position(|b| b.header.block_number == expected_height);

            let block = match idx {
                Some(i) => self.block_buffer.remove(i).unwrap(),
                None => break,
            };

            if self.pre_sync_state.is_none() {
                self.pre_sync_state = Some(self.capture_accounts(storage));
            }

            self.apply_single_block(storage, &block)?;
            self.current_height = block.header.block_number;
            applied += 1;

            if self.current_height >= self.target_height {
                self.phase = SyncPhase::Complete;
                break;
            }
        }
        Ok(applied)
    }

    pub fn mark_interrupted(&mut self) {
        self.interrupted = true;
    }

    pub fn rollback_interrupted(&self, storage: &Storage) -> Result<(), SyncError> {
        if let Some(ref pre_state) = self.pre_sync_state {
            storage
                .rollback_to_account_state(pre_state)
                .map_err(|e| SyncError::Storage(e.to_string()))?;
        }
        Ok(())
    }

    pub fn resume_from(
        &mut self,
        current_height: u64,
        target_height: u64,
    ) -> Result<(), SyncError> {
        if current_height >= target_height {
            self.phase = SyncPhase::Complete;
            self.current_height = current_height;
            self.target_height = target_height;
            return Ok(());
        }

        self.phase = SyncPhase::ReplayingBlocks;
        self.current_height = current_height;
        self.checkpoint_height = current_height;
        self.target_height = target_height;
        self.block_buffer.clear();
        self.interrupted = false;
        self.pre_sync_state = None;
        self.checkpoint = None;

        Ok(())
    }

    pub fn is_complete(&self) -> bool {
        self.phase == SyncPhase::Complete
    }

    pub fn is_interrupted(&self) -> bool {
        self.interrupted
    }

    fn capture_accounts(&self, storage: &Storage) -> HashMap<u64, Account> {
        let mut accounts = HashMap::new();
        for account_number in 1..=self.current_height.saturating_add(10) {
            if let Ok(Some(account)) = storage.get_account(account_number) {
                accounts.insert(account_number, account);
            }
        }
        accounts
    }

    fn operation_fee(op: &Operation) -> u64 {
        match &op.payload {
            OperationPayload::Transaction { fee, .. } => *fee,
            OperationPayload::AddressTransaction { fee, .. } => *fee,
            OperationPayload::ChangeKey { fee, .. } => *fee,
            OperationPayload::RecoverFounds { .. } => 0,
            OperationPayload::ListAccountForSale { fee, .. } => *fee,
            OperationPayload::DelistAccount { fee, .. } => *fee,
            OperationPayload::BuyAccount { fee, .. } => *fee,
            OperationPayload::ChangeKeySigned { fee, .. } => *fee,
            OperationPayload::ChangeAccountInfo { fee, .. } => *fee,
            OperationPayload::MultiOperation { fee, .. } => *fee,
            OperationPayload::Data { fee, .. } => *fee,
            OperationPayload::ValidatorAdmin(_) => 0,
            OperationPayload::CreateAccount { .. } => 0,
            OperationPayload::GiftAccount { fee, .. } => *fee,
            OperationPayload::AcceptGift { fee, .. } => *fee,
            OperationPayload::Contract { fee, .. } => *fee,
        }
    }

    fn apply_single_block(
        &self,
        storage: &Storage,
        block: &OperationBlock,
    ) -> Result<(), SyncError> {
        let leader_id = block.header.leader_id;
        let block_number = block.header.block_number;

        let ed_pk = [0u8; 32];
        let new_account = Account::new(block_number, ed_pk, block_number);

        storage
            .put_account(&new_account)
            .map_err(|e| SyncError::Storage(e.to_string()))?;

        let mut leader = storage
            .get_account(leader_id)
            .map_err(|e| SyncError::Storage(e.to_string()))?
            .unwrap_or_else(|| Account::new(leader_id, [0u8; 32], block_number));

        let total_fees: u64 = block.operations.iter().map(Self::operation_fee).sum();

        let reward = block.header.reward;
        leader
            .add_balance(reward.checked_add(total_fees).unwrap_or(reward))
            .map_err(|_| SyncError::Storage("balance overflow on leader reward".into()))?;

        storage
            .put_account(&leader)
            .map_err(|e| SyncError::Storage(e.to_string()))?;

        Ok(())
    }
}

impl Default for SyncManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod sync_tests {
    use super::*;
    use augecoin_core::account::Account;
    use augecoin_core::block::{OperationBlock, OperationBlockHeader};
    use augecoin_crypto::signature::Ed25519Signature;
    use augecoin_storage::Storage;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(5000);

    fn temp_path() -> String {
        format!(
            "/tmp/augecoin-sync-{}",
            TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
        )
    }

    fn make_account(number: u64, balance: u64) -> Account {
        let mut ed = [0u8; 32];
        ed[0..8].copy_from_slice(&number.to_be_bytes());
        let mut acc = Account::new(number, ed, 100);
        if balance > 0 {
            acc.add_balance(balance).unwrap();
        }
        acc
    }

    fn dummy_signature() -> Ed25519Signature {
        Ed25519Signature { bytes: [0u8; 64] }
    }

    fn make_block(block_number: u64, leader_id: u64) -> OperationBlock {
        OperationBlock {
            header: OperationBlockHeader {
                block_number,
                account_key: [0xAAu8; 32],
                reward: augecoin_core::emission::block_reward(block_number),
                fee: 0,
                protocol_version: 5,
                protocol_available: 6,
                timestamp: 1700000000 + block_number * 60,
                initial_safe_box_hash: [0u8; 64],
                operations_hash: [0u8; 64],
                block_payload: vec![],
                proof_of_work: [0u8; 32],
                previous_proof_of_work: [0u8; 32],
                leader_id,
                chain_id: 1,
            },
            operations: vec![],
            leader_signature: dummy_signature(),
            quorum_signatures: vec![dummy_signature(), dummy_signature(), dummy_signature()],
            block_hash: [0u8; 64],
        }
    }

    fn setup_source_and_blocks(
        source_storage: &Storage,
        checkpoint_height: u64,
    ) -> (AccountSnapshot, Vec<OperationBlock>) {
        for i in 0..5 {
            source_storage
                .put_account(&make_account(i, (i + 1) * 1000))
                .unwrap();
        }

        let checkpoint = source_storage.create_checkpoint(checkpoint_height);

        let final_height = checkpoint_height + 3;
        let mut blocks = Vec::new();

        for h in (checkpoint_height + 1)..=final_height {
            let block = make_block(h, h % 4);

            let new_acc = Account::new(h, [0u8; 32], h);
            source_storage.put_account(&new_acc).unwrap();

            let mut leader = source_storage
                .get_account(block.header.leader_id)
                .unwrap()
                .unwrap_or_else(|| make_account(block.header.leader_id, 0));
            leader.add_balance(block.header.reward).unwrap();
            source_storage.put_account(&leader).unwrap();

            blocks.push(block);
        }

        (checkpoint, blocks)
    }

    #[test]
    fn full_sync_via_checkpoint_and_replay() {
        let source_path = temp_path();
        let target_path = temp_path();

        let source_storage = Storage::open(&source_path).unwrap();
        let checkpoint_height = 10;
        let (checkpoint, blocks) = setup_source_and_blocks(&source_storage, checkpoint_height);

        let checkpoint_bytes = checkpoint.to_bytes();
        let block_bytes: Vec<Vec<u8>> = blocks.iter().map(|b| b.to_bytes()).collect();
        let final_height = blocks.last().unwrap().header.block_number;

        let target_storage = Storage::open(&target_path).unwrap();
        let mut sync = SyncManager::new();

        sync.start_sync(final_height).unwrap();
        let ck_height = sync.receive_checkpoint(&checkpoint_bytes).unwrap();
        assert_eq!(ck_height, checkpoint_height);

        sync.apply_checkpoint(&target_storage).unwrap();

        for i in 0..5 {
            let acc = target_storage.get_account(i).unwrap().unwrap();
            assert_eq!(acc.balance, (i + 1) * 1000);
        }

        let next = sync.begin_replay().unwrap();
        assert_eq!(next, checkpoint_height + 1);

        for b in &block_bytes {
            let applied = sync.receive_block(b, &target_storage).unwrap();
            assert!(applied > checkpoint_height);
        }

        assert!(sync.is_complete());
        assert_eq!(sync.state().current_height, final_height);

        for block in &blocks {
            let acc = target_storage
                .get_account(block.header.block_number)
                .unwrap()
                .unwrap();
            assert_eq!(acc.account_number, block.header.block_number);
        }

        std::fs::remove_dir_all(&source_path).ok();
        std::fs::remove_dir_all(&target_path).ok();
    }

    #[test]
    fn interrupted_sync_can_be_resumed() {
        let source_path = temp_path();
        let target_path = temp_path();

        let source_storage = Storage::open(&source_path).unwrap();
        let checkpoint_height = 5;
        let (checkpoint, blocks) = setup_source_and_blocks(&source_storage, checkpoint_height);

        let checkpoint_bytes = checkpoint.to_bytes();
        let block_bytes: Vec<Vec<u8>> = blocks.iter().map(|b| b.to_bytes()).collect();
        let final_height = blocks.last().unwrap().header.block_number;

        let target_storage = Storage::open(&target_path).unwrap();
        let mut sync = SyncManager::new();

        sync.start_sync(final_height).unwrap();
        sync.receive_checkpoint(&checkpoint_bytes).unwrap();
        sync.apply_checkpoint(&target_storage).unwrap();
        sync.begin_replay().unwrap();

        sync.receive_block(&block_bytes[0], &target_storage)
            .unwrap();

        sync.mark_interrupted();
        assert!(sync.is_interrupted());
        sync.rollback_interrupted(&target_storage).unwrap();

        let mut resumed = SyncManager::new();
        resumed
            .resume_from(checkpoint_height, final_height)
            .unwrap();
        assert_eq!(resumed.state().phase, SyncPhase::ReplayingBlocks);

        for b in &block_bytes {
            resumed.receive_block(b, &target_storage).unwrap();
        }

        assert!(resumed.is_complete());
        assert_eq!(resumed.state().current_height, final_height);

        std::fs::remove_dir_all(&source_path).ok();
        std::fs::remove_dir_all(&target_path).ok();
    }

    #[test]
    fn state_not_corrupted_after_full_sync() {
        let source_path = temp_path();
        let target_path = temp_path();

        let source_storage = Storage::open(&source_path).unwrap();
        let checkpoint_height = 3;
        let (checkpoint, blocks) = setup_source_and_blocks(&source_storage, checkpoint_height);

        let checkpoint_bytes = checkpoint.to_bytes();
        let block_bytes: Vec<Vec<u8>> = blocks.iter().map(|b| b.to_bytes()).collect();
        let final_height = blocks.last().unwrap().header.block_number;

        let target_storage = Storage::open(&target_path).unwrap();
        let mut sync = SyncManager::new();

        sync.start_sync(final_height).unwrap();
        sync.receive_checkpoint(&checkpoint_bytes).unwrap();
        sync.apply_checkpoint(&target_storage).unwrap();
        sync.begin_replay().unwrap();

        for b in &block_bytes {
            sync.receive_block(b, &target_storage).unwrap();
        }
        assert!(sync.is_complete());

        for acc_num in 0..=final_height {
            let src = source_storage.get_account(acc_num).unwrap();
            let tgt = target_storage.get_account(acc_num).unwrap();
            match (&src, &tgt) {
                (Some(s), Some(t)) => {
                    assert_eq!(
                        s.balance, t.balance,
                        "balance mismatch for account {acc_num}"
                    );
                    assert_eq!(s.account_number, t.account_number);
                }
                (None, None) => {}
                _ => panic!("presence mismatch for account {acc_num}: src={src:?}, tgt={tgt:?}"),
            }
        }

        std::fs::remove_dir_all(&source_path).ok();
        std::fs::remove_dir_all(&target_path).ok();
    }

    #[test]
    fn buffer_and_flush_out_of_order_blocks() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let checkpoint = storage.create_checkpoint(0);

        let mut sync = SyncManager::new();
        sync.start_sync(3).unwrap();
        sync.receive_checkpoint(&checkpoint.to_bytes()).unwrap();
        sync.apply_checkpoint(&storage).unwrap();
        sync.begin_replay().unwrap();

        let block1 = make_block(1, 1);
        let block2 = make_block(2, 2);
        let block3 = make_block(3, 3);

        sync.buffer_block(&block3.to_bytes()).unwrap();
        sync.buffer_block(&block1.to_bytes()).unwrap();
        sync.buffer_block(&block2.to_bytes()).unwrap();

        let applied = sync.flush_buffer(&storage).unwrap();
        assert_eq!(applied, 3);
        assert!(sync.is_complete());
        assert_eq!(sync.state().current_height, 3);

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn resume_from_midpoint_continues_correctly() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        for i in 0..3 {
            storage
                .put_account(&make_account(i, (i + 1) * 100))
                .unwrap();
        }
        let checkpoint = storage.create_checkpoint(5);
        let ck_bytes = checkpoint.to_bytes();

        let mut sync = SyncManager::new();
        sync.start_sync(8).unwrap();
        sync.receive_checkpoint(&ck_bytes).unwrap();
        sync.apply_checkpoint(&storage).unwrap();
        sync.begin_replay().unwrap();

        let block6 = make_block(6, 0);
        let block7 = make_block(7, 3);
        sync.receive_block(&block6.to_bytes(), &storage).unwrap();
        sync.receive_block(&block7.to_bytes(), &storage).unwrap();
        sync.mark_interrupted();

        let mut resumed = SyncManager::new();
        resumed.resume_from(7, 8).unwrap();

        let block8 = make_block(8, 1);
        resumed.receive_block(&block8.to_bytes(), &storage).unwrap();

        assert!(resumed.is_complete());
        assert_eq!(resumed.state().current_height, 8);

        std::fs::remove_dir_all(&path).ok();
    }
}
