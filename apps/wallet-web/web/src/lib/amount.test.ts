import { describe, it, expect } from 'vitest';
import { parseTokenAmount, formatTokenAmount, u64MaxTokens, U64_MAX } from './amount';

describe('parseTokenAmount', () => {
  it('parses plain integers as whole tokens', () => {
    expect(parseTokenAmount('1000', 8)).toBe(1000n * 10n ** 8n);
    expect(parseTokenAmount('1000000', 8)).toBe(1000000n * 10n ** 8n);
  });

  it('parses pt-BR formatted amounts', () => {
    expect(parseTokenAmount('50.000,00', 8)).toBe(50000n * 10n ** 8n);
    expect(parseTokenAmount('500.000.000.000,00', 0)).toBe(500000000000n);
    expect(parseTokenAmount('1.000.000', 8)).toBe(1000000n * 10n ** 8n);
    expect(parseTokenAmount('0,5', 8)).toBe(5n * 10n ** 7n);
  });

  it('parses en-US formatted amounts', () => {
    expect(parseTokenAmount('50,000.00', 8)).toBe(50000n * 10n ** 8n);
    expect(parseTokenAmount('1,000,000', 8)).toBe(1000000n * 10n ** 8n);
    expect(parseTokenAmount('10.5', 8)).toBe(105n * 10n ** 7n);
  });

  it('parses fractional parts within decimals', () => {
    expect(parseTokenAmount('0,00000001', 8)).toBe(1n);
    expect(parseTokenAmount('1,23456789', 8)).toBe(123456789n);
  });

  it('rejects empty and garbage input', () => {
    expect(() => parseTokenAmount('', 8)).toThrow();
    expect(() => parseTokenAmount('abc', 8)).toThrow();
    expect(() => parseTokenAmount('10a', 8)).toThrow();
  });

  it('rejects excess fractional digits', () => {
    expect(() => parseTokenAmount('1,123456789', 8)).toThrow(/casas decimais/);
  });

  it('rejects u64 overflow with a helpful message', () => {
    expect(() => parseTokenAmount('100000000000000', 8)).toThrow(/u64/);
    expect(() => parseTokenAmount((U64_MAX + 1n).toString(), 0)).toThrow(/u64/);
    // But the exact u64 max itself must pass.
    expect(parseTokenAmount(U64_MAX.toString(), 0)).toBe(U64_MAX);
  });

  it('works for zero decimals tokens', () => {
    expect(parseTokenAmount('500.000.000.000,00', 0)).toBe(500000000000n);
    expect(parseTokenAmount('100.000.000.000.000,00', 0)).toBe(100000000000000n);
  });
});

describe('formatTokenAmount', () => {
  it('formats with pt-BR separators and two decimals', () => {
    expect(formatTokenAmount(5000000000000n, 8)).toBe('50.000,00');
    expect(formatTokenAmount(22000000000000n, 8)).toBe('220.000,00');
    expect(formatTokenAmount(0n, 8)).toBe('0,00');
  });

  it('exposes the u64 limit per decimals', () => {
    expect(u64MaxTokens(8)).toBe('184.467.440.737,09551615');
    expect(parseTokenAmount(u64MaxTokens(8), 8)).toBeLessThanOrEqual(U64_MAX);
  });
});
