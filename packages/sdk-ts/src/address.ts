import { blake3 } from '@noble/hashes/blake3';

const ADDRESS_PAYLOAD_LENGTH = 24;
const ADDRESS_CHECKSUM_LENGTH = 4;
const ADDRESS_DOMAIN = new TextEncoder().encode('AUGECOIN-SHORT-ADDRESS-V1');
const BASE58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

export function getAddress(ed25519PublicKey: Uint8Array): string {
  const hash = blake3(ed25519PublicKey, { dkLen: 64 });
  const payload = hash.slice(0, ADDRESS_PAYLOAD_LENGTH);
  const checksum = getChecksum(payload);
  const bytes = new Uint8Array(payload.length + checksum.length);
  bytes.set(payload);
  bytes.set(checksum, payload.length);
  return encodeBase58(bytes);
}

export function validateAddress(address: string): boolean {
  const bytes = decodeBase58(address);
  if (!bytes || bytes.length !== ADDRESS_PAYLOAD_LENGTH + ADDRESS_CHECKSUM_LENGTH) return false;
  const expected = getChecksum(bytes.slice(0, ADDRESS_PAYLOAD_LENGTH));
  return bytes.slice(ADDRESS_PAYLOAD_LENGTH).every((byte, index) => byte === expected[index]);
}

export function addressFromPublicKeyHex(publicKeyHex: string): string {
  const bytes = new Uint8Array(publicKeyHex.match(/.{1,2}/g)!.map(b => parseInt(b, 16)));
  return getAddress(bytes);
}

function getChecksum(payload: Uint8Array): Uint8Array {
  const input = new Uint8Array(ADDRESS_DOMAIN.length + payload.length);
  input.set(ADDRESS_DOMAIN);
  input.set(payload, ADDRESS_DOMAIN.length);
  return blake3(input, { dkLen: ADDRESS_CHECKSUM_LENGTH });
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
