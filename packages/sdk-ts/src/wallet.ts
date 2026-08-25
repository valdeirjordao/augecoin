/**
 * HD wallet backed by the AUGECOIN WASM crypto module.
 *
 * Key derivation is deterministic and byte-compatible with the Rust core
 * (crates/augecoin-crypto/src/hdkeys.rs). Signing operates on raw bytes
 * (hex-encoded across the WASM boundary).
 */
import * as wasm from '@augecoin/wasm-crypto';

const hexToBytes = (hex: string): Uint8Array => {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.substr(i * 2, 2), 16);
  }
  return out;
};

const bytesToHex = (bytes: Uint8Array): string =>
  Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');

export class HdWallet {
  private inner: wasm.WasmHdWallet;
  readonly mnemonic: string;

  private constructor(mnemonic: string) {
    this.mnemonic = mnemonic;
    this.inner = new wasm.WasmHdWallet(mnemonic);
  }

  static fromMnemonic(mnemonic: string): HdWallet {
    return new HdWallet(mnemonic);
  }

  /** Derive the Ed25519 public key (32 bytes) at the given index. */
  ed25519PublicKey(index: number): Uint8Array {
    return hexToBytes(this.inner.ed25519_public_key_hex(BigInt(index)));
  }


  /** Derive and sign `message` (raw bytes) at the given index. Returns 64 bytes. */
  sign(index: number, message: Uint8Array): Uint8Array {
    return hexToBytes(this.inner.sign_hex(BigInt(index), bytesToHex(message)));
  }
}

export interface KeyPair {
  ed25519PublicKey: Uint8Array;
  sign(message: Uint8Array): Uint8Array;
}

/** A non-HD, freshly generated key pair. */
export class WasmKeyPair implements KeyPair {
  private inner: wasm.WasmKeyPair;

  constructor() {
    this.inner = new wasm.WasmKeyPair();
  }

  get ed25519PublicKey(): Uint8Array {
    return hexToBytes(this.inner.ed25519_public_key_hex());
  }


  sign(message: Uint8Array): Uint8Array {
    return hexToBytes(this.inner.sign_hex(bytesToHex(message)));
  }
}
