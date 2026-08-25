//! Build and sign AUGECOIN operations for the benchmark.
//!
//! Operations are constructed with the official `augecoin-core` types and
//! signed with `augecoin-crypto` keypairs, mirroring what the wallets do.
//! The signed operations are then submitted over the official RPCs
//! (`sendoperation`, `sellaccount`, `buyaccount`, `giftaccount`,
//! `acceptgift`, and ChangeAccountInfo via `sendoperation`).

use anyhow::{anyhow, Result};
use augecoin_core::operation::{
    Operation, OperationPayload, OperationType, ReceiverInfo, SenderInfo,
};
use augecoin_crypto::signature::{HybridKeyPair, HybridSignature};
use serde_json::{json, Value};

/// Minimum fee in augesat (matches `MIN_FEE_AUGESAT`).
pub const MIN_FEE: u64 = 1_000;
/// The testnet chain id used by the bench (matches the running devnet).
pub const CHAIN_ID: u64 = 2;

/// Build a signed `Transaction` operation (transfer between accounts).
pub fn make_transfer(
    key: &HybridKeyPair,
    sender: u64,
    n_operation: u64,
    receiver: u64,
    amount: u64,
) -> Operation {
    let unsigned = Operation {
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
            fee: MIN_FEE,
        },
        signatures: vec![],
        chain_id: CHAIN_ID,
    };
    sign_operation(key, &unsigned)
}

/// Build a signed `ListAccountForSale` (sell) operation.
pub fn make_sell(
    key: &HybridKeyPair,
    account: u64,
    n_operation: u64,
    sale_price: u64,
    account_to_pay: u64,
) -> Operation {
    let unsigned = Operation {
        op_type: OperationType::ListAccountForSale,
        payload: OperationPayload::ListAccountForSale {
            account,
            n_operation,
            sale_price,
            account_to_pay,
            new_ed25519_public_key: key.verifying_key().to_bytes(),
            locked_until_block: 0,
            fee: MIN_FEE,
        },
        signatures: vec![],
        chain_id: CHAIN_ID,
    };
    sign_operation(key, &unsigned)
}

/// Build a signed `BuyAccount` operation.
pub fn make_buy(
    key: &HybridKeyPair,
    buyer_account: u64,
    n_operation: u64,
    account_to_purchase: u64,
    amount: u64,
    seller_account: u64,
) -> Operation {
    let unsigned = Operation {
        op_type: OperationType::BuyAccount,
        payload: OperationPayload::BuyAccount {
            buyer_account,
            n_operation,
            account_to_purchase,
            amount,
            fee: MIN_FEE,
            new_ed25519_public_key: key.verifying_key().to_bytes(),
            seller_account,
        },
        signatures: vec![],
        chain_id: CHAIN_ID,
    };
    sign_operation(key, &unsigned)
}

/// Build a signed `GiftAccount` operation.
pub fn make_gift(
    key: &HybridKeyPair,
    account: u64,
    n_operation: u64,
    recipient_public_key: [u8; 32],
) -> Operation {
    let unsigned = Operation {
        op_type: OperationType::GiftAccount,
        payload: OperationPayload::GiftAccount {
            account,
            n_operation,
            recipient_public_key,
            fee: MIN_FEE,
        },
        signatures: vec![],
        chain_id: CHAIN_ID,
    };
    sign_operation(key, &unsigned)
}

/// Build a signed `AcceptGift` operation.
pub fn make_accept_gift(key: &HybridKeyPair, account: u64, n_operation: u64) -> Operation {
    let unsigned = Operation {
        op_type: OperationType::AcceptGift,
        payload: OperationPayload::AcceptGift {
            account,
            n_operation,
            fee: MIN_FEE,
        },
        signatures: vec![],
        chain_id: CHAIN_ID,
    };
    sign_operation(key, &unsigned)
}

/// Build a signed `ChangeAccountInfo` (rename) operation.
pub fn make_rename(
    key: &HybridKeyPair,
    account: u64,
    n_operation: u64,
    new_name: String,
) -> Operation {
    let unsigned = Operation {
        op_type: OperationType::ChangeAccountInfo,
        payload: OperationPayload::ChangeAccountInfo {
            account,
            n_operation,
            fee: MIN_FEE,
            new_ed25519_public_key: key.verifying_key().to_bytes(),
            new_name: Some(new_name),
            new_type: 0,
            new_account_data: vec![],
            new_account_seal: vec![],
        },
        signatures: vec![],
        chain_id: CHAIN_ID,
    };
    sign_operation(key, &unsigned)
}

