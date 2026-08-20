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

/** Derive the AUGECOIN address (bech32m "auge1…") from an Ed25519 pubkey hex. */
export function deriveAddress(pubkeyHex) {
  const pk = hexToBytes(pubkeyHex);
  const hash = blake3_512(pk);
  return bech32mEncode('auge', hash);
}

/** Compact Base58Check address for external payment displays. */
export function deriveShortAddress(pubkeyHex) {
  const hash = blake3_512(hexToBytes(pubkeyHex));
  const payload = hash.slice(0, 24);
  const checksum = blake3_512(new Uint8Array([
    ...new TextEncoder().encode('AUGECOIN-SHORT-ADDRESS-V1'), ...payload,
  ])).slice(0, 4);
  return base58Encode(new Uint8Array([...payload, ...checksum]));
}

function base58Encode(bytes) {
  const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
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
  return '1'.repeat(zeroCount) + digits.reverse().map((digit) => alphabet[digit]).join('');
}

// bech32m encoder (BIP-350) — self-contained.
const BECH32_CHARSET = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l';
function bech32mEncode(hrp, data) {
  const words = bech32ToWords(data);
  const checksum = bech32CreateChecksum(hrp, words, 0x2bc830a3);
  const combined = words.concat(checksum);
  let out = hrp + '1';
  for (const w of combined) out += BECH32_CHARSET[w];
  return out;
}
function bech32HrpExpand(hrp) {
  const out = [];
  for (let i = 0; i < hrp.length; i++) out.push(hrp.charCodeAt(i) >> 5);
  out.push(0);
  for (let i = 0; i < hrp.length; i++) out.push(hrp.charCodeAt(i) & 31);
  return out;
}
function bech32ToWords(data) {
  const out = [];
  let bits = 0, value = 0;
  for (const byte of data) {
    value = (value << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      out.push((value >> (bits - 5)) & 31);
      bits -= 5;
    }
  }
  if (bits > 0) out.push((value << (5 - bits)) & 31);
  return out;
}
const BECH32_GEN = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
function bech32Polymod(values) {
  let chk = 1;
  for (const v of values) {
    const b = chk >>> 25;
    chk = ((chk & 0x1ffffff) << 5) ^ v;
    for (let i = 0; i < 5; i++) if ((b >>> i) & 1) chk ^= BECH32_GEN[i];
  }
  return chk;
}
function bech32CreateChecksum(hrp, data, constant) {
  const values = bech32HrpExpand(hrp).concat(data, [0, 0, 0, 0, 0, 0]);
  const polymod = bech32Polymod(values) ^ constant;
  const out = [];
  for (let i = 0; i < 6; i++) out.push((polymod >>> (5 * (5 - i))) & 31);
  return out;
}

const BECH32M_CONSTANT = 0x2bc830a3;
const BECH32_CONSTANT = 1;

function bech32mDecode(addr) {
  const s = String(addr || '').trim().toLowerCase();
  const hasLower = s !== s.toUpperCase();
  const hasUpper = s !== s.toLowerCase();
  if (hasLower && hasUpper) throw new Error('mixed case');
  if (s.length < 8 || s.length > 90) throw new Error('invalid length');
  let pos = s.lastIndexOf('1');
  if (pos < 1 || pos + 7 > s.length) throw new Error('invalid separator');
  const hrp = s.slice(0, pos);
  const dataPart = s.slice(pos + 1);
  const data = [];
  for (const ch of dataPart) {
    const idx = BECH32_CHARSET.indexOf(ch);
    if (idx === -1) throw new Error('invalid character');
    data.push(idx);
  }
  const poly = bech32Polymod(bech32HrpExpand(hrp).concat(data));
  if (poly !== BECH32M_CONSTANT && poly !== BECH32_CONSTANT) throw new Error('checksum mismatch');
  // drop checksum
  const words = data.slice(0, -6);
  // 5-bit → 8-bit
  const bytes = [];
  let bits = 0, value = 0;
  for (const w of words) {
    value = (value << 5) | w;
    bits += 5;
    if (bits >= 8) {
      bytes.push((value >> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  if (bits >= 5 || ((value << (8 - bits)) & 0xff) !== 0) throw new Error('invalid padding');
  return { hrp, data: bytes };
}

/** True when `addr` is a canonical AUGECOIN address (bech32m `auge1…`, 64-byte BLAKE3-512 payload). */
export function isValidAugeAddress(addr) {
  try {
    const { hrp, data } = bech32mDecode(addr);
    return hrp === 'auge' && data.length === 64;
  } catch {
    return false;
  }
}
