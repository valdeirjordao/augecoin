import { describe, it, expect } from 'vitest';
import { HdWallet, WasmKeyPair } from '../src/wallet';

const MNEMONIC =
  'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';

describe('HdWallet', () => {
  it('rejects an invalid mnemonic', () => {
    expect(() => HdWallet.fromMnemonic('not a valid mnemonic phrase')).toThrow();
  });

  it('derives deterministic keys for a known mnemonic', () => {
    const w = HdWallet.fromMnemonic(MNEMONIC);
    const k1 = w.ed25519PublicKey(0);
    const k2 = w.ed25519PublicKey(0);
    expect(k1).toEqual(k2);
    expect(k1).toHaveLength(32);
  });

  it('derives different keys for different indices', () => {
    const w = HdWallet.fromMnemonic(MNEMONIC);
    expect(w.ed25519PublicKey(0)).not.toEqual(w.ed25519PublicKey(1));
  });

  it('signs arbitrary bytes and returns a 64-byte signature', () => {
    const w = HdWallet.fromMnemonic(MNEMONIC);
    const sig = w.sign(0, new Uint8Array([1, 2, 3, 4]));
    expect(sig).toHaveLength(64);
  });

  it('deterministic signature for the same input', () => {
    const w = HdWallet.fromMnemonic(MNEMONIC);
    const msg = new Uint8Array([9, 8, 7]);
    expect(w.sign(0, msg)).toEqual(w.sign(0, msg));
  });
});

describe('WasmKeyPair', () => {
  it('generates a key and signs a message', () => {
    const kp = new WasmKeyPair();
    expect(kp.ed25519PublicKey).toHaveLength(32);
    const sig = kp.sign(new Uint8Array([1, 2, 3]));
    expect(sig).toHaveLength(64);
  });

  it('generates distinct keys', () => {
    const a = new WasmKeyPair();
    const b = new WasmKeyPair();
    expect(a.ed25519PublicKey).not.toEqual(b.ed25519PublicKey);
  });
});
