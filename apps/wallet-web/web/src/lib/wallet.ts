/**
 * HD wallet helpers for the AUGECOIN web wallet (Fase 17).
 *
 * Pure-JS key derivation (BIP39 + Ed25519) so the web build stays wasm-free.
 * The mnemonic/seed are only ever held in memory and stored encrypted via
 * {@link ./keystore}. The section only needs the derived address for display;
 * contract operations are authorized by the node admin API key, not by this
 * key (see SmartContracts section).
 */
import { generateMnemonic, validateMnemonic, mnemonicToSeedSync } from 'bip39';
import { getPublicKey } from '@noble/ed25519';
import { getAddress } from '@augecoin/sdk/dist/address.js';

export interface WalletKeys {
  mnemonic: string;
  privateKey: Uint8Array;
  publicKey: Uint8Array;
  address: string;
}

export function createWallet(): WalletKeys {
  return fromMnemonic(generateMnemonic(128));
}

export function isValidMnemonic(phrase: string): boolean {
  return validateMnemonic(phrase.trim());
}

export function importWallet(mnemonic: string): WalletKeys {
  const phrase = mnemonic.trim();
  if (!validateMnemonic(phrase)) throw new Error('seed invalida');
  return fromMnemonic(phrase);
}

function fromMnemonic(mnemonic: string): WalletKeys {
  const seed = new Uint8Array(mnemonicToSeedSync(mnemonic));
  const privateKey = seed.slice(0, 32);
  const publicKey = getPublicKey(privateKey);
  const address = getAddress(publicKey);
  return { mnemonic, privateKey, publicKey, address };
}
