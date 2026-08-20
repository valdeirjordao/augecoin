import { describe, it, expect } from 'vitest';
import {
  getAddress,
  validateAddress,
  addressFromPublicKeyHex,
  getShortAddress,
  validateShortAddress,
  shortAddressFromPublicKeyHex,
} from '../src/address';

// Cross-language vector locked against crates/augecoin-node/tests/sdk_parity.rs
// and crates/augecoin-crypto/src/address.rs. The address is
// bech32m("auge", blake3-512(ed25519_pubkey)) with the BIP-350 1023-char limit.
const PUBKEY_HEX = '6589bfd8bbf0e34991b0cf5cf3467a2755ddf4a744809cb718b8f040cf3d780c';
const ADDRESS = 'auge1ddj5hphrcz0snh0ctvp6300rf6rhre4yuv4jeltgg60zq3svdcvqgf8k2jqf2a9x0qjngue84vf9u593k2dxschd4t8l2nd4g9hzjlqm2ysay';

describe('address', () => {
  it('derives the cross-language address vector', () => {
    const pk = new Uint8Array(PUBKEY_HEX.match(/.{1,2}/g)!.map((b) => parseInt(b, 16)));
    expect(getAddress(pk)).toBe(ADDRESS);
  });

  it('derives from hex helper', () => {
    expect(addressFromPublicKeyHex(PUBKEY_HEX)).toBe(ADDRESS);
  });

  it('validates the address (1023-char bech32m limit)', () => {
    expect(validateAddress(ADDRESS)).toBe(true);
  });

  it('rejects a wrong prefix', () => {
    expect(validateAddress(ADDRESS.replace('auge1', 'bc1p'))).toBe(false);
  });

  it('generates a compact external address with a valid checksum', () => {
    const pk = new Uint8Array(PUBKEY_HEX.match(/.{1,2}/g)!.map((b) => parseInt(b, 16)));
    const address = getShortAddress(pk);
    expect(address.length).toBeGreaterThanOrEqual(32);
    expect(address.length).toBeLessThanOrEqual(42);
    expect(address.startsWith('auge1')).toBe(false);
    expect(validateShortAddress(address)).toBe(true);
    expect(shortAddressFromPublicKeyHex(PUBKEY_HEX)).toBe(address);
    expect(address).toBe('274rGuUx9XozCeJ2LBXggKLp5dd31fugXWKNinW');
  });

  it('rejects a corrupted compact address', () => {
    const pk = new Uint8Array(PUBKEY_HEX.match(/.{1,2}/g)!.map((b) => parseInt(b, 16)));
    const address = getShortAddress(pk);
    const replacement = address[0] === '1' ? '2' : '1';
    expect(validateShortAddress(replacement + address.slice(1))).toBe(false);
  });
});
