use crate::signature::Ed25519KeyPair;
use ed25519_dalek::SigningKey;
use hmac::{Hmac, Mac};
use sha3::Sha3_512;
use std::str::FromStr;

type HmacSha3_512 = Hmac<Sha3_512>;

const HDKEY_INFO_PREFIX: &[u8] = b"augecoin-hdkey-";
const DERIVED_KEY_LEN: usize = 64;

fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; DERIVED_KEY_LEN] {
    let mut mac = HmacSha3_512::new_from_slice(salt).expect("HMAC accepts any key length");
    mac.update(ikm);
    mac.finalize().into_bytes().into()
}

fn hkdf_expand(prk: &[u8; DERIVED_KEY_LEN], info: &[u8], output_len: usize) -> Vec<u8> {
    let mut okm = Vec::with_capacity(output_len);
    let mut t_prev = Vec::new();
    let mut block_num: u8 = 1;

    while okm.len() < output_len {
        let mut mac = HmacSha3_512::new_from_slice(prk).expect("HKDF expand requires valid PRK");
        mac.update(&t_prev);
        mac.update(info);
        mac.update(&[block_num]);
        t_prev = mac.finalize().into_bytes().to_vec();
        let remaining = output_len - okm.len();
        let take = remaining.min(t_prev.len());
        okm.extend_from_slice(&t_prev[..take]);
        block_num = block_num.wrapping_add(1);
    }

    okm
}

pub struct HdWallet {
    master_seed: [u8; 64],
}

impl HdWallet {
    pub fn from_mnemonic(phrase: &str) -> Result<Self, String> {
        let mnemonic =
            bip39::Mnemonic::from_str(phrase).map_err(|e| format!("invalid mnemonic: {e}"))?;
        let seed = mnemonic.to_seed("");
        let mut master_seed = [0u8; 64];
        master_seed.copy_from_slice(&seed);
        Ok(HdWallet { master_seed })
    }

    pub fn from_seed(seed: &[u8; 64]) -> Self {
        HdWallet { master_seed: *seed }
    }

    pub fn derive_keypair(&self, index: u64) -> Ed25519KeyPair {
        let info = [HDKEY_INFO_PREFIX, &index.to_be_bytes()].concat();
        let prk = hkdf_extract(&[], &self.master_seed);
        let derived = hkdf_expand(&prk, &info, DERIVED_KEY_LEN);

        let ed25519_seed: [u8; 32] = derived[..32].try_into().unwrap();

        let signing = SigningKey::from_bytes(&ed25519_seed);
        let verifying = signing.verifying_key();

        Ed25519KeyPair::from_components(signing, verifying)
    }
}

impl Ed25519KeyPair {
    pub(crate) fn from_components(
        signing: SigningKey,
        verifying: ed25519_dalek::VerifyingKey,
    ) -> Self {
        Ed25519KeyPair { signing, verifying }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct HdKeyVector {
    pub mnemonic: String,
    pub index: u64,
    pub ed25519_public_key_hex: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn same_seed_same_index_same_key() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let kp1 = wallet.derive_keypair(0);
        let kp2 = wallet.derive_keypair(0);

        assert_eq!(
            kp1.verifying_key().to_bytes(),
            kp2.verifying_key().to_bytes()
        );
    }

    #[test]
    fn different_index_different_key() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let kp0 = wallet.derive_keypair(0);
        let kp1 = wallet.derive_keypair(1);

        assert_ne!(
            kp0.verifying_key().to_bytes(),
            kp1.verifying_key().to_bytes()
        );
    }

    #[test]
    fn same_mnemonic_same_wallet() {
        let w1 = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let w2 = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let pk1 = w1.derive_keypair(5).verifying_key();
        let pk2 = w2.derive_keypair(5).verifying_key();

        assert_eq!(pk1.to_bytes(), pk2.to_bytes());
    }
}
