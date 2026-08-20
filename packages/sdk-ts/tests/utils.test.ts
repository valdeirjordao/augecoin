import { describe, it, expect } from 'vitest';
import {
  augesatToAuge,
  augeToAugesat,
  formatAuge,
  DECIMALS,
  AUGESAT_PER_AUGE,
  TOTAL_SUPPLY_AUGE,
  TOTAL_SUPPLY_AUGESAT,
} from '../src/utils';

describe('augesatToAuge', () => {
  it('converts 0 augesat to "0.00000000"', () => {
    expect(augesatToAuge(0n)).toBe('0.00000000');
  });

  it('converts 1 augesat to "0.00000001"', () => {
    expect(augesatToAuge(1n)).toBe('0.00000001');
  });

  it('converts 1 AUGE (100_000_000 augesat) to "1.00000000"', () => {
    expect(augesatToAuge(100_000_000n)).toBe('1.00000000');
  });

  it('converts max supply augesat', () => {
    expect(augesatToAuge(TOTAL_SUPPLY_AUGESAT)).toBe('762120000.00000000');
  });

  it('handles 99999999 (near 1 AUGE)', () => {
    expect(augesatToAuge(99_999_999n)).toBe('0.99999999');
  });
});

describe('augeToAugesat', () => {
  it('converts "0" to 0n', () => {
    expect(augeToAugesat('0')).toBe(0n);
  });

  it('converts "1" to 100_000_000n', () => {
    expect(augeToAugesat('1')).toBe(100_000_000n);
  });

  it('converts "1.5" to 150_000_000n', () => {
    expect(augeToAugesat('1.5')).toBe(150_000_000n);
  });

  it('converts "0.00000001" to 1n', () => {
    expect(augeToAugesat('0.00000001')).toBe(1n);
  });

  it('converts "762120000" (max supply) correctly', () => {
    expect(augeToAugesat('762120000')).toBe(TOTAL_SUPPLY_AUGESAT);
  });

  it('handles extra decimal places by truncating', () => {
    // "0.000000001" should truncate to 0 (9 decimals)
    expect(augeToAugesat('0.000000001')).toBe(0n);
  });

  it('roundtrip: auge -> augesat -> auge', () => {
    const original = '123.45678901';
    const augesat = augeToAugesat(original);
    const back = augesatToAuge(augesat);
    expect(back).toBe('123.45678901');
  });
});

describe('formatAuge', () => {
  it('formats with symbol', () => {
    expect(formatAuge(100_000_000n, { symbol: true })).toBe('1.0 AUGE');
  });

  it('strips trailing zeros by default', () => {
    expect(formatAuge(100_000_000n)).toBe('1.0');
  });

  it('shows full decimals when requested', () => {
    expect(formatAuge(100_000_000n, { decimals: 8 })).toBe('1.00000000');
  });

  it('formats zero', () => {
    expect(formatAuge(0n)).toBe('0.0');
  });

  it('formats max supply', () => {
    expect(formatAuge(TOTAL_SUPPLY_AUGESAT)).toBe('762120000.0');
  });
});

describe('constants', () => {
  it('DECIMALS is 8', () => {
    expect(DECIMALS).toBe(8);
  });

  it('TOTAL_SUPPLY_AUGE is 762.12 million (linear emission)', () => {
    expect(TOTAL_SUPPLY_AUGE).toBe(762_120_000n);
  });

  it('TOTAL_SUPPLY_AUGESAT matches calculation', () => {
    expect(TOTAL_SUPPLY_AUGESAT).toBe(76_212_000_000_000_000n);
  });
});
