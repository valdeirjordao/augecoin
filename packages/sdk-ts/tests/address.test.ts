import { describe, it, expect } from 'vitest';
import {
  getAddress,
  validateAddress,
  addressFromPublicKeyHex,
} from '../src/address';

// Cross-language vector locked against crates/augecoin-node/tests/sdk_parity.rs
// and crates/augecoin-crypto/src/address.rs. The address is Base58 of the first
// 24 bytes of blake3-512(ed25519_pubkey) plus a 4-byte domain-separated checksum.
const PUBKEY_HEX = '6589bfd8bbf0e34991b0cf5cf3467a2755ddf4a744809cb718b8f040cf3d780c';
const ADDRESS = '274rGuUx9XozCeJ2LBXggKLp5dd31fugXWKNinW';

describe('address', () => {
  it('derives the cross-language address vector', () => {
    const pk = new Uint8Array(PUBKEY_HEX.match(/.{1,2}/g)!.map((b) => parseInt(b, 16)));
    expect(getAddress(pk)).toBe(ADDRESS);
  });

  it('derives from hex helper', () => {
    expect(addressFromPublicKeyHex(PUBKEY_HEX)).toBe(ADDRESS);
  });

  it('validates the address', () => {
    expect(validateAddress(ADDRESS)).toBe(true);
  });

  it('rejects a corrupted address', () => {
    const replacement = ADDRESS[0] === '1' ? '2' : '1';
    expect(validateAddress(replacement + ADDRESS.slice(1))).toBe(false);
  });

  it('rejects a non-base58 address', () => {
    expect(validateAddress('0')).toBe(false);
    expect(validateAddress('O')).toBe(false);
    expect(validateAddress('')).toBe(false);
  });
});
