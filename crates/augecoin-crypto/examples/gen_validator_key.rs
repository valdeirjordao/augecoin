// AUGECOIN — Generate a validator keypair.
//
// Prints a fresh Ed25519 seed (64 hex chars) and the derived public key
// (64 hex chars). Use the seed as `AUGECOIN_VALIDATOR_KEY_HEX` on the
// validator node and register the public key with `validatoradd`.
//
// Usage:
//   cargo run --release -p augecoin-crypto --example gen_validator_key
//   cargo run --release -p augecoin-crypto --example gen_validator_key -- --seed <64-hex>
//   cargo run --release -p augecoin-crypto --example gen_validator_key -- --mnemonic "<phrase>"

use augecoin_crypto::hdkeys::HdWallet;
use augecoin_crypto::signature::HybridKeyPair;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let (seed_hex, mnemonic): (Option<String>, Option<String>) = if let Some(pos) =
        args.iter().position(|a| a == "--seed")
    {
        (args.get(pos + 1).cloned(), None)
    } else if let Some(pos) = args.iter().position(|a| a == "--mnemonic") {
        (None, args.get(pos + 1).cloned())
    } else {
        (None, None)
    };

    let kp: HybridKeyPair;
    let mut mnemonic_out = String::new();

    if let Some(hex) = seed_hex {
        let bytes = hex::decode(&hex).unwrap_or_else(|e| {
            eprintln!("invalid --seed hex: {e}");
            std::process::exit(1);
        });
        if bytes.len() != 32 {
            eprintln!("seed must be exactly 32 bytes (64 hex chars)");
            std::process::exit(1);
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        kp = HybridKeyPair::from_seed(seed);
    } else if let Some(phrase) = mnemonic {
        let wallet = HdWallet::from_mnemonic(&phrase).unwrap_or_else(|e| {
            eprintln!("invalid mnemonic: {e}");
            std::process::exit(1);
        });
        let derived = wallet.derive_keypair(0);
        kp = derived;
        mnemonic_out = phrase;
    } else {
        // Fresh random key. Generate the 32-byte seed and derive from it so
        // that `seed_hex` below is exactly the AUGECOIN_VALIDATOR_KEY_HEX.
        let mut seed = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut seed);
        kp = HybridKeyPair::from_seed(seed);
    }

    let pubkey = kp.verifying_key();
    let pub_hex = hex::encode(pubkey.to_bytes());
    let seed_bytes = kp.signing_key_bytes();
    let seed_hex = hex::encode(seed_bytes);

    println!("AUGECOIN_VALIDATOR_KEY_HEX={}", seed_hex);
    println!("ED25519_PUBLIC_KEY_HEX={}", pub_hex);
    if !mnemonic_out.is_empty() {
        println!("MNEMONIC={}", mnemonic_out);
    }
}
