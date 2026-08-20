import { bech32m } from 'bech32';
import { blake3 } from '@noble/hashes/blake3';

const HRP = 'auge';
// bech32m (BIP-350) allows up to 1023 characters; the default 90-character
// limit in the `bech32` JS library is a segwit-era restriction and would
// reject AUGECOIN's 64-byte hash addresses. Match the Rust encoder exactly.
const BECH32M_LIMIT = 1023;
const SHORT_ADDRESS_PAYLOAD_LENGTH = 24;
const SHORT_ADDRESS_CHECKSUM_LENGTH = 4;
const SHORT_ADDRESS_DOMAIN = new TextEncoder().encode('AUGECOIN-SHORT-ADDRESS-V1');
const BASE58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

export function getAddress(ed25519PublicKey: Uint8Array): string {
  const hash = blake3(ed25519PublicKey, { dkLen: 64 });
  const words = bech32m.toWords(hash);
  return bech32m.encode(HRP, words, BECH32M_LIMIT);
}

export function validateAddress(address: string): boolean {
  try {
    const { prefix, words } = bech32m.decode(address, BECH32M_LIMIT);
    if (prefix !== HRP) return false;
    const data = bech32m.fromWords(words);
    return data.length === 64;
  } catch {
    return false;
  }
}

export function addressFromPublicKeyHex(publicKeyHex: string): string {
  const bytes = new Uint8Array(publicKeyHex.match(/.{1,2}/g)!.map(b => parseInt(b, 16)));
  return getAddress(bytes);
}

export function getShortAddress(ed25519PublicKey: Uint8Array): string {
  const hash = blake3(ed25519PublicKey, { dkLen: 64 });
  const payload = hash.slice(0, SHORT_ADDRESS_PAYLOAD_LENGTH);
  const checksum = getShortChecksum(payload);
  const bytes = new Uint8Array(payload.length + checksum.length);
  bytes.set(payload);
  bytes.set(checksum, payload.length);
  return encodeBase58(bytes);
}

export function validateShortAddress(address: string): boolean {
  const bytes = decodeBase58(address);
  if (!bytes || bytes.length !== SHORT_ADDRESS_PAYLOAD_LENGTH + SHORT_ADDRESS_CHECKSUM_LENGTH) return false;
  const expected = getShortChecksum(bytes.slice(0, SHORT_ADDRESS_PAYLOAD_LENGTH));
  return bytes.slice(SHORT_ADDRESS_PAYLOAD_LENGTH).every((byte, index) => byte === expected[index]);
}

export function shortAddressFromPublicKeyHex(publicKeyHex: string): string {
  const bytes = new Uint8Array(publicKeyHex.match(/.{1,2}/g)!.map(b => parseInt(b, 16)));
  return getShortAddress(bytes);
}

function getShortChecksum(payload: Uint8Array): Uint8Array {
  const input = new Uint8Array(SHORT_ADDRESS_DOMAIN.length + payload.length);
  input.set(SHORT_ADDRESS_DOMAIN);
  input.set(payload, SHORT_ADDRESS_DOMAIN.length);
  return blake3(input, { dkLen: SHORT_ADDRESS_CHECKSUM_LENGTH });
}

function encodeBase58(bytes: Uint8Array): string {
  const digits = [0];
  for (const byte of bytes) {
    let carry = byte;
    for (let index = 0; index < digits.length; index++) {
      const value = digits[index] * 256 + carry;
      digits[index] = value % 58;
      carry = Math.floor(value / 58);
    }
    while (carry > 0) {
      digits.push(carry % 58);
      carry = Math.floor(carry / 58);
    }
  }
  const firstNonZero = [...bytes].findIndex(byte => byte !== 0);
  const zeroCount = firstNonZero === -1 ? bytes.length : firstNonZero;
  return '1'.repeat(zeroCount) + digits.reverse().map(digit => BASE58[digit]).join('');
}

function decodeBase58(value: string): Uint8Array | null {
  if (!value) return null;
  const bytes = [0];
  for (const character of value) {
    const digit = BASE58.indexOf(character);
    if (digit < 0) return null;
    let carry = digit;
    for (let index = 0; index < bytes.length; index++) {
      const current = bytes[index] * 58 + carry;
      bytes[index] = current % 256;
      carry = Math.floor(current / 256);
    }
    while (carry > 0) {
      bytes.push(carry % 256);
      carry = Math.floor(carry / 256);
    }
  }
  const firstNonOne = [...value].findIndex(character => character !== '1');
  const oneCount = firstNonOne === -1 ? value.length : firstNonOne;
  bytes.push(...new Array(oneCount).fill(0));
  bytes.reverse();
  return new Uint8Array(bytes);
}
