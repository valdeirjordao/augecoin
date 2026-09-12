use augecoin_crypto::signature::HybridKeyPair;
use std::env;

fn main() {
    let hex = env::args().nth(1).expect("usage: verify_key <64hex seed>");
    let bytes = hex::decode(hex.trim()).expect("invalid hex");
    if bytes.len() < 32 {
        panic!("seed must be at least 32 bytes");
    }
    let seed: [u8; 32] = bytes[..32].try_into().unwrap();
    let kp = HybridKeyPair::from_seed(seed);
    println!("priv: {}", hex::encode(kp.signing_key_bytes()));
    println!("pub : {}", hex::encode(kp.verifying_key().to_bytes()));
}
