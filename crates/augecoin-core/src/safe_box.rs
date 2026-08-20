use crate::account::Account;
use augecoin_crypto::hash::blake3_512;
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProof {
    /// The account as it exists in the SafeBox.
    pub account: Account,
    /// Sibling hashes from leaf to root (each entry is the sibling at that level).
    pub siblings: Vec<[u8; 64]>,
    /// Index of the account leaf in the full leaf list (0-based).
    pub leaf_index: usize,
    /// Total number of leaves in the tree when the proof was generated.
    pub total_leaves: usize,
}

#[derive(Debug, Error)]
pub enum MerkleProofError {
    #[error("proof verification failed")]
    InvalidProof,
    #[error("account not found in safebox")]
    AccountNotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeBoxHeader {
    pub protocol: u16,
    pub start_block: u64,
    pub end_block: u64,
    pub blocks_count: u64,
    pub safe_box_hash: [u8; 64],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeBox {
    pub header: SafeBoxHeader,
    pub accounts: BTreeMap<u64, Account>,
    pub name_index: BTreeMap<String, u64>,
}

#[derive(Debug, Error)]
pub enum SafeBoxError {
    #[error("account not found: {0}")]
    AccountNotFound(u64),
    #[error("invalid safe box hash")]
    InvalidHash,
}

impl SafeBoxHeader {
    pub fn new(protocol: u16, start_block: u64, end_block: u64) -> Self {
        SafeBoxHeader {
            protocol,
            start_block,
            end_block,
            blocks_count: end_block.saturating_sub(start_block),
            safe_box_hash: [0u8; 64],
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.protocol.to_be_bytes());
        buf.extend_from_slice(&self.start_block.to_be_bytes());
        buf.extend_from_slice(&self.end_block.to_be_bytes());
        buf.extend_from_slice(&self.blocks_count.to_be_bytes());
        buf.extend_from_slice(&self.safe_box_hash);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 2 + 8 + 8 + 8 + 64 {
            return None;
        }
        let mut pos = 0;
        let protocol = u16::from_be_bytes([data[pos], data[pos + 1]]);
        pos += 2;
        let start_block = u64::from_be_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let end_block = u64::from_be_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let blocks_count = u64::from_be_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let safe_box_hash: [u8; 64] = data[pos..pos + 64].try_into().unwrap();

        Some(SafeBoxHeader {
            protocol,
            start_block,
            end_block,
            blocks_count,
            safe_box_hash,
        })
    }
}

impl SafeBox {
    pub fn new(protocol: u16, start_block: u64) -> Self {
        SafeBox {
            header: SafeBoxHeader::new(protocol, start_block, start_block),
            accounts: BTreeMap::new(),
            name_index: BTreeMap::new(),
        }
    }

    pub fn get_account(&self, account_number: u64) -> Option<&Account> {
        self.accounts.get(&account_number)
    }

    pub fn get_account_mut(&mut self, account_number: u64) -> Option<&mut Account> {
        self.accounts.get_mut(&account_number)
    }

    pub fn add_account(&mut self, account: Account) {
        // Remove old name entry for this account if it had one
        if let Some(old) = self.accounts.get(&account.account_number) {
            if let Some(ref old_name) = old.name {
                self.name_index.remove(old_name);
            }
        }
        // Insert new name if account has one
        if let Some(ref name) = account.name {
            self.name_index.insert(name.clone(), account.account_number);
        }
        self.accounts.insert(account.account_number, account);
    }

    pub fn account_count(&self) -> u64 {
        self.accounts.len() as u64
    }

    pub fn max_account_number(&self) -> u64 {
        self.accounts.keys().last().copied().unwrap_or(0)
    }

    pub fn is_name_taken(&self, name: &str) -> Option<u64> {
        self.name_index.get(name).copied()
    }

    pub fn compute_safe_box_hash(&self) -> [u8; 64] {
        if self.accounts.is_empty() {
            return [0u8; 64];
        }
        let mut hashes: Vec<[u8; 64]> = self.accounts.values().map(|a| a.hash()).collect();
        // Include name_index entries in the hash for consensus
        for (name, number) in &self.name_index {
            let mut data = Vec::new();
            data.extend_from_slice(name.as_bytes());
            data.extend_from_slice(&number.to_be_bytes());
            hashes.push(blake3_512(&data));
        }
        while hashes.len() > 1 {
            let mut next = Vec::with_capacity(hashes.len().div_ceil(2));
            for chunk in hashes.chunks(2) {
                let mut combined = Vec::new();
                combined.extend_from_slice(&chunk[0]);
                if chunk.len() > 1 {
                    combined.extend_from_slice(&chunk[1]);
                } else {
                    combined.extend_from_slice(&chunk[0]);
                }
                next.push(blake3_512(&combined));
            }
            hashes = next;
        }
        hashes[0]
    }

    pub fn update_hash(&mut self) {
        self.header.safe_box_hash = self.compute_safe_box_hash();
    }

    /// Generate a Merkle proof that `account_number` exists in this SafeBox with
    /// its current state. The proof can be verified by any node with only the
    /// SafeBox root hash.
    pub fn generate_merkle_proof(
        &self,
        account_number: u64,
    ) -> Result<MerkleProof, MerkleProofError> {
        let account = self
            .accounts
            .get(&account_number)
            .ok_or(MerkleProofError::AccountNotFound)?;

        let mut all_hashes: Vec<([u8; 64], u64)> = self
            .accounts
            .keys()
            .map(|num| {
                let h = self.accounts[num].hash();
                (h, *num)
            })
            .collect();

        // Also include name_index entries (same as compute_safe_box_hash)
        for (name, number) in &self.name_index {
            let mut data = Vec::new();
            data.extend_from_slice(name.as_bytes());
            data.extend_from_slice(&number.to_be_bytes());
            all_hashes.push((blake3_512(&data), u64::MAX));
        }

        // Find the leaf index of our account
        let leaf_idx = all_hashes
            .iter()
            .position(|(_, num)| *num == account_number)
            .ok_or(MerkleProofError::AccountNotFound)?;

        let mut siblings = Vec::new();
        let mut hashes: Vec<[u8; 64]> = all_hashes.iter().map(|(h, _)| *h).collect();
        let mut idx = leaf_idx;

        while hashes.len() > 1 {
            // If odd number of hashes, duplicate the last
            if !hashes.len().is_multiple_of(2) {
                hashes.push(*hashes.last().unwrap());
            }
            let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
            siblings.push(hashes[sibling_idx]);

            let mut next_level = Vec::with_capacity(hashes.len() / 2);
            for chunk in hashes.chunks(2) {
                let mut combined = Vec::new();
                combined.extend_from_slice(&chunk[0]);
                combined.extend_from_slice(&chunk[1]);
                next_level.push(blake3_512(&combined));
            }
            hashes = next_level;
            idx /= 2;
        }

        Ok(MerkleProof {
            account: account.clone(),
            siblings,
            leaf_index: leaf_idx,
            total_leaves: all_hashes.len(),
        })
    }

    /// Verify a Merkle proof against the SafeBox root hash.
    pub fn verify_merkle_proof(
        proof: &MerkleProof,
        root_hash: &[u8; 64],
    ) -> Result<(), MerkleProofError> {
        let mut current_hash = proof.account.hash();
        let mut idx = proof.leaf_index;

        for sibling in &proof.siblings {
            let mut combined = Vec::new();
            if idx.is_multiple_of(2) {
                combined.extend_from_slice(&current_hash);
                combined.extend_from_slice(sibling);
            } else {
                combined.extend_from_slice(sibling);
                combined.extend_from_slice(&current_hash);
            }
            current_hash = blake3_512(&combined);
            idx /= 2;
        }

        if current_hash == *root_hash {
            Ok(())
        } else {
            Err(MerkleProofError::InvalidProof)
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.header.to_bytes());
        buf.extend_from_slice(&(self.accounts.len() as u32).to_be_bytes());
        for account in self.accounts.values() {
            let acc_bytes = account.to_bytes();
            buf.extend_from_slice(&(acc_bytes.len() as u32).to_be_bytes());
            buf.extend_from_slice(&acc_bytes);
        }
        // Serialize name index
        buf.extend_from_slice(&(self.name_index.len() as u32).to_be_bytes());
        for (name, number) in &self.name_index {
            let name_bytes = name.as_bytes();
            buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
            buf.extend_from_slice(name_bytes);
            buf.extend_from_slice(&number.to_be_bytes());
        }
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let header = SafeBoxHeader::from_bytes(data)?;
        let header_size = header.to_bytes().len();
        if data.len() < header_size + 4 {
            return None;
        }
        let mut pos = header_size;
        let account_count = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        let mut accounts = BTreeMap::new();
        for _ in 0..account_count {
            if pos + 4 > data.len() {
                return None;
            }
            let acc_len = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;
            if pos + acc_len > data.len() {
                return None;
            }
            if let Ok(account) = Account::from_bytes(&data[pos..pos + acc_len]) {
                accounts.insert(account.account_number, account);
            }
            pos += acc_len;
        }
        // Deserialize name index
        let mut name_index = BTreeMap::new();
        if pos + 4 <= data.len() {
            let name_count = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;
            for _ in 0..name_count {
                if pos + 2 > data.len() {
                    return None;
                }
                let name_len = u16::from_be_bytes(data[pos..pos + 2].try_into().unwrap()) as usize;
                pos += 2;
                if pos + name_len + 8 > data.len() {
                    return None;
                }
                let name = String::from_utf8(data[pos..pos + name_len].to_vec()).ok()?;
                pos += name_len;
                let number = u64::from_be_bytes(data[pos..pos + 8].try_into().unwrap());
                pos += 8;
                name_index.insert(name, number);
            }
        }
        Some(SafeBox {
            header,
            accounts,
            name_index,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{AccountInfo, AccountKey};

    fn make_test_account(num: u64, balance: u64) -> Account {
        let mut ed = [0u8; 32];
        ed[0..8].copy_from_slice(&num.to_be_bytes());
        Account {
            account_number: num,
            account_info: AccountInfo {
                account_key: AccountKey {
                    ed25519_public_key: ed,
                },
                ..AccountInfo::default()
            },
            balance,
            updated_on_block_passive_mode: num * 10,
            updated_on_block_active_mode: num * 10,
            n_operation: 0,
            name: None,
            account_type: 0,
            account_data: vec![],
            account_seal: vec![],
        }
    }

    #[test]
    fn empty_safebox() {
        let sb = SafeBox::new(5, 0);
        assert_eq!(sb.account_count(), 0);
        assert_eq!(sb.max_account_number(), 0);
        assert_eq!(sb.compute_safe_box_hash(), [0u8; 64]);
        assert_eq!(sb.name_index.len(), 0);
    }

    #[test]
    fn safebox_with_accounts() {
        let mut sb = SafeBox::new(5, 0);
        sb.add_account(make_test_account(0, 1000));
        sb.add_account(make_test_account(1, 2000));
        sb.add_account(make_test_account(2, 3000));
        assert_eq!(sb.account_count(), 3);
        assert_eq!(sb.max_account_number(), 2);
        assert!(sb.get_account(1).is_some());
        assert!(sb.get_account(99).is_none());
    }

    #[test]
    fn safe_box_hash_changes() {
        let mut sb = SafeBox::new(5, 0);
        sb.add_account(make_test_account(0, 1000));
        let h1 = sb.compute_safe_box_hash();
        sb.add_account(make_test_account(1, 2000));
        let h2 = sb.compute_safe_box_hash();
        assert_ne!(h1, h2);
    }

    #[test]
    fn safebox_serialization_roundtrip() {
        let mut sb = SafeBox::new(5, 0);
        sb.add_account(make_test_account(0, 1000));
        sb.add_account(make_test_account(1, 2000));
        sb.add_account(make_test_account(5, 500));
        sb.update_hash();

        let bytes = sb.to_bytes();
        let restored = SafeBox::from_bytes(&bytes).unwrap();
        assert_eq!(sb.header, restored.header);
        assert_eq!(sb.accounts.len(), restored.accounts.len());
        assert_eq!(sb.get_account(0).unwrap().balance, 1000);
        assert_eq!(sb.name_index, restored.name_index);
    }

    #[test]
    fn name_index_unique_names() {
        let mut sb = SafeBox::new(5, 0);
        let mut acc1 = make_test_account(1, 1000);
        acc1.name = Some("alice".to_string());
        sb.add_account(acc1);
        assert_eq!(sb.is_name_taken("alice"), Some(1));
        assert_eq!(sb.is_name_taken("bob"), None);

        let mut acc2 = make_test_account(2, 2000);
        acc2.name = Some("bob".to_string());
        sb.add_account(acc2);
        assert_eq!(sb.is_name_taken("bob"), Some(2));
    }

    #[test]
    fn name_index_serialization_roundtrip() {
        let mut sb = SafeBox::new(5, 0);
        let mut acc1 = make_test_account(1, 1000);
        acc1.name = Some("alice".to_string());
        sb.add_account(acc1);
        let mut acc2 = make_test_account(2, 2000);
        acc2.name = Some("bob".to_string());
        sb.add_account(acc2);
        sb.update_hash();

        let bytes = sb.to_bytes();
        let restored = SafeBox::from_bytes(&bytes).unwrap();
        assert_eq!(restored.is_name_taken("alice"), Some(1));
        assert_eq!(restored.is_name_taken("bob"), Some(2));
        assert_eq!(restored.compute_safe_box_hash(), sb.compute_safe_box_hash());
    }

    #[test]
    fn merkle_proof_verifies() {
        let mut sb = SafeBox::new(5, 0);
        for i in 1..=5 {
            sb.add_account(make_test_account(i, i * 100));
        }
        sb.update_hash();
        let root = sb.header.safe_box_hash;

        let proof = sb.generate_merkle_proof(3).unwrap();
        assert_eq!(proof.account.account_number, 3);
        assert_eq!(proof.total_leaves, 5);

        SafeBox::verify_merkle_proof(&proof, &root).unwrap();
    }

    #[test]
    fn merkle_proof_wrong_root_rejected() {
        let mut sb = SafeBox::new(5, 0);
        sb.add_account(make_test_account(1, 100));
        sb.update_hash();
        let proof = sb.generate_merkle_proof(1).unwrap();

        let wrong_root = [0xFF; 64];
        assert!(SafeBox::verify_merkle_proof(&proof, &wrong_root).is_err());
    }

    #[test]
    fn merkle_proof_nonexistent_account() {
        let sb = SafeBox::new(5, 0);
        assert!(sb.generate_merkle_proof(999).is_err());
    }

    #[test]
    fn merkle_proof_different_account_data_rejected() {
        let mut sb = SafeBox::new(5, 0);
        sb.add_account(make_test_account(1, 100));
        sb.update_hash();
        let root = sb.header.safe_box_hash;

        let mut proof = sb.generate_merkle_proof(1).unwrap();
        proof.account.balance = 999;

        assert!(SafeBox::verify_merkle_proof(&proof, &root).is_err());
    }
}
