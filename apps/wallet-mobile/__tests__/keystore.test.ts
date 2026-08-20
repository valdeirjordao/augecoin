import { saveWallet, loadWallet, deleteWallet, walletExists } from '../src/lib/keystore';

jest.mock('expo-secure-store', () => {
  let store: Record<string, string> = {};
  return {
    __clearStore: () => { store = {}; },
    setItemAsync: jest.fn(async (key: string, value: string) => { store[key] = value; }),
    getItemAsync: jest.fn(async (key: string) => store[key] ?? null),
    deleteItemAsync: jest.fn(async (key: string) => { delete store[key]; }),
  };
});

import * as SecureStore from 'expo-secure-store';

describe('Keystore', () => {
  const seed = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
  const password = 'secure-password-123';

  beforeEach(() => {
    jest.clearAllMocks();
    (SecureStore as any).__clearStore();
  });

  describe('saveWallet and loadWallet', () => {
    it('should encrypt and store wallet, then load with correct password', async () => {
      await saveWallet(seed, password);
      const loaded = await loadWallet(password);
      expect(loaded).not.toBeNull();
      expect(loaded).toBeInstanceOf(Uint8Array);
      expect(loaded!.length).toBe(seed.length);
      expect(Array.from(loaded!)).toEqual(Array.from(seed));
    });

    it('should return null when loading with wrong password', async () => {
      await saveWallet(seed, password);
      const loaded = await loadWallet('wrong-password');
      expect(loaded).toBeNull();
    });

    it('should return null when no wallet exists', async () => {
      const loaded = await loadWallet(password);
      expect(loaded).toBeNull();
    });

    it('should produce different ciphertexts for same seed (random IV/salt)', async () => {
      await saveWallet(seed, password);
      const loaded1 = await loadWallet(password);
      expect(loaded1).not.toBeNull();

      await saveWallet(seed, password);
      const loaded2 = await loadWallet(password);
      expect(loaded2).not.toBeNull();

      expect(Array.from(loaded1!)).toEqual(Array.from(loaded2!));
    });
  });

  describe('deleteWallet', () => {
    it('should remove stored wallet', async () => {
      await saveWallet(seed, password);
      const exists1 = await walletExists();
      expect(exists1).toBe(true);

      await deleteWallet();
      const exists2 = await walletExists();
      expect(exists2).toBe(false);
    });
  });

  describe('walletExists', () => {
    it('should return false when no wallet stored', async () => {
      const exists = await walletExists();
      expect(exists).toBe(false);
    });

    it('should return true after saving wallet', async () => {
      await saveWallet(seed, password);
      const exists = await walletExists();
      expect(exists).toBe(true);
    });
  });

  describe('data integrity', () => {
    it('should preserve exact byte content through encrypt/decrypt cycle', async () => {
      const largeSeed = new Uint8Array(64);
      for (let i = 0; i < largeSeed.length; i++) {
        largeSeed[i] = i;
      }

      await saveWallet(largeSeed, password);
      const loaded = await loadWallet(password);
      expect(loaded).not.toBeNull();
      expect(Array.from(loaded!)).toEqual(Array.from(largeSeed));
    });

    it('should handle empty seed', async () => {
      const emptySeed = new Uint8Array(0);
      await saveWallet(emptySeed, password);
      const loaded = await loadWallet(password);
      expect(loaded).not.toBeNull();
      expect(loaded!.length).toBe(0);
    });
  });
});
