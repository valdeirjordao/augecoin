use crate::hash::blake3_512;
use ed25519_dalek::VerifyingKey;
use std::hash::{Hash, Hasher};

const ADDRESS_PAYLOAD_LEN: usize = 24;
pub const ADDRESS_HASH_LEN: usize = ADDRESS_PAYLOAD_LEN;

/// Canonical binary destination carried by address-based transactions.
/// The public key is included in a first-receive transaction because a hash
/// is intentionally one-way; nodes verify that it derives the advertised hash.
#[derive(Debug, Clone, Copy)]
pub struct AddressHash {
    pub hash: [u8; ADDRESS_HASH_LEN],
    /// Present only for new self-describing addresses. Legacy addresses are
    /// hash-only and remain resolvable for accounts already indexed on-chain.
    pub public_key: Option<[u8; 32]>,
}

impl PartialEq for AddressHash {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}
impl Eq for AddressHash {}
impl Hash for AddressHash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

const ADDRESS_CHECKSUM_LEN: usize = 4;
const ADDRESS_DOMAIN: &[u8] = b"AUGECOIN-SHORT-ADDRESS-V1";
const BASE58: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Canonical AUGECOIN address: Base58 of the first 24 bytes of
/// BLAKE3-512(public key) plus a 4-byte domain-separated checksum.
///
/// Each member has exactly one address, derived from their Ed25519 public key;
/// the same address receives both AUGE (coin) and AUGEID (identity).
pub fn derive_address(verifying_key: &VerifyingKey) -> String {
    let hash = blake3_512(&verifying_key.to_bytes());
    let mut payload = [0u8; ADDRESS_PAYLOAD_LEN];
    payload.copy_from_slice(&hash[..ADDRESS_PAYLOAD_LEN]);
    let checksum = address_checksum(&payload);

    let mut bytes = [0u8; ADDRESS_PAYLOAD_LEN + ADDRESS_CHECKSUM_LEN];
    bytes[..ADDRESS_PAYLOAD_LEN].copy_from_slice(&payload);
    bytes[ADDRESS_PAYLOAD_LEN..].copy_from_slice(&checksum);
    base58_encode(&bytes)
}

impl AddressHash {
    pub fn from_public_key(public_key: [u8; 32]) -> Self {
        let hash = blake3_512(&public_key);
        let mut address_hash = [0u8; ADDRESS_HASH_LEN];
        address_hash.copy_from_slice(&hash[..ADDRESS_HASH_LEN]);
        Self {
            hash: address_hash,
            public_key: Some(public_key),
        }
    }

    pub fn from_address(address: &str, public_key: [u8; 32]) -> Option<Self> {
        let expected = Self::parse(address)?;
        (expected == Self::from_public_key(public_key)).then_some(expected)
    }

    pub fn parse(address: &str) -> Option<Self> {
        let bytes = base58_decode(address)?;
        if bytes.len() == 32 + ADDRESS_CHECKSUM_LEN {
            let public_key: [u8; 32] = bytes[..32].try_into().ok()?;
            VerifyingKey::from_bytes(&public_key).ok()?;
            let checksum = address_checksum_v2(&bytes[..32]);
            if bytes[32..] != checksum {
                return None;
            }
            return Some(Self::from_public_key(public_key));
        }
        if bytes.len() != ADDRESS_HASH_LEN + ADDRESS_CHECKSUM_LEN
            || bytes[ADDRESS_HASH_LEN..] != address_checksum(&bytes[..ADDRESS_HASH_LEN])
        {
            return None;
        }
        let mut hash = [0u8; ADDRESS_HASH_LEN];
        hash.copy_from_slice(&bytes[..ADDRESS_HASH_LEN]);
        Some(Self {
            hash,
            public_key: None,
        })
    }

    pub fn to_address(&self) -> String {
        let (payload, checksum) = match self.public_key {
            Some(public_key) => (public_key.to_vec(), address_checksum_v2(&public_key)),
            None => (self.hash.to_vec(), address_checksum(&self.hash)),
        };
        let mut bytes = Vec::with_capacity(payload.len() + ADDRESS_CHECKSUM_LEN);
        bytes.extend_from_slice(&payload);
        bytes.extend_from_slice(&checksum);
        base58_encode(&bytes)
    }
}

fn address_checksum_v2(public_key: &[u8]) -> [u8; ADDRESS_CHECKSUM_LEN] {
    let mut input = Vec::with_capacity(ADDRESS_DOMAIN.len() + 2 + public_key.len());
    input.extend_from_slice(ADDRESS_DOMAIN);
    input.extend_from_slice(b"-PUBKEY");
    input.extend_from_slice(public_key);
    let hash = blake3_512(&input);
    let mut checksum = [0u8; ADDRESS_CHECKSUM_LEN];
    checksum.copy_from_slice(&hash[..ADDRESS_CHECKSUM_LEN]);
    checksum
}

pub fn derive_embedded_address(verifying_key: &VerifyingKey) -> String {
    AddressHash::from_public_key(verifying_key.to_bytes()).to_address()
}

