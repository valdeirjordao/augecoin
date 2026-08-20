//! Temporary testnet driver helper: builds and signs the exact operations a
//! wallet/CLI would produce, and prints them as hex for submission over the
//! JSON-RPC `sendoperation` endpoint. Uses the real crypto and serialization
//! crates (no mocks).

use augecoin_core::operation::{
    Operation, OperationPayload, OperationType, ReceiverInfo, SenderInfo, ValidatorAdminOp,
};
use augecoin_crypto::hdkeys::HdWallet;
use augecoin_crypto::signature::HybridKeyPair;

fn dev_admin_keypair() -> HybridKeyPair {
    let base_seed: [u8; 64] = [0xABu8; 64];
    let wallet = HdWallet::from_seed(&base_seed);
    wallet.derive_keypair(999)
}

fn parse_seed(hex: &str) -> [u8; 32] {
    let bytes = hex::decode(hex.trim()).expect("invalid seed hex");
    bytes[..32].try_into().expect("seed must be 32 bytes")
}

fn build_transfer(
    chain_id: u64,
    sender: u64,
    sender_seed: &str,
    n_operation: u64,
    receiver: u64,
    amount: u64,
    fee: u64,
) -> Operation {
    let kp = HybridKeyPair::from_seed(parse_seed(sender_seed));
    let op = Operation {
        chain_id,
        op_type: OperationType::Transaction,
        payload: OperationPayload::Transaction {
            senders: vec![SenderInfo {
                account: sender,
                n_operation,
                amount,
                payload: vec![],
            }],
            receivers: vec![ReceiverInfo {
                account: receiver,
                amount,
                payload: vec![],
            }],
            changers: vec![],
            fee,
        },
        signatures: vec![],
    };
    let message = op.to_bytes_stripped();
    let sig = kp.sign(&message);
    Operation {
        signatures: vec![sig],
        ..op
    }
}

fn build_admin(chain_id: u64, va_op: ValidatorAdminOp) -> Operation {
    let kp = dev_admin_keypair();
    let op = Operation {
        chain_id,
        op_type: OperationType::ValidatorAdminOp,
        payload: OperationPayload::ValidatorAdmin(va_op),
        signatures: vec![],
    };
    let message = op.to_bytes_stripped();
    let sig = kp.sign(&message);
    Operation {
        signatures: vec![sig],
        ..op
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: testnet_helper <command> ...");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "wallet-key" => {
            let seed = parse_seed(&args[2]);
            let kp = HybridKeyPair::from_seed(seed);
            let pk = kp.verifying_key();
            let addr = augecoin_crypto::address::derive_address(&pk);
            println!("public_key_hex={}", hex::encode(pk.to_bytes()));
            println!("address={addr}");
        }
        "transfer" => {
            let chain_id: u64 = args[2].parse().unwrap();
            let sender: u64 = args[3].parse().unwrap();
            let sender_seed = &args[4];
            let n_operation: u64 = args[5].parse().unwrap();
            let receiver: u64 = args[6].parse().unwrap();
            let amount: u64 = args[7].parse().unwrap();
            let fee: u64 = args.get(8).map(|s| s.parse().unwrap()).unwrap_or(0);
            let op = build_transfer(
                chain_id,
                sender,
                sender_seed,
                n_operation,
                receiver,
                amount,
                fee,
            );
            println!("{}", hex::encode(op.to_bytes()));
        }
        "admin-add" => {
            let chain_id: u64 = args[2].parse().unwrap();
            let ed_hex = &args[3];
            let ed: [u8; 32] = hex::decode(ed_hex).unwrap().try_into().unwrap();
            let activation_height: u64 = args[4].parse().unwrap();
            let op = build_admin(
                chain_id,
                ValidatorAdminOp::Add {
                    ed25519_public_key: ed,
                    activation_height,
                },
            );
            println!("{}", hex::encode(op.to_bytes()));
        }
        "admin-remove" => {
            let chain_id: u64 = args[2].parse().unwrap();
            let id: u64 = args[3].parse().unwrap();
            let activation_height: u64 = args[4].parse().unwrap();
            let op = build_admin(
                chain_id,
                ValidatorAdminOp::Remove {
                    validator_id: id,
                    activation_height,
                },
            );
            println!("{}", hex::encode(op.to_bytes()));
        }
        "admin-activate" => {
            let chain_id: u64 = args[2].parse().unwrap();
            let id: u64 = args[3].parse().unwrap();
            let activation_height: u64 = args[4].parse().unwrap();
            let op = build_admin(
                chain_id,
                ValidatorAdminOp::Activate {
                    validator_id: id,
                    activation_height,
                },
            );
            println!("{}", hex::encode(op.to_bytes()));
        }
        "admin-deactivate" => {
            let chain_id: u64 = args[2].parse().unwrap();
            let id: u64 = args[3].parse().unwrap();
            let activation_height: u64 = args[4].parse().unwrap();
            let op = build_admin(
                chain_id,
                ValidatorAdminOp::Deactivate {
                    validator_id: id,
                    activation_height,
                },
            );
            println!("{}", hex::encode(op.to_bytes()));
        }
        "dev-validator-key" => {
            let id: u64 = args[2].parse().unwrap();
            let kp = augecoin_node::consensus::create_dev_validator_key(id);
            let pk = kp.verifying_key();
            println!("{}", hex::encode(pk.to_bytes()));
        }
        _ => {
            eprintln!("unknown command {}", args[1]);
            std::process::exit(1);
        }
    }
}
