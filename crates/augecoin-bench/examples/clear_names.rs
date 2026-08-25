//! One-off utility: clear the names of the dev validator accounts (0..3)
//! that a rename benchmark run set to "bench-N-0".

use augecoin_bench::client::{BenchClient, ClientConfig};
use augecoin_core::operation::{Operation, OperationPayload, OperationType};
use augecoin_crypto::hdkeys::HdWallet;
use serde_json::json;

fn derive_key(vid: u64) -> augecoin_crypto::signature::HybridKeyPair {
    let mut seed = [0u8; 64];
    seed[0] = 0xAB;
    seed[1..9].copy_from_slice(&vid.to_be_bytes());
    HdWallet::from_seed(&seed).derive_keypair(0)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let endpoints = vec!["https://127.0.0.1:9005".to_string()];
    let client = BenchClient::new(ClientConfig::new(endpoints))?;

    for account in 0..4u64 {
        let key = derive_key(account);
        let nonce = client
            .call("getaccount", json!({ "account_number": account }))
            .await
            .ok()
            .and_then(|v| v.get("n_operation").and_then(|n| n.as_u64()))
            .unwrap_or(0);

        let op = Operation {
            op_type: OperationType::ChangeAccountInfo,
            payload: OperationPayload::ChangeAccountInfo {
                account,
                n_operation: nonce,
                fee: 1_000,
                new_ed25519_public_key: key.verifying_key().to_bytes(),
                new_name: None,
                new_type: 0,
                new_account_data: vec![],
                new_account_seal: vec![],
            },
            signatures: vec![],
            chain_id: 2,
        };
        let sig = key.sign(&op.to_bytes_stripped());
        let signed = Operation {
            signatures: vec![sig],
            ..op
        };
        let params = json!({ "hex": hex::encode(signed.to_bytes()) });
        let res = client.call("sendoperation", params).await;
        match res {
            Ok(v) => println!(
                "account {account}: accepted={}",
                v.get("accepted").and_then(|b| b.as_bool()).unwrap_or(false)
            ),
            Err(e) => println!("account {account}: error {e}"),
        }
    }
    Ok(())
}
