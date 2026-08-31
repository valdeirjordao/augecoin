/**
 * Parsing/formatting of human token amounts for the wallet UI.
 *
 * On-chain AUGE20 supplies and amounts are u64 integers in the token's
 * smallest unit (10^-decimals). Users type human amounts ("50.000,00",
 * "50,000.00", "1000000"); this module converts them safely to the raw
 * u64 bigint and back, failing with a clear error instead of throwing a
 * silent `SyntaxError` (which made the deploy button appear dead).
 */

/** Largest value encodable in the on-chain u64 fields. */
export const U64_MAX = 2n ** 64n - 1n;

/**
 * Parse a human token amount into raw smallest units.
 *
 * Accepted formats (pt-BR and en-US tolerant):
 *   "1000"            → 1000 tokens
 *   "50.000,00"       → 50000 tokens   (pt-BR: '.' thousands, ',' decimal)
 *   "50,000.00"       → 50000 tokens   (en-US: ',' thousands, '.' decimal)
 *   "1.000.000"       → 1000000 tokens (repeated separator = thousands)
 *   "0,00000001"      → minimum unit (1 raw)
 *
 * A lone separator followed by exactly 3 digits is treated as a thousands
 * separator ("500.000" → 500000); any other lone separator is decimal
 * ("10.5" → 10.5 tokens).
 *
 * @throws Error with a user-facing (pt-BR) message on invalid input,
 *         excess fractional digits or u64 overflow.
 */
export function parseTokenAmount(raw: string, decimals: number): bigint {
  const s = String(raw ?? '')
    .trim()
    .replace(/[\s_]/g, '');
  if (!s) throw new Error('Informe um valor de supply.');
  if (!/^[0-9.,]+$/.test(s)) {
    throw new Error(`Valor inválido: "${raw}". Use apenas dígitos, pontos e vírgulas.`);
  }

  const hasDot = s.includes('.');
  const hasComma = s.includes(',');
  let intPart: string;
  let fracPart = '';

  if (hasDot && hasComma) {
    // Both present: the rightmost one is the decimal separator.
    const decSep = s.lastIndexOf('.') > s.lastIndexOf(',') ? '.' : ',';
    const thouSep = decSep === '.' ? ',' : '.';
    const clean = s.split(thouSep).join('');
    const idx = clean.indexOf(decSep);
    intPart = clean.slice(0, idx);
    fracPart = clean.slice(idx + 1);
  } else if (hasDot || hasComma) {
    const sep = hasDot ? '.' : ',';
    const parts = s.split(sep);
    if (parts.length > 2) {
      intPart = parts.join(''); // "1.000.000" → thousands separators
    } else if (parts[1].length === 3 && parts[0] !== '') {
      intPart = parts.join(''); // lone separator + 3 digits → thousands
    } else {
      intPart = parts[0]; // lone separator → decimal ("10.5", "0,25")
      fracPart = parts[1];
    }
  } else {
    intPart = s;
  }

  if (!/^\d*$/.test(intPart) || !/^\d*$/.test(fracPart)) {
    throw new Error(`Valor inválido: "${raw}".`);
  }
  if (intPart === '') intPart = '0';
  if (/^0+$/.test(fracPart)) fracPart = ''; // ",00" adds no precision
  if (fracPart.length > decimals) {
    throw new Error(
      `"${raw}" tem ${fracPart.length} casas decimais; o token usa ${decimals}.`
    );
  }

  const per = 10n ** BigInt(decimals);
  const rawUnits =
    BigInt(intPart) * per + BigInt((fracPart + '0'.repeat(decimals)).slice(0, decimals));
  if (rawUnits > U64_MAX) {
    throw new Error(
      `"${raw}" excede o limite u64. Com ${decimals} decimais, o máximo é ` +
        `${formatTokenAmount(U64_MAX, decimals)}.`
    );
  }
  return rawUnits;
}

/** Raw smallest units → human string with pt-BR separators ("50.000,00"). */
export function formatTokenAmount(rawUnits: bigint, decimals: number): string {
  const per = 10n ** BigInt(decimals);
  const int = rawUnits / per;
  let frac = (rawUnits % per).toString().padStart(decimals, '0');
  frac = frac.replace(/0+$/, '');
  const intStr = Number(int).toLocaleString('pt-BR');
  return frac ? `${intStr},${frac}` : `${intStr},00`;
}

/** u64 max representable as whole tokens for the given decimals. */
export function u64MaxTokens(decimals: number): string {
  return formatTokenAmount(U64_MAX, decimals);
}
