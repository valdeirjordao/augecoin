// AUGECOIN Wallet Web — WASM crypto (Ed25519 signing, HD key derivation).
//
// The WASM module is loaded lazily so that importing this module (e.g. in a
// test) does not instantiate WebAssembly until a crypto operation actually runs.

import type { WasmHdWallet } from '@augecoin/wasm-crypto';

type WasmModule = {
  WasmHdWallet: new (mnemonic: string) => WasmHdWallet;
};

let wasmPromise: Promise<WasmModule> | null = null;

function loadWasm(): Promise<WasmModule> {
  if (!wasmPromise) {
    wasmPromise = import('@augecoin/wasm-crypto').then((m) => m as unknown as WasmModule);
  }
  return wasmPromise;
}

/** Derive the Ed25519 public key (32-byte hex) for the account at `index`. */
export async function derivePublicKeyHex(mnemonic: string, index = 0): Promise<string> {
  const { WasmHdWallet: W } = await loadWasm();
  const wallet = new W(mnemonic);
  try {
    return wallet.ed25519_public_key_hex(BigInt(index));
  } finally {
    wallet.free();
  }
}

/** Sign the *bytes* encoded by `messageHex`, returning a 128-char hex signature. */
export async function signMessageHex(mnemonic: string, index: number, messageHex: string): Promise<string> {
  const { WasmHdWallet: W } = await loadWasm();
  const wallet = new W(mnemonic);
  try {
    return wallet.sign_hex(BigInt(index), messageHex);
  } finally {
    wallet.free();
  }
}