/// Sign an unsigned operation using the given keypair over the stripped bytes.
pub fn sign_operation(key: &HybridKeyPair, op: &Operation) -> Operation {
    let stripped = op.to_bytes_stripped();
    let sig = key.sign(&stripped);
    let mut signed = op.clone();
    signed.signatures = vec![sig];
    signed
}

/// Serialize a signed operation into the hex form expected by `sendoperation`.
pub fn op_to_hex(op: &Operation) -> String {
    hex::encode(op.to_bytes())
}

/// Return the `new_public_key_hex` / `new_ed25519_public_key` hex for params.
pub fn pubkey_hex(key: &HybridKeyPair) -> String {
    hex::encode(key.verifying_key().to_bytes())
}

/// Convert an `Operation` into the JSON params for the `sendoperation` RPC.
pub fn sendoperation_params(op: &Operation) -> Value {
    json!({ "hex": op_to_hex(op) })
}

/// Convert a signed operation into `sellaccount` RPC params.
pub fn sellaccount_params(
    key: &HybridKeyPair,
    account: u64,
    n_operation: u64,
    sale_price: u64,
    account_to_pay: u64,
) -> Value {
    let op = make_sell(key, account, n_operation, sale_price, account_to_pay);
    json!({
        "account": account,
        "n_operation": n_operation,
        "sale_price": sale_price,
        "account_to_pay": account_to_pay,
        "locked_until_block": 0,
        "fee": MIN_FEE,
        "new_public_key_hex": pubkey_hex(key),
        "signature_hex": sig_hex(&op),
    })
}

/// Convert a signed operation into `buyaccount` RPC params.
pub fn buyaccount_params(
    key: &HybridKeyPair,
    buyer_account: u64,
    n_operation: u64,
    account_to_purchase: u64,
    amount: u64,
    seller_account: u64,
) -> Value {
    let op = make_buy(
        key,
        buyer_account,
        n_operation,
        account_to_purchase,
        amount,
        seller_account,
    );
    json!({
        "buyer_account": buyer_account,
        "n_operation": n_operation,
        "account_to_purchase": account_to_purchase,
        "amount": amount,
        "fee": MIN_FEE,
        "new_public_key_hex": pubkey_hex(key),
        "seller_account": seller_account,
        "signature_hex": sig_hex(&op),
    })
}

/// Convert a signed operation into `giftaccount` RPC params.
pub fn giftaccount_params(
    key: &HybridKeyPair,
    account: u64,
    n_operation: u64,
    recipient_public_key: [u8; 32],
) -> Value {
    let op = make_gift(key, account, n_operation, recipient_public_key);
    json!({
        "account": account,
        "n_operation": n_operation,
        "recipient_public_key_hex": hex::encode(recipient_public_key),
        "fee": MIN_FEE,
        "signature_hex": sig_hex(&op),
    })
}

/// Convert a signed operation into `acceptgift` RPC params.
pub fn acceptgift_params(key: &HybridKeyPair, account: u64, n_operation: u64) -> Value {
    let op = make_accept_gift(key, account, n_operation);
    json!({
        "account": account,
        "n_operation": n_operation,
        "fee": MIN_FEE,
        "signature_hex": sig_hex(&op),
    })
}

/// Extract the signature hex (the first signature) from a signed operation.
pub fn sig_hex(op: &Operation) -> String {
    match op.signatures.first() {
        Some(HybridSignature { bytes }) => hex::encode(bytes),
        None => String::new(),
    }
}

/// RPC params for `getaccount` by account number.
pub fn getaccount_params(account: u64) -> Value {
    json!({ "account_number": account })
}

/// RPC params for `nodestatus`.
pub fn nodestatus_params() -> Value {
    json!({})
}

/// RPC params for `getpendings`.
pub fn getpendings_params() -> Value {
    json!({})
}

/// Parse the `n_operation` from a `getaccount` result.
pub fn parse_n_operation(result: &Value) -> Result<u64> {
    result
        .get("n_operation")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("missing n_operation in getaccount result"))
}

/// Parse the `balance` from a `getaccount` result.
pub fn parse_balance(result: &Value) -> Result<u64> {
    result
        .get("balance")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("missing balance in getaccount result"))
}

/// Parse `accepted` from an operation submission result.
pub fn parse_accepted(result: &Value) -> Result<bool> {
    result
        .get("accepted")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("missing accepted in operation result"))
}
