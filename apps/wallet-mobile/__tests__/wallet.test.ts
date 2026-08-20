import { createWallet, importWallet, isValidMnemonic, getMnemonicWord, getWordCount } from '../src/lib/wallet';

describe('Wallet', () => {
  describe('createWallet', () => {
    it('should generate a valid 12-word mnemonic', () => {
      const wallet = createWallet();
      const words = wallet.mnemonic.split(' ');
      expect(words.length).toBe(12);
      expect(isValidMnemonic(wallet.mnemonic)).toBe(true);
    });

    it('should produce a non-empty seed', () => {
      const wallet = createWallet();
      expect(wallet.seed).toBeInstanceOf(Uint8Array);
      expect(wallet.seed.length).toBeGreaterThan(0);
    });

    it('should set createdAt timestamp', () => {
      const wallet = createWallet();
      expect(wallet.createdAt).toBeGreaterThan(0);
      expect(wallet.createdAt).toBeLessThanOrEqual(Date.now());
    });

    it('should generate different mnemonics each time', () => {
      const w1 = createWallet();
      const w2 = createWallet();
      expect(w1.mnemonic).not.toBe(w2.mnemonic);
    });

    it('should generate different seeds each time', () => {
      const w1 = createWallet();
      const w2 = createWallet();
      expect(w1.seed).not.toEqual(w2.seed);
    });
  });

  describe('isValidMnemonic', () => {
    it('should return true for a valid mnemonic', () => {
      const wallet = createWallet();
      expect(isValidMnemonic(wallet.mnemonic)).toBe(true);
    });

    it('should return false for an empty string', () => {
      expect(isValidMnemonic('')).toBe(false);
    });

    it('should return false for invalid phrase', () => {
      expect(isValidMnemonic('not a valid seed phrase at all')).toBe(false);
    });

    it('should return false for 11 words (wrong count)', () => {
      const fake = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon';
      expect(isValidMnemonic(fake)).toBe(false);
    });

    it('should trim whitespace before validation', () => {
      const wallet = createWallet();
      expect(isValidMnemonic('  ' + wallet.mnemonic + '  ')).toBe(true);
    });
  });

  describe('importWallet', () => {
    it('should import a valid mnemonic', () => {
      const wallet = createWallet();
      const imported = importWallet(wallet.mnemonic);
      expect(imported.mnemonic).toBe(wallet.mnemonic);
      expect(imported.seed).toBeInstanceOf(Uint8Array);
      expect(imported.seed.length).toBeGreaterThan(0);
    });

    it('should throw on invalid mnemonic', () => {
      expect(() => importWallet('invalid mnemonic phrase here yeah test fake')).toThrow('Seed invalida');
    });

    it('should trim whitespace from imported mnemonic', () => {
      const wallet = createWallet();
      const imported = importWallet('  ' + wallet.mnemonic + '  ');
      expect(imported.mnemonic).toBe(wallet.mnemonic.trim());
    });

    it('should set createdAt on import', () => {
      const wallet = createWallet();
      const imported = importWallet(wallet.mnemonic);
      expect(imported.createdAt).toBeGreaterThan(0);
    });
  });

  describe('getMnemonicWord', () => {
    it('should return correct word by index', () => {
      const wallet = createWallet();
      const words = wallet.mnemonic.split(' ');
      expect(getMnemonicWord(wallet.mnemonic, 0)).toBe(words[0]);
      expect(getMnemonicWord(wallet.mnemonic, 11)).toBe(words[11]);
    });

    it('should throw on out-of-range index', () => {
      const wallet = createWallet();
      expect(() => getMnemonicWord(wallet.mnemonic, -1)).toThrow();
      expect(() => getMnemonicWord(wallet.mnemonic, 12)).toThrow();
    });
  });

  describe('getWordCount', () => {
    it('should return 12 for a standard mnemonic', () => {
      const wallet = createWallet();
      expect(getWordCount(wallet.mnemonic)).toBe(12);
    });
  });
});
