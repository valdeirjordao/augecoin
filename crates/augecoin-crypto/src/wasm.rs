use crate::hdkeys::HdWallet;
use crate::signature::Ed25519KeyPair;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmKeyPair {
    inner: Ed25519KeyPair,
}

#[wasm_bindgen]
impl WasmKeyPair {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        WasmKeyPair {
            inner: Ed25519KeyPair::generate(),
        }
    }

    pub fn ed25519_public_key_hex(&self) -> String {
        hex::encode(self.inner.verifying_key().to_bytes())
    }

    pub fn sign(&self, message: &str) -> String {
        let sig = self.inner.sign(message.as_bytes());
        hex::encode(sig.bytes)
    }

    /// Sign the *bytes* encoded by `message_hex` (hex string), returning the
    /// signature as a hex string. This is the canonical entry point for
    /// signing arbitrary binary messages (e.g. serialized operations).
    pub fn sign_hex(&self, message_hex: &str) -> Result<String, String> {
        let bytes = hex::decode(message_hex).map_err(|e| format!("invalid hex: {e}"))?;
        let sig = self.inner.sign(&bytes);
        Ok(hex::encode(sig.bytes))
    }

    pub fn verify(&self, message: &str, signature_hex: &str) -> bool {
        let sig_bytes = match hex::decode(signature_hex) {
            Ok(b) => b,
            Err(_) => return false,
        };

        if sig_bytes.len() != 64 {
            return false;
        }

        let mut arr = [0u8; 64];
        arr.copy_from_slice(&sig_bytes);
        let signature = crate::signature::Ed25519Signature { bytes: arr };
        signature.verify(&self.inner.verifying_key(), message.as_bytes())
    }

    /// Verify a signature over the *bytes* encoded by `message_hex`.
    pub fn verify_hex(&self, message_hex: &str, signature_hex: &str) -> bool {
        let message_bytes = match hex::decode(message_hex) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sig_bytes = match hex::decode(signature_hex) {
            Ok(b) => b,
            Err(_) => return false,
        };
        if sig_bytes.len() != 64 {
            return false;
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&sig_bytes);
        let signature = crate::signature::Ed25519Signature { bytes: arr };
        signature.verify(&self.inner.verifying_key(), &message_bytes)
    }
}

impl Default for WasmKeyPair {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
pub struct WasmHdWallet {
    wallet: HdWallet,
}

#[wasm_bindgen]
impl WasmHdWallet {
    #[wasm_bindgen(constructor)]
    pub fn new(mnemonic: &str) -> Result<WasmHdWallet, String> {
        let wallet =
            HdWallet::from_mnemonic(mnemonic).map_err(|e| format!("invalid mnemonic: {e}"))?;
        Ok(WasmHdWallet { wallet })
    }

    pub fn ed25519_public_key_hex(&self, index: u64) -> String {
        let kp = self.wallet.derive_keypair(index);
        hex::encode(kp.verifying_key().to_bytes())
    }

    pub fn sign(&self, index: u64, message: &str) -> String {
        let kp = self.wallet.derive_keypair(index);
        let sig = kp.sign(message.as_bytes());
        hex::encode(sig.bytes)
    }

    /// Sign the *bytes* encoded by `message_hex` (hex string) at the given index.
    pub fn sign_hex(&self, index: u64, message_hex: &str) -> Result<String, String> {
        let bytes = hex::decode(message_hex).map_err(|e| format!("invalid hex: {e}"))?;
        let kp = self.wallet.derive_keypair(index);
        let sig = kp.sign(&bytes);
        Ok(hex::encode(sig.bytes))
    }
}
