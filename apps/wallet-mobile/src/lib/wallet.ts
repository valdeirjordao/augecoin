import { generateMnemonic, validateMnemonic, mnemonicToSeedSync } from 'bip39';

export interface WalletData {
  mnemonic: string;
  seed: Uint8Array;
  createdAt: number;
}

export function createWallet(): WalletData {
  const mnemonic = generateMnemonic(128);
  const seedBuffer = mnemonicToSeedSync(mnemonic);
  return {
    mnemonic,
    seed: new Uint8Array(seedBuffer),
    createdAt: Date.now(),
  };
}

export function isValidMnemonic(phrase: string): boolean {
  return validateMnemonic(phrase.trim());
}

export function importWallet(mnemonic: string): WalletData {
  if (!validateMnemonic(mnemonic.trim())) {
    throw new Error('Seed invalida');
  }
  const seedBuffer = mnemonicToSeedSync(mnemonic.trim());
  return {
    mnemonic: mnemonic.trim(),
    seed: new Uint8Array(seedBuffer),
    createdAt: Date.now(),
  };
}

export function getMnemonicWord(mnemonic: string, index: number): string {
  const words = mnemonic.split(' ');
  if (index < 0 || index >= words.length) {
    throw new Error('Indice fora do intervalo');
  }
  return words[index];
}

export function getWordCount(mnemonic: string): number {
  return mnemonic.split(' ').length;
}
