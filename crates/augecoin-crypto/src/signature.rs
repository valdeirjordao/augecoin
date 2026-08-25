use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};

#[derive(Clone)]
pub struct Ed25519KeyPair {
    pub(crate) signing: SigningKey,
    pub(crate) verifying: VerifyingKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ed25519Signature {
    pub bytes: [u8; 64],
}

impl Ed25519KeyPair {
    pub fn generate() -> Self {
        let signing = SigningKey::generate(&mut rand::thread_rng());
        let verifying = signing.verifying_key();
        Ed25519KeyPair { signing, verifying }
    }

    pub fn from_seed(seed: [u8; 32]) -> Self {
        let signing = SigningKey::from_bytes(&seed);
        let verifying = signing.verifying_key();
        Ed25519KeyPair { signing, verifying }
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.verifying
    }

    pub fn sign(&self, message: &[u8]) -> Ed25519Signature {
        let sig = self.signing.sign(message);
        Ed25519Signature {
            bytes: sig.to_bytes(),
        }
    }

    pub fn signing_key_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }
}

impl Ed25519Signature {
    pub fn verify(&self, verifying_key: &VerifyingKey, message: &[u8]) -> bool {
        let sig = ed25519_dalek::Signature::from_bytes(&self.bytes);
        verifying_key.verify(message, &sig).is_ok()
    }
}

// Legacy aliases — preserved for minimal code churn
pub type HybridKeyPair = Ed25519KeyPair;
pub type HybridPublicKey = VerifyingKey;
pub type HybridSignature = Ed25519Signature;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_signature_verifies() {
        let keypair = Ed25519KeyPair::generate();
        let vk = keypair.verifying_key();
        let message = b"augecoin test message";
        let signature = keypair.sign(message);
        assert!(signature.verify(&vk, message));
    }

    #[test]
    fn corrupted_signature_rejected() {
        let keypair = Ed25519KeyPair::generate();
        let vk = keypair.verifying_key();
        let message = b"augecoin test message";
        let mut signature = keypair.sign(message);
        signature.bytes[0] ^= 0xff;
        assert!(!signature.verify(&vk, message));
    }

    #[test]
    fn different_message_rejected() {
        let keypair = Ed25519KeyPair::generate();
        let vk = keypair.verifying_key();
        let message = b"augecoin test message";
        let signature = keypair.sign(message);
        let different_message = b"different message";
        assert!(!signature.verify(&vk, different_message));
    }

    #[test]
    fn legacy_aliases_work() {
        let kp: HybridKeyPair = Ed25519KeyPair::generate();
        let msg = b"test";
        let sig: HybridSignature = kp.sign(msg);
        let pk: &HybridPublicKey = &kp.verifying_key();
        assert!(sig.verify(pk, msg));
    }
}
