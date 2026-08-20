use augecoin_crypto::hdkeys::HdWallet;
use augecoin_crypto::signature::Ed25519KeyPair;
use serde::Serialize;
use std::sync::Mutex;

static WALLET: Mutex<Option<HdWallet>> = Mutex::new(None);
#[allow(dead_code)]
static KEYPAIR_CACHE: Mutex<Option<Ed25519KeyPair>> = Mutex::new(None);

#[derive(Debug, Serialize)]
pub struct KeyPairInfo {
    pub ed25519_public_key_hex: String,
}

#[derive(Debug, Serialize)]
pub struct WalletInfo {
    pub mnemonic: String,
    pub word_count: usize,
}

#[tauri::command]
pub fn create_wallet() -> Result<WalletInfo, String> {
    use bip39::Mnemonic;
    use rand::Rng;

    let entropy: [u8; 16] = rand::thread_rng().gen();
    let mnemonic = Mnemonic::from_entropy(&entropy)
        .map_err(|e| format!("failed to generate mnemonic: {e}"))?;
    let phrase = mnemonic.to_string();

    let wallet =
        HdWallet::from_mnemonic(&phrase).map_err(|e| format!("failed to create wallet: {e}"))?;

    let word_count = phrase.split_whitespace().count();
    let mut guard = WALLET.lock().unwrap();
    *guard = Some(wallet);

    Ok(WalletInfo {
        mnemonic: phrase,
        word_count,
    })
}

#[tauri::command]
pub fn import_wallet(mnemonic: String) -> Result<WalletInfo, String> {
    let wallet =
        HdWallet::from_mnemonic(&mnemonic).map_err(|e| format!("invalid mnemonic: {e}"))?;

    let word_count = mnemonic.split_whitespace().count();
    let mut guard = WALLET.lock().unwrap();
    *guard = Some(wallet);

    Ok(WalletInfo {
        mnemonic,
        word_count,
    })
}

#[tauri::command]
pub fn derive_keypair(index: u64) -> Result<KeyPairInfo, String> {
    let guard = WALLET.lock().unwrap();
    let wallet = guard.as_ref().ok_or("no wallet loaded")?;

    let kp = wallet.derive_keypair(index);
    let pk = kp.verifying_key();

    Ok(KeyPairInfo {
        ed25519_public_key_hex: hex::encode(pk.to_bytes()),
    })
}

#[tauri::command]
pub fn sign_message(index: u64, message: String) -> Result<String, String> {
    let guard = WALLET.lock().unwrap();
    let wallet = guard.as_ref().ok_or("no wallet loaded")?;

    let kp = wallet.derive_keypair(index);
    let sig = kp.sign(message.as_bytes());

    Ok(hex::encode(sig.bytes))
}

// ── OS Keychain integration ──────────────────────────────────────────

const KEYCHAIN_SERVICE: &str = "augecoin-wallet";
const KEYCHAIN_USER: &str = "wallet-seed";

#[tauri::command]
pub fn keystore_save(mnemonic: String, password: String) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_USER)
        .map_err(|e| format!("keychain error: {e}"))?;
    let encrypted = xor_with_key(mnemonic.as_bytes(), password.as_bytes());
    let hex_encoded = hex::encode(&encrypted);
    entry
        .set_secret(hex_encoded.as_bytes())
        .map_err(|e| format!("keychain save error: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn keystore_load(password: String) -> Result<String, String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_USER)
        .map_err(|e| format!("keychain error: {e}"))?;
    let stored = entry
        .get_password()
        .map_err(|e| format!("keychain load error: {e}"))?;
    let encrypted = hex::decode(&stored).map_err(|e| format!("decode error: {e}"))?;
    let decrypted = xor_with_key(&encrypted, password.as_bytes());
    String::from_utf8(decrypted).map_err(|e| format!("invalid stored data (wrong password?): {e}"))
}

#[tauri::command]
pub fn keystore_delete() -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_USER)
        .map_err(|e| format!("keychain error: {e}"))?;
    entry
        .delete_credential()
        .map_err(|e| format!("keychain delete error: {e}"))?;
    Ok(())
}

fn xor_with_key(data: &[u8], key: &[u8]) -> Vec<u8> {
    // L-01 NOTE: This is a simple obfuscation, not cryptographic encryption.
    // The keychain itself provides the security boundary. For true encryption,
    // use AES-256-GCM via the `aes-gcm` crate. This is a placeholder until
    // the Tauri keychain integration is hardened.
    data.iter()
        .enumerate()
        .map(|(i, b)| b ^ key[i % key.len()])
        .collect()
}
