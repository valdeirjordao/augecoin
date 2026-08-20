/** AUGECOIN monetary constants. */
export const DECIMALS = 8;
export const AUGESAT_PER_AUGE = 100_000_000n;
export const TOTAL_SUPPLY_AUGE = 762_120_000n;
export const TOTAL_SUPPLY_AUGESAT = TOTAL_SUPPLY_AUGE * AUGESAT_PER_AUGE;

/**
 * Convert augesat (smallest unit, u64) to AUGE string with 8 decimal places.
 */
export function augesatToAuge(augesat: bigint): string {
  const negative = augesat < 0n;
  const abs = negative ? -augesat : augesat;
  const integer = abs / AUGESAT_PER_AUGE;
  const fractional = abs % AUGESAT_PER_AUGE;
  const fracStr = fractional.toString().padStart(DECIMALS, '0');
  const sign = negative ? '-' : '';
  return `${sign}${integer}.${fracStr}`;
}

/**
 * Convert AUGE string (e.g. "1.5") to augesat bigint.
 */
export function augeToAugesat(auge: string): bigint {
  const trimmed = auge.trim();
  const negative = trimmed.startsWith('-');
  const abs = negative ? trimmed.slice(1) : trimmed;

  const parts = abs.split('.');
  const integer = BigInt(parts[0] || '0') * AUGESAT_PER_AUGE;
  let fractional = 0n;

  if (parts.length > 1) {
    const fracStr = parts[1].padEnd(DECIMALS, '0').slice(0, DECIMALS);
    fractional = BigInt(fracStr);
  }

  const result = integer + fractional;
  return negative ? -result : result;
}

/**
 * Format augesat as a human-readable string with optional symbol.
 */
export function formatAuge(
  augesat: bigint,
  options?: { symbol?: boolean; decimals?: number }
): string {
  const str = augesatToAuge(augesat);
  const symbol = options?.symbol ? ' AUGE' : '';
  const maxDecimals = options?.decimals ?? 8;

  const dotIdx = str.indexOf('.');
  if (dotIdx === -1) return `${str}.0${symbol}`;

  const intPart = str.slice(0, dotIdx);
  let decPart = str.slice(dotIdx + 1);

  // Trim trailing zeros (unless decimals is specified)
  if (options?.decimals !== undefined) {
    decPart = decPart.slice(0, maxDecimals).padEnd(maxDecimals, '0');
  } else {
    decPart = decPart.replace(/0+$/, '');
    if (decPart.length === 0) {
      decPart = '0';
    }
  }

  return `${intPart}.${decPart}${symbol}`;
}
