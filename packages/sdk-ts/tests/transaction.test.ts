import { describe, it, expect } from 'vitest';
import {
  createTransfer,
  serializeOperationStripped,
  serializeOperation,
  serializeSignedOperation,
} from '../src/transaction';
import { HdWallet } from '../src/wallet';

const MNEMONIC =
  'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';

// Deterministic vectors produced by the Rust core (crates/augecoin-core +
// crates/augecoin-crypto). Lock the cross-language serialization/signing.
const STRIPPED_HEX =
  '010000000100000000000000640000000000000000000000003b9aca0000000000000100000000000000c8000000003b9aca0000000000000000000000000000000000000000000002';
const PUBKEY_HEX = '6589bfd8bbf0e34991b0cf5cf3467a2755ddf4a744809cb718b8f040cf3d780c';
const SIG_HEX =
  '76aa57e857703d30e1282ce21388bc6c025346cdf6d5b1b895ed5a4429ec469251f32f7fe8ff47dfc3ec88e5c8d20677b0060db12812304703f0716055c08601';

const hexToBytes = (hex: string): Uint8Array => {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.substr(i * 2, 2), 16);
  return out;
};

const bytesToHex = (bytes: Uint8Array): string =>
  Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');

describe('createTransfer', () => {
  it('builds a transfer operation', () => {
    const op = createTransfer(2n, 100n, 0n, 200n, 1_000_000_000n, 0n);
    expect(op.chainId).toBe(2n);
    expect(op.payload).toMatchObject({ type: 'transaction' });
  });
});

describe('serializeOperationStripped', () => {
  it('produces deterministic bytes matching the Rust core', () => {
    const op = createTransfer(2n, 100n, 0n, 200n, 1_000_000_000n, 0n);
    expect(bytesToHex(serializeOperationStripped(op))).toBe(STRIPPED_HEX);
  });

  it('differs for a different chain_id (replay protection)', () => {
    const a = createTransfer(2n, 100n, 0n, 200n, 1_000_000_000n, 0n);
    const b = createTransfer(3n, 100n, 0n, 200n, 1_000_000_000n, 0n);
    expect(serializeOperationStripped(a)).not.toEqual(serializeOperationStripped(b));
  });
});

describe('serializeOperation', () => {
  it('appends signature count and signature bytes', () => {
    const op = createTransfer(2n, 100n, 0n, 200n, 1_000_000_000n, 0n);
    const sig = hexToBytes(SIG_HEX);
    const full = serializeOperation(op, [sig]);
    const strippedLen = serializeOperationStripped(op).length;
    expect(full.length).toBe(strippedLen + 4 + 64);
    // signature count (u32 BE) = 1
    expect(new DataView(full.buffer, full.byteOffset + strippedLen).getUint32(0, false)).toBe(1);
  });
});

describe('cross-language signing parity', () => {
  it('derives the same Ed25519 key as Rust (HD)', () => {
    const wallet = HdWallet.fromMnemonic(MNEMONIC);
    expect(bytesToHex(wallet.ed25519PublicKey(0))).toBe(PUBKEY_HEX);
  });

  it('signs the stripped bytes identically to Rust', () => {
    const op = createTransfer(2n, 100n, 0n, 200n, 1_000_000_000n, 0n);
    const stripped = serializeOperationStripped(op);
    const wallet = HdWallet.fromMnemonic(MNEMONIC);
    expect(bytesToHex(wallet.sign(0, stripped))).toBe(SIG_HEX);
  });

  it('produces a full signed operation', () => {
    const op = createTransfer(2n, 100n, 0n, 200n, 1_000_000_000n, 0n);
    const stripped = serializeOperationStripped(op);
    const wallet = HdWallet.fromMnemonic(MNEMONIC);
    const sig = wallet.sign(0, stripped);
    const full = serializeSignedOperation(op, sig);
    expect(full.length).toBe(stripped.length + 4 + 64);
  });
});
