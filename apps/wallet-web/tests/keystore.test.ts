import { describe, it, expect, beforeEach } from 'vitest';
import { saveEncrypted, loadEncrypted, removeEncrypted, exists } from '../src/lib/keystore';

// Mock IndexedDB for tests
import 'fake-indexeddb/auto';

const TEST_ID = 'test-wallet-1';
const TEST_DATA = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]);
const TEST_PASSWORD = 'my-secure-password-123';

describe('keystore', () => {
  beforeEach(async () => {
    // Clean up before each test
    try { await removeEncrypted(TEST_ID); } catch {}
  });

  it('saves and loads encrypted data correctly', async () => {
    await saveEncrypted(TEST_ID, TEST_DATA, TEST_PASSWORD);

    const loaded = await loadEncrypted(TEST_ID, TEST_PASSWORD);
    expect(loaded).not.toBeNull();
    expect(loaded!).toEqual(TEST_DATA);
  });

  it('returns null for wrong password', async () => {
    await saveEncrypted(TEST_ID, TEST_DATA, TEST_PASSWORD);

    const loaded = await loadEncrypted(TEST_ID, 'wrong-password');
    expect(loaded).toBeNull();
  });

  it('returns null for non-existent wallet', async () => {
    const loaded = await loadEncrypted('non-existent', TEST_PASSWORD);
    expect(loaded).toBeNull();
  });

  it('can check if a wallet exists', async () => {
    expect(await exists(TEST_ID)).toBe(false);
    await saveEncrypted(TEST_ID, TEST_DATA, TEST_PASSWORD);
    expect(await exists(TEST_ID)).toBe(true);
  });

  it('can remove a wallet', async () => {
    await saveEncrypted(TEST_ID, TEST_DATA, TEST_PASSWORD);
    expect(await exists(TEST_ID)).toBe(true);
    await removeEncrypted(TEST_ID);
    expect(await exists(TEST_ID)).toBe(false);
  });

  it('produces different ciphertexts with different passwords', async () => {
    // This test verifies encryption is actually working (different keys → different ciphertext)
    // We can't directly check the ciphertext since it contains random IV,
    // but we can verify round-trip with both passwords.
    await saveEncrypted('a', TEST_DATA, 'password1');
    await saveEncrypted('b', TEST_DATA, 'password2');

    const loaded1 = await loadEncrypted('a', 'password1');
    const loaded2 = await loadEncrypted('b', 'password2');

    expect(loaded1).toEqual(TEST_DATA);
    expect(loaded2).toEqual(TEST_DATA);

    // Wrong passwords should fail
    expect(await loadEncrypted('a', 'password2')).toBeNull();
    expect(await loadEncrypted('b', 'password1')).toBeNull();

    await removeEncrypted('a');
    await removeEncrypted('b');
  });
});
