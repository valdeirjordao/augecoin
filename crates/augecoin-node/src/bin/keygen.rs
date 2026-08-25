use augecoin_crypto::hdkeys::HdWallet;
use bip39::Mnemonic;
use rand::RngCore;

fn main() {
    let mut entropy = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut entropy);

    let mnemonic = Mnemonic::from_entropy(&entropy).expect("failed to create mnemonic");
    let phrase = mnemonic.to_string();

    let wallet = HdWallet::from_mnemonic(&phrase).expect("failed to create wallet");
    let keypair = wallet.derive_keypair(0);

    let verifying = keypair.verifying_key();
    let private_bytes = keypair.signing_key_bytes();
    let public_hex = hex::encode(verifying.to_bytes());
    let private_hex = hex::encode(private_bytes);

    eprintln!("============================================================");
    eprintln!("  SAVE THIS MNEMONIC SECURELY. IT WILL NOT BE SHOWN AGAIN.");
    eprintln!("============================================================");
    println!("{phrase}");
    eprintln!();
    eprintln!("Ed25519 public key (hex):");
    println!("{public_hex}");
    eprintln!();
    eprintln!("============================================================");
    eprintln!("  WARNING: Keep this private key secret. Never share it.");
    eprintln!("============================================================");
    eprintln!("Ed25519 private key (hex):");
    println!("{private_hex}");
}
