// AUGECOIN Wallet — utility helpers (formatting, escaping, address derivation).

import { blake3_512 } from './blake3.js';

export function esc(value) {
  if (value === null || value === undefined) return '';
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/** augesat (u64) → human string with thousands separators and 8 decimals. */
export function auge(value, opts = {}) {
  const { decimals = 8, symbol = false } = opts;
  if (value === null || value === undefined) return '—';
  const v = BigInt(value);
  const per = 10n ** BigInt(decimals);
  const int = v / per;
  const frac = (v % per).toString().padStart(decimals, '0');
  const intStr = Number(int).toLocaleString('en-US');
  const out = `${intStr}.${frac}`;
  return symbol ? `${out} AUGE` : out;
}

export function augesatToAuge(value) {
  return Number(BigInt(value)) / 1e8;
}

export function fmtNum(value) {
  if (value === null || value === undefined) return '—';
  return Number(value).toLocaleString('en-US');
}

export function shortHash(hash, chars = 10) {
  if (!hash) return '—';
  const h = String(hash);
  if (h.length <= chars * 2 + 3) return h;
  return `${h.slice(0, chars)}…${h.slice(-chars)}`;
}

export function timeAgo(tsSeconds) {
  if (!tsSeconds) return '—';
  const diff = Math.floor(Date.now() / 1000) - Number(tsSeconds);
  if (diff < 0) return 'now';
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

export function fmtTime(tsSeconds) {
  if (!tsSeconds) return '—';
  return new Date(Number(tsSeconds) * 1000).toLocaleString('en-GB', { timeZone: 'UTC' }) + ' UTC';
}

export async function copy(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.select();
    try { document.execCommand('copy'); } catch { /* ignore */ }
    document.body.removeChild(ta);
    return true;
  }
}

export function hexToBytes(hex) {
  const clean = String(hex).replace(/^0x/, '');
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(clean.substr(i * 2, 2), 16);
  return out;
}

export function bytesToHex(bytes) {
  return Array.from(bytes).map((b) => b.toString(16).padStart(2, '0')).join('');
}

const BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const ADDRESS_DOMAIN = 'AUGECOIN-SHORT-ADDRESS-V1';

/** Derive the canonical AUGECOIN address (Base58) from an Ed25519 pubkey hex.
 *  A single address per member — it receives both AUGE (coin) and AUGEID. */
export function deriveAddress(pubkeyHex) {
  const hash = blake3_512(hexToBytes(pubkeyHex));
  const payload = hash.slice(0, 24);
  const checksum = blake3_512(new Uint8Array([
    ...new TextEncoder().encode(ADDRESS_DOMAIN), ...payload,
  ])).slice(0, 4);
  return base58Encode(new Uint8Array([...payload, ...checksum]));
}

/** Alias kept for compatibility — the address is a single Base58 address. */
export const deriveShortAddress = deriveAddress;

/** New self-describing address. It embeds the Ed25519 public key so a first
 * receive can activate a Reserved AUGEID without a separate key field. */
export function deriveEmbeddedAddress(pubkeyHex) {
  const publicKey = hexToBytes(pubkeyHex);
  const checksum = blake3_512(new Uint8Array([
    ...new TextEncoder().encode(`${ADDRESS_DOMAIN}-PUBKEY`), ...publicKey,
  ])).slice(0, 4);
  return base58Encode(new Uint8Array([...publicKey, ...checksum]));
}

function base58Encode(bytes) {
  const digits = [0];
  for (const byte of bytes) {
    let carry = byte;
    for (let i = 0; i < digits.length; i++) {
      const value = digits[i] * 256 + carry;
      digits[i] = value % 58;
      carry = Math.floor(value / 58);
    }
    while (carry > 0) {
      digits.push(carry % 58);
      carry = Math.floor(carry / 58);
    }
  }
  const firstNonZero = bytes.findIndex((byte) => byte !== 0);
  const zeroCount = firstNonZero === -1 ? bytes.length : firstNonZero;
  return '1'.repeat(zeroCount) + digits.reverse().map((digit) => BASE58_ALPHABET[digit]).join('');
}

function base58Decode(value) {
  const s = String(value || '');
  if (!s) return null;
  const bytes = [0];
  for (const ch of s) {
    const digit = BASE58_ALPHABET.indexOf(ch);
    if (digit < 0) return null;
    let carry = digit;
    for (let i = 0; i < bytes.length; i++) {
      const current = bytes[i] * 58 + carry;
      bytes[i] = current % 256;
      carry = Math.floor(current / 256);
    }
    while (carry > 0) {
      bytes.push(carry % 256);
      carry = Math.floor(carry / 256);
    }
  }
  const firstNonOne = [...s].findIndex((ch) => ch !== '1');
  const oneCount = firstNonOne === -1 ? s.length : firstNonOne;
  for (let i = 0; i < oneCount; i++) bytes.push(0);
  bytes.reverse();
  return new Uint8Array(bytes);
}

export function embeddedPublicKeyHex(address) {
  const bytes = base58Decode(address);
  if (!bytes || bytes.length !== 36) return null;
  const payload = bytes.slice(0, 32);
  const expected = blake3_512(new Uint8Array([
    ...new TextEncoder().encode(`${ADDRESS_DOMAIN}-PUBKEY`), ...payload,
  ])).slice(0, 4);
  if (bytes.slice(32).some((v, i) => v !== expected[i])) return null;
  return bytesToHex(payload);
}

/** True when `addr` is a valid AUGECOIN Base58 address. */
export function isValidAugeAddress(addr) {
  const bytes = base58Decode(addr);
  if (!bytes || bytes.length !== 28) return false;
  const payload = bytes.slice(0, 24);
  const expected = blake3_512(new Uint8Array([
    ...new TextEncoder().encode(ADDRESS_DOMAIN), ...payload,
  ])).slice(0, 4);
  return bytes.slice(24).every((b, i) => b === expected[i]);
}