pub fn validate_address(address: &str) -> bool {
    AddressHash::parse(address).is_some()
}

fn address_checksum(payload: &[u8]) -> [u8; ADDRESS_CHECKSUM_LEN] {
    let mut input = Vec::with_capacity(ADDRESS_DOMAIN.len() + payload.len());
    input.extend_from_slice(ADDRESS_DOMAIN);
    input.extend_from_slice(payload);
    let hash = blake3_512(&input);
    let mut checksum = [0u8; ADDRESS_CHECKSUM_LEN];
    checksum.copy_from_slice(&hash[..ADDRESS_CHECKSUM_LEN]);
    checksum
}

fn base58_encode(bytes: &[u8]) -> String {
    let mut digits = vec![0u8];
    for &byte in bytes {
        let mut carry = byte as u32;
        for digit in &mut digits {
            let value = (*digit as u32) * 256 + carry;
            *digit = (value % 58) as u8;
            carry = value / 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }

    let leading_zeroes = bytes.iter().take_while(|&&byte| byte == 0).count();
    let mut output = String::with_capacity(leading_zeroes + digits.len());
    output.extend(std::iter::repeat_n('1', leading_zeroes));
    for digit in digits.iter().rev() {
        output.push(BASE58[*digit as usize] as char);
    }
    output
}

fn base58_decode(value: &str) -> Option<Vec<u8>> {
    let mut bytes = vec![0u8];
    for character in value.bytes() {
        let digit = BASE58
            .iter()
            .position(|&candidate| candidate == character)? as u32;
        let mut carry = digit;
        for byte in &mut bytes {
            let value = (*byte as u32) * 58 + carry;
            *byte = (value % 256) as u8;
            carry = value / 256;
        }
        while carry > 0 {
            bytes.push((carry % 256) as u8);
            carry /= 256;
        }
    }

    let leading_ones = value.bytes().take_while(|&byte| byte == b'1').count();
    bytes.extend(std::iter::repeat_n(0, leading_ones));
    bytes.reverse();
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hdkeys::HdWallet;

    const TEST_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn different_keypairs_produce_different_addresses() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let pk0 = wallet.derive_keypair(0).verifying_key();
        let pk1 = wallet.derive_keypair(1).verifying_key();
        let addr0 = derive_address(&pk0);
        let addr1 = derive_address(&pk1);
        assert_ne!(addr0, addr1);
    }

    #[test]
    fn valid_address_passes_validation() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let pk = wallet.derive_keypair(0).verifying_key();
        let address = derive_address(&pk);
        assert!(validate_address(&address));
    }

    #[test]
    fn embedded_address_recovers_public_key() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let key = wallet.derive_keypair(0).verifying_key();
        let address = derive_embedded_address(&key);
        let parsed = AddressHash::parse(&address).unwrap();
        assert_eq!(parsed.public_key, Some(key.to_bytes()));
        assert!(validate_address(&address));
    }

    #[test]
    fn corrupted_address_rejected() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let pk = wallet.derive_keypair(0).verifying_key();
        let mut address = derive_address(&pk);
        address.push('x');
        assert!(!validate_address(&address));
    }

    #[test]
    fn address_round_trip() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let key = wallet.derive_keypair(0).verifying_key();
        let address = derive_address(&key);
        assert!(address.len() >= 32 && address.len() <= 42);
        assert!(validate_address(&address));
    }

    #[test]
    fn address_cross_language_vector() {
        let public_key = [
            0x65, 0x89, 0xbf, 0xd8, 0xbb, 0xf0, 0xe3, 0x49, 0x91, 0xb0, 0xcf, 0x5c, 0xf3, 0x46,
            0x7a, 0x27, 0x55, 0xdd, 0xf4, 0xa7, 0x44, 0x80, 0x9c, 0xb7, 0x18, 0xb8, 0xf0, 0x40,
            0xcf, 0x3d, 0x78, 0x0c,
        ];
        let key = VerifyingKey::from_bytes(&public_key).unwrap();
        assert_eq!(
            derive_address(&key),
            "274rGuUx9XozCeJ2LBXggKLp5dd31fugXWKNinW"
        );
    }

    #[test]
    fn address_corruption_is_rejected() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let key = wallet.derive_keypair(0).verifying_key();
        let mut address = derive_address(&key).into_bytes();
        address[0] = if address[0] == b'1' { b'2' } else { b'1' };
        assert!(!validate_address(std::str::from_utf8(&address).unwrap()));
    }

    #[test]
    fn addresses_differ_for_different_keys() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let key0 = wallet.derive_keypair(0).verifying_key();
        let key1 = wallet.derive_keypair(1).verifying_key();
        assert_ne!(derive_address(&key0), derive_address(&key1));
    }

    #[test]
    fn invalid_base58_character_rejected() {
        // '0', 'O', 'I' and 'l' are not part of the Base58 alphabet.
        assert!(!validate_address("0"));
        assert!(!validate_address("O"));
        assert!(!validate_address("I"));
        assert!(!validate_address("l"));
        assert!(!validate_address(""));
    }
}
