//! FASE 1 instrumentation: measure the real serialized size distribution of
//! stress-test transfer operations and the fixed block-header overhead.
//!
//! Run with: cargo run --release -p augecoin-core --example tx_size_measure

use augecoin_core::block::{block_serialized_size, OperationBlockHeader};
use augecoin_core::limits;
use augecoin_core::operation::{
    Operation, OperationPayload, OperationType, ReceiverInfo, SenderInfo,
};

fn main() {
    let n = 1_000usize;

    let mut min = usize::MAX;
    let mut max = 0usize;
    let mut total = 0usize;
    for i in 0..n {
        let op = Operation {
            op_type: OperationType::Transaction,
            payload: OperationPayload::Transaction {
                senders: vec![SenderInfo {
                    account: (i % 4) as u64,
                    n_operation: (i / 4) as u64,
                    amount: 100_000,
                    payload: vec![],
                }],
                receivers: vec![ReceiverInfo {
                    account: 40,
                    amount: 100_000,
                    payload: vec![],
                }],
                changers: vec![],
                fee: 1_000,
            },
            chain_id: 2,
            signatures: vec![augecoin_crypto::signature::Ed25519Signature { bytes: [0u8; 64] }],
        };
        let sz = op.to_bytes().len();
        min = min.min(sz);
        max = max.max(sz);
        total += sz;
    }
    let avg = total as f64 / n as f64;
    println!("tx_size_bytes min={min} max={max} avg={avg:.1} samples={n}");

    let header = OperationBlockHeader {
        block_number: 1,
        account_key: [0u8; 32],
        reward: 250_000_000,
        fee: 0,
        protocol_version: 5,
        protocol_available: 6,
        timestamp: 1_700_000_000,
        initial_safe_box_hash: [0u8; 64],
        operations_hash: [0u8; 64],
        block_payload: vec![],
        proof_of_work: [0u8; 32],
        previous_proof_of_work: [0u8; 32],
        leader_id: 0,
        chain_id: 1,
    };
    let header_overhead = header.to_bytes().len();
    let fixed_block_overhead = block_serialized_size(&header, &[], 4) - header_overhead;
    println!(
        "header_bytes={header_overhead} fixed_block_overhead_bytes={fixed_block_overhead} (ops count + leader sig + quorum count + 4 quorum sigs)"
    );

    let max_transmit = limits::max_transmit_size();
    let max_block = limits::max_block_serialized_size();
    let ops_budget = max_block.saturating_sub(block_serialized_size(&header, &[], 4));
    let ops_that_fit = ops_budget / (4 + max);
    println!(
        "max_transmit_size={max_transmit} max_block_serialized_size={max_block} ops_that_fit~={ops_that_fit}"
    );
}
