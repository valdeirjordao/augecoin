// AUGECOIN Wallet — WASM crypto loader + HD wallet helpers.
//
// Loads the prebuilt augecoin_crypto WASM (blake3 + Ed25519 + BIP39 HD key
// derivation, byte-for-byte identical to the Rust core) and exposes the
// minimal surface the wallet needs.

import * as bg from './wasm/augecoin_crypto_bg.js';

let wasmExports = null;
let initPromise = null;

async function initWasm() {
  if (initPromise) return initPromise;
  initPromise = (async () => {
    const url = new URL('./wasm/augecoin_crypto_bg.wasm', import.meta.url);
    const res = await fetch(url);
    if (!res.ok) throw new Error('failed to fetch wasm crypto');
    const bytes = await res.arrayBuffer();
    const { instance } = await WebAssembly.instantiate(bytes, { './augecoin_crypto_bg.js': bg });
    bg.__wbg_set_wasm(instance.exports);
    if (instance.exports.__wbindgen_start) instance.exports.__wbindgen_start();
    wasmExports = instance.exports;
    return instance.exports;
  })();
  return initPromise;
}

/**
 * Derive the Ed25519 public key (hex) for the account at `index`.
 * Throws if the mnemonic is invalid.
 */
export async function derivePublicKeyHex(mnemonic, index = 0) {
  await initWasm();
  const seedPhrase = String(mnemonic).trim().split(/\s+/).slice(0, 12).join(' ');
  const wallet = new bg.WasmHdWallet(seedPhrase);
  const pub = wallet.ed25519_public_key_hex(BigInt(index));
  wallet.free();
  return pub;
}

/**
 * Sign the *bytes* encoded by `messageHex` with the account key at `index`.
 * Returns a 128-char hex signature.
 */
export async function signMessageHex(mnemonic, index, messageHex) {
  await initWasm();
  const seedPhrase = String(mnemonic).trim().split(/\s+/).slice(0, 12).join(' ');
  const wallet = new bg.WasmHdWallet(seedPhrase);
  const sig = wallet.sign_hex(BigInt(index), messageHex);
  wallet.free();
  return sig;
}

export { initWasm };
