import { generateMnemonic, validateMnemonic, mnemonicToSeedSync } from 'bip39';

export interface WalletData {
  mnemonic: string;
  seed: Uint8Array;
  createdAt: number;
}

/** Generate a new BIP39 wallet (12-word mnemonic). */
export function createWallet(): WalletData {
  const mnemonic = generateMnemonic(128);
  const seedBuffer = mnemonicToSeedSync(mnemonic);
  return {
    mnemonic,
    seed: new Uint8Array(seedBuffer),
    createdAt: Date.now(),
  };
}

/** Validate a BIP39 mnemonic phrase. */
export function isValidMnemonic(phrase: string): boolean {
  return validateMnemonic(phrase.trim());
}

/** Import wallet from mnemonic. */
export function importWallet(mnemonic: string): WalletData {
  if (!validateMnemonic(mnemonic.trim())) {
    throw new Error('Invalid mnemonic phrase');
  }
  const seedBuffer = mnemonicToSeedSync(mnemonic.trim());
  return {
    mnemonic: mnemonic.trim(),
    seed: new Uint8Array(seedBuffer),
    createdAt: Date.now(),
  };
}
