import { describe, it, expect, beforeEach, vi } from 'vitest';
import { createWallet, isValidMnemonic, importWallet } from '../src/lib/wallet';

describe('createWallet', () => {
  it('generates a 12-word mnemonic', () => {
    const wallet = createWallet();
    expect(wallet.mnemonic.split(' ')).toHaveLength(12);
  });

  it('generates a 64-byte seed', () => {
    const wallet = createWallet();
    expect(wallet.seed.length).toBe(64);
  });

  it('sets createdAt timestamp', () => {
    const before = Date.now();
    const wallet = createWallet();
    expect(wallet.createdAt).toBeGreaterThanOrEqual(before);
  });

  it('generates different wallets each time', () => {
    const w1 = createWallet();
    const w2 = createWallet();
    expect(w1.mnemonic).not.toBe(w2.mnemonic);
  });
});

describe('isValidMnemonic', () => {
  it('validates a real mnemonic', () => {
    const wallet = createWallet();
    expect(isValidMnemonic(wallet.mnemonic)).toBe(true);
  });

  it('rejects invalid phrases', () => {
    expect(isValidMnemonic('not valid')).toBe(false);
  });

  it('accepts trimmed whitespace', () => {
    const wallet = createWallet();
    expect(isValidMnemonic('  ' + wallet.mnemonic + '  ')).toBe(true);
  });
});

describe('importWallet', () => {
  it('imports from valid mnemonic', () => {
    const original = createWallet();
    const imported = importWallet(original.mnemonic);
    expect(imported.mnemonic).toBe(original.mnemonic);
    expect(imported.seed).toEqual(original.seed);
  });

  it('throws on invalid mnemonic', () => {
    expect(() => importWallet('bad phrase')).toThrow();
  });
});
