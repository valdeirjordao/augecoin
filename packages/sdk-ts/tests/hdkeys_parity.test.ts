import { describe, it, expect } from 'vitest';
import * as path from 'path';
import * as fs from 'fs';
import * as wasmModule from '@augecoin/wasm-crypto';

/**
 * Parity test: verifies that the TypeScript SDK (via WASM) produces
 * the same HD key derivation results as the Rust core.
 *
 * Uses the SAME test vectors from crates/augecoin-crypto/tests/vectors/hdkeys.json
 * without duplicating them.
 */
describe('HD Key Parity: Rust <-> TypeScript (WASM)', () => {
  const vectorsPath = path.resolve(
    __dirname,
    '..',
    '..',
    '..',
    'crates',
    'augecoin-crypto',
    'tests',
    'vectors',
    'hdkeys.json'
  );

  const vectors: Array<{
    mnemonic: string;
    index: number;
    ed25519_public_key_hex: string;
  }> = JSON.parse(fs.readFileSync(vectorsPath, 'utf-8'));

  it('test vectors file exists and has entries', () => {
    expect(vectors.length).toBeGreaterThan(0);
    expect(vectors[0].mnemonic).toBeDefined();
    expect(vectors[0].ed25519_public_key_hex).toBeDefined();
  });

  for (const v of vectors) {
    it(`mnemonic "${v.mnemonic.split(' ')[0]}..." index=${v.index}`, () => {
      const wallet = new wasmModule.WasmHdWallet(v.mnemonic);

      const ed25519Hex = wallet.ed25519_public_key_hex(BigInt(v.index));

      expect(ed25519Hex).toBe(v.ed25519_public_key_hex);
    });
  }

  it('different indices produce different keys', () => {
    const wallet = new wasmModule.WasmHdWallet(vectors[0].mnemonic);

    const key0 = wallet.ed25519_public_key_hex(BigInt(0));
    const key1 = wallet.ed25519_public_key_hex(BigInt(1));

    expect(key0).not.toBe(key1);
  });

  it('same mnemonic and index produces deterministic key', () => {
    const w1 = new wasmModule.WasmHdWallet(vectors[0].mnemonic);
    const w2 = new wasmModule.WasmHdWallet(vectors[0].mnemonic);

    const k1 = w1.ed25519_public_key_hex(BigInt(0));
    const k2 = w2.ed25519_public_key_hex(BigInt(0));

    expect(k1).toBe(k2);
  });
});
