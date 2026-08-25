//! Cross-language parity test (Rust side).
//!
//! These vectors are also asserted by the TypeScript SDK test suite
//! (packages/sdk-ts/tests/transaction.test.ts). Together they lock the
//! operation serialization, HD key derivation and signing scheme so that
//! Wallet, SDK and Node produce identical digests.

use augecoin_core::operation::{
    Operation, OperationPayload, OperationType, ReceiverInfo, SenderInfo,
};
use augecoin_crypto::hdkeys::HdWallet;

const MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

const STRIPPED_HEX: &str = "010000000100000000000000640000000000000000000000003b9aca0000000000000100000000000000c8000000003b9aca0000000000000000000000000000000000000000000002";
const PUBKEY_HEX: &str = "6589bfd8bbf0e34991b0cf5cf3467a2755ddf4a744809cb718b8f040cf3d780c";
const SIG_HEX: &str = "76aa57e857703d30e1282ce21388bc6c025346cdf6d5b1b895ed5a4429ec469251f32f7fe8ff47dfc3ec88e5c8d20677b0060db12812304703f0716055c08601";
// Base58 address: first 24 bytes of blake3-512(PUBKEY_HEX) plus a 4-byte
// domain-separated checksum, locked against packages/sdk-ts/tests/address.test.ts.
const ADDRESS: &str = "274rGuUx9XozCeJ2LBXggKLp5dd31fugXWKNinW";

fn transfer_op() -> Operation {
    Operation {
        chain_id: 2,
        op_type: OperationType::Transaction,
        payload: OperationPayload::Transaction {
            senders: vec![SenderInfo {
                account: 100,
                n_operation: 0,
                amount: 1_000_000_000,
                payload: vec![],
            }],
            receivers: vec![ReceiverInfo {
                account: 200,
                amount: 1_000_000_000,
                payload: vec![],
            }],
            changers: vec![],
            fee: 0,
        },
        signatures: vec![],
    }
}

#[test]
fn operation_serialization_matches_cross_language_vector() {
    let op = transfer_op();
    assert_eq!(hex::encode(op.to_bytes_stripped()), STRIPPED_HEX);
}

#[test]
fn hd_key_matches_cross_language_vector() {
    let wallet = HdWallet::from_mnemonic(MNEMONIC).unwrap();
    let kp = wallet.derive_keypair(0);
    assert_eq!(hex::encode(kp.verifying_key().to_bytes()), PUBKEY_HEX);
}

#[test]
fn signature_matches_cross_language_vector() {
    let wallet = HdWallet::from_mnemonic(MNEMONIC).unwrap();
    let kp = wallet.derive_keypair(0);
    let op = transfer_op();
    let sig = kp.sign(&op.to_bytes_stripped());
    assert_eq!(hex::encode(sig.bytes), SIG_HEX);
}

#[test]
fn address_matches_cross_language_vector() {
    let pk: [u8; 32] = hex::decode(PUBKEY_HEX).unwrap().try_into().unwrap();
    let vk = ed25519_dalek::VerifyingKey::from_bytes(&pk).unwrap();
    assert_eq!(augecoin_crypto::address::derive_address(&vk), ADDRESS);
}
