import { describe, it, expect, beforeEach } from 'vitest';

class MemStorage {
  private m = new Map<string, string>();
  getItem(k: string): string | null {
    return this.m.has(k) ? (this.m.get(k) as string) : null;
  }
  setItem(k: string, v: string): void {
    this.m.set(k, v);
  }
  removeItem(k: string): void {
    this.m.delete(k);
  }
  clear(): void {
    this.m.clear();
  }
  key(i: number): string | null {
    return [...this.m.keys()][i] ?? null;
  }
  get length(): number {
    return this.m.size;
  }
}

beforeEach(() => {
  (globalThis as unknown as { localStorage: MemStorage }).localStorage = new MemStorage();
});

import { saveWallet, unlockWallet, hasWallet, clearWallet } from './keystore';

const SAMPLE = {
  mnemonic: 'test test test test test test test test test test test test',
  account: 1,
  createdAt: Date.now(),
};

describe('encrypted keystore', () => {
  it('roundtrips an encrypted wallet', async () => {
    expect(hasWallet()).toBe(false);
    await saveWallet(SAMPLE, 'pw');
    expect(hasWallet()).toBe(true);
    const got = await unlockWallet('pw');
    expect(got.mnemonic).toBe(SAMPLE.mnemonic);
    expect(got.account).toBe(1);
  });

  it('rejects the wrong password', async () => {
    await saveWallet({ mnemonic: 'x', account: 2, createdAt: 0 }, 'pw');
    await expect(unlockWallet('wrong')).rejects.toBeTruthy();
  });

  it('clears the stored wallet', async () => {
    await saveWallet({ mnemonic: 'x', account: 3, createdAt: 0 }, 'pw');
    clearWallet();
    expect(hasWallet()).toBe(false);
  });
});
