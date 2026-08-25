//! License key generation, canonicalization, hashing and validation.
//!
//! Format: 32 characters of Crockford Base32 (excludes `I`, `L`, `O`, `U` to
//! avoid visual ambiguity), grouped as 8 blocks of 4 separated by `-`:
//!
//! ```text
//! 8F2K-X91M-A7QP-5NLD-3R8C-HJ4T-Z6WV-PQ2X
//! ```
//!
//! A key encodes 20 random bytes (160 bits of entropy). The canonical form used
//! for hashing is the uppercase 32-character string **without** dashes. Only the
//! BLAKE3-256 hash of the canonical key is ever persisted.

use blake3::Hash;
use thiserror::Error;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const RAW_LEN: usize = 20;
const ENCODED_LEN: usize = 32;
const GROUP_SIZE: usize = 4;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeyError {
    #[error("license key must be 32 characters ({ENCODED_LEN} after removing dashes)")]
    InvalidLength,
    #[error("license key contains invalid characters")]
    InvalidCharacters,
}

/// A validated license key (holds the canonical 32-char, uppercase, dash-free form).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicenseKey(String);

impl LicenseKey {
    /// Generate a new random license key from the OS CSPRNG.
    pub fn generate() -> Self {
        let mut bytes = [0u8; RAW_LEN];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
        Self::from_raw(&bytes)
    }

    /// Build a key from raw entropy bytes (deterministic for tests/tooling).
    pub fn from_raw(bytes: &[u8]) -> Self {
        debug_assert_eq!(bytes.len(), RAW_LEN);
        LicenseKey(encode_base32(bytes))
    }

    /// Parse and validate an arbitrary user-supplied string.
    pub fn parse(input: &str) -> std::result::Result<Self, KeyError> {
        let canonical = canonicalize(input);
        if canonical.len() != ENCODED_LEN {
            return Err(KeyError::InvalidLength);
        }
        if !canonical.bytes().all(is_alphabet) {
            return Err(KeyError::InvalidCharacters);
        }
        Ok(LicenseKey(canonical))
    }

    /// Uppercase, dash-free canonical form.
    pub fn canonical(&self) -> &str {
        &self.0
    }

    /// First 4-character group, used for display and admin lookups.
    pub fn prefix(&self) -> &str {
        &self.0[..GROUP_SIZE]
    }

    /// Human-readable grouped form `XXXX-XXXX-...`.
    pub fn display(&self) -> String {
        self.0
            .as_bytes()
            .chunks(GROUP_SIZE)
            .map(|c| std::str::from_utf8(c).expect("ascii"))
            .collect::<Vec<_>>()
            .join("-")
    }

    /// BLAKE3-256 hash of the canonical key (the only persisted representation).
    pub fn hash(&self) -> Hash {
        blake3::hash(self.0.as_bytes())
    }

    pub fn hash_hex(&self) -> String {
        self.hash().to_hex().to_string()
    }
}

/// Uppercase and strip dashes/spaces; leaves the rest for validation.
fn canonicalize(input: &str) -> String {
    input
        .trim()
        .to_ascii_uppercase()
        .chars()
        .filter(|c| *c != '-' && !c.is_whitespace())
        .collect()
}

fn is_alphabet(c: u8) -> bool {
    ALPHABET.contains(&c)
}

/// Encode raw bytes to Crockford Base32 (big-endian bit packing, no padding).
fn encode_base32(bytes: &[u8]) -> String {
    let mut out = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut acc: u64 = 0;
    let mut bits: u32 = 0;
    for &b in bytes {
        acc = (acc << 8) | b as u64;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((acc >> bits) & 0x1f) as usize;
            out.push(ALPHABET[idx] as char);
        }
    }
    if bits > 0 {
        let idx = ((acc << (5 - bits)) & 0x1f) as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_zero_bytes_is_all_zeros() {
        assert_eq!(encode_base32(&[0u8; RAW_LEN]), "0".repeat(ENCODED_LEN));
    }

    #[test]
    fn raw_len_encodes_to_exactly_32_chars() {
        let k = LicenseKey::from_raw(&[0xab; RAW_LEN]);
        assert_eq!(k.canonical().len(), ENCODED_LEN);
    }

    #[test]
    fn generation_is_random_and_valid() {
        let a = LicenseKey::generate();
        let b = LicenseKey::generate();
        assert_ne!(a, b);
        assert_eq!(a.canonical().len(), ENCODED_LEN);
        assert!(a.canonical().bytes().all(is_alphabet));
    }

    #[test]
    fn parse_accepts_grouped_lowercase_and_whitespace() {
        let k = LicenseKey::generate();
        let grouped = k.display();
        let reparsed = LicenseKey::parse(&grouped.to_lowercase()).unwrap();
        assert_eq!(reparsed, k);

        let spaced = format!("  {}  ", grouped);
        assert_eq!(LicenseKey::parse(&spaced).unwrap(), k);
    }

    #[test]
    fn parse_rejects_wrong_length_and_chars() {
        assert_eq!(
            LicenseKey::parse("ABCD-EFGH-IJKL-MNOP-QRST-UVWX-YZ12").unwrap_err(),
            KeyError::InvalidLength
        );
        // 'I', 'L', 'O', 'U' are not in the Crockford alphabet.
        assert_eq!(
            LicenseKey::parse("IIII-IIII-IIII-IIII-IIII-IIII-IIII-IIII").unwrap_err(),
            KeyError::InvalidCharacters
        );
    }

    #[test]
    fn prefix_is_first_group() {
        let k = LicenseKey::from_raw(&[
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff, 0x01, 0x23, 0x45, 0x67,
        ]);
        assert_eq!(k.prefix(), &k.canonical()[..4]);
        assert!(k.display().starts_with(k.prefix()));
    }

    #[test]
    fn hash_is_deterministic_and_distinct() {
        let k = LicenseKey::from_raw(&[7u8; RAW_LEN]);
        assert_eq!(k.hash_hex(), k.hash_hex());
        let other = LicenseKey::from_raw(&[8u8; RAW_LEN]);
        assert_ne!(k.hash_hex(), other.hash_hex());
    }
}
