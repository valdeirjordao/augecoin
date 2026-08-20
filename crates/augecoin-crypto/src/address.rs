use crate::hash::blake3_512;
use bech32::{Bech32m, Hrp};
use ed25519_dalek::VerifyingKey;

const ADDRESS_PREFIX: &str = "auge";
const SHORT_ADDRESS_PAYLOAD_LEN: usize = 24;
const SHORT_ADDRESS_CHECKSUM_LEN: usize = 4;
const SHORT_ADDRESS_DOMAIN: &[u8] = b"AUGECOIN-SHORT-ADDRESS-V1";
const BASE58: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

pub fn derive_address(verifying_key: &VerifyingKey) -> String {
    let hash = blake3_512(&verifying_key.to_bytes());
    let hrp = Hrp::parse(ADDRESS_PREFIX).expect("auge is a valid HRP");
    bech32::encode::<Bech32m>(hrp, &hash).expect("bech32m encoding should succeed")
}

pub fn validate_address(address: &str) -> bool {
    let (hrp, data) = match bech32::decode(address) {
        Ok(result) => result,
        Err(_) => return false,
    };

    if hrp.as_str() != ADDRESS_PREFIX {
        return false;
    }

    data.len() == 64
}

/// Returns the compact external-payment representation of an address key.
/// The canonical `auge1...` address remains the protocol identity.
pub fn derive_short_address(verifying_key: &VerifyingKey) -> String {
    let hash = blake3_512(&verifying_key.to_bytes());
    let mut payload = [0u8; SHORT_ADDRESS_PAYLOAD_LEN];
    payload.copy_from_slice(&hash[..SHORT_ADDRESS_PAYLOAD_LEN]);
    let checksum = short_address_checksum(&payload);

    let mut bytes = [0u8; SHORT_ADDRESS_PAYLOAD_LEN + SHORT_ADDRESS_CHECKSUM_LEN];
    bytes[..SHORT_ADDRESS_PAYLOAD_LEN].copy_from_slice(&payload);
    bytes[SHORT_ADDRESS_PAYLOAD_LEN..].copy_from_slice(&checksum);
    base58_encode(&bytes)
}

pub fn validate_short_address(address: &str) -> bool {
    let bytes = match base58_decode(address) {
        Some(bytes) if bytes.len() == SHORT_ADDRESS_PAYLOAD_LEN + SHORT_ADDRESS_CHECKSUM_LEN => {
            bytes
        }
        _ => return false,
    };

    let payload = &bytes[..SHORT_ADDRESS_PAYLOAD_LEN];
    bytes[SHORT_ADDRESS_PAYLOAD_LEN..] == short_address_checksum(payload)
}

fn short_address_checksum(payload: &[u8]) -> [u8; SHORT_ADDRESS_CHECKSUM_LEN] {
    let mut input = Vec::with_capacity(SHORT_ADDRESS_DOMAIN.len() + payload.len());
    input.extend_from_slice(SHORT_ADDRESS_DOMAIN);
    input.extend_from_slice(payload);
    let hash = blake3_512(&input);
    let mut checksum = [0u8; SHORT_ADDRESS_CHECKSUM_LEN];
    checksum.copy_from_slice(&hash[..SHORT_ADDRESS_CHECKSUM_LEN]);
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
    fn corrupted_address_rejected() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let pk = wallet.derive_keypair(0).verifying_key();
        let mut address = derive_address(&pk);
        address.push('x');
        assert!(!validate_address(&address));
    }

    #[test]
    fn wrong_prefix_rejected() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let pk = wallet.derive_keypair(0).verifying_key();
        let address = derive_address(&pk);
        let wrong = address.replace("auge1", "bc1p");
        assert!(!validate_address(&wrong));
    }

    #[test]
    fn short_address_round_trip() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let key = wallet.derive_keypair(0).verifying_key();
        let address = derive_short_address(&key);
        assert!(address.len() >= 32 && address.len() <= 42);
        assert!(!address.starts_with("auge1"));
        assert!(validate_short_address(&address));
    }

    #[test]
    fn short_address_cross_language_vector() {
        let public_key = [
            0x65, 0x89, 0xbf, 0xd8, 0xbb, 0xf0, 0xe3, 0x49, 0x91, 0xb0, 0xcf, 0x5c, 0xf3, 0x46,
            0x7a, 0x27, 0x55, 0xdd, 0xf4, 0xa7, 0x44, 0x80, 0x9c, 0xb7, 0x18, 0xb8, 0xf0, 0x40,
            0xcf, 0x3d, 0x78, 0x0c,
        ];
        let key = VerifyingKey::from_bytes(&public_key).unwrap();
        assert_eq!(
            derive_short_address(&key),
            "274rGuUx9XozCeJ2LBXggKLp5dd31fugXWKNinW"
        );
    }

    #[test]
    fn short_address_corruption_is_rejected() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let key = wallet.derive_keypair(0).verifying_key();
        let mut address = derive_short_address(&key).into_bytes();
        address[0] = if address[0] == b'1' { b'2' } else { b'1' };
        assert!(!validate_short_address(
            std::str::from_utf8(&address).unwrap()
        ));
    }

    #[test]
    fn short_addresses_differ_for_different_keys() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let key0 = wallet.derive_keypair(0).verifying_key();
        let key1 = wallet.derive_keypair(1).verifying_key();
        assert_ne!(derive_short_address(&key0), derive_short_address(&key1));
    }
}
