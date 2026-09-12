use augecoin_crypto::hdkeys::HdWallet;
use serde::Serialize;

#[derive(Serialize)]
struct HdKeyVector {
    mnemonic: String,
    index: u64,
    ed25519_public_key_hex: String,
}

fn main() {
    let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let wallet = HdWallet::from_mnemonic(mnemonic).unwrap();
    let mut vectors = Vec::new();

    for i in [0u64, 1, 2, 42, 999] {
        let kp = wallet.derive_keypair(i);
        let vk = kp.verifying_key();
        let ed25519_hex = hex::encode(vk.to_bytes());

        vectors.push(HdKeyVector {
            mnemonic: mnemonic.to_string(),
            index: i,
            ed25519_public_key_hex: ed25519_hex,
        });
    }

    let json = serde_json::to_string_pretty(&vectors).unwrap();
    println!("{}", json);
}
