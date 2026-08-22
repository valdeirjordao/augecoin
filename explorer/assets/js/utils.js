// AUGECOIN Explorer — utility helpers (formatting, escaping, time, units).

import { CONFIG } from './config.js';
import { blake3_512 } from './blake3.js';

// ── HTML escaping (XSS) ─────────────────────────────────────────────────

export function esc(value) {
  if (value === null || value === undefined) return '';
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

// ── AUGE / augesat ─────────────────────────────────────────────────────

/** augesat (u64) → human string with thousands separators and 8 decimals. */
export function auge(value, opts = {}) {
  const { decimals = 8, symbol = false, trim = false } = opts;
  if (value === null || value === undefined) return '—';
  const v = BigInt(value);
  const neg = v < 0n;
  const abs = neg ? -v : v;
  const per = 10n ** BigInt(decimals);
  const int = abs / per;
  let frac = (abs % per).toString().padStart(decimals, '0');
  if (trim) frac = frac.replace(/0+$/, '') || '0';
  const intStr = Number(int).toLocaleString('en-US');
  const out = `${neg ? '-' : ''}${intStr}.${frac}`;
  return symbol ? `${out} AUGE` : out;
}

/** Short human amount for dashboards (e.g. 762.12M). */
export function augeShort(value) {
  if (value === null || value === undefined) return '—';
  const augeVal = Number(value) / CONFIG.AUGESAT_PER_AUGE;
  const abs = Math.abs(augeVal);
  if (abs >= 1e9) return (augeVal / 1e9).toFixed(2) + 'B';
  if (abs >= 1e6) return (augeVal / 1e6).toFixed(2) + 'M';
  if (abs >= 1e3) return (augeVal / 1e3).toFixed(2) + 'K';
  return augeVal.toFixed(2);
}

// ── Time ───────────────────────────────────────────────────────────────

export function fmtTime(tsSeconds) {
  if (!tsSeconds) return '—';
  const d = new Date(Number(tsSeconds) * 1000);
  return d.toLocaleString('en-GB', { timeZone: 'UTC' }) + ' UTC';
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

export function fmtDuration(seconds) {
  const s = Number(seconds) || 0;
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${s % 60}s`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
  return `${Math.floor(s / 86400)}d ${Math.floor((s % 86400) / 3600)}h`;
}

// ── Numbers / hashes ────────────────────────────────────────────────────

export function fmtNum(value) {
  if (value === null || value === undefined) return '—';
  return Number(value).toLocaleString('en-US');
}

/** Shorten a long hex hash for display, keeping prefix and suffix. */
export function shortHash(hash, chars = 10) {
  if (!hash) return '—';
  const h = String(hash);
  if (h.length <= chars * 2 + 3) return h;
  return `${h.slice(0, chars)}…${h.slice(-chars)}`;
}

export function fmtBytes(bytes) {
  const b = Number(bytes) || 0;
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(2)} KB`;
  return `${(b / 1024 / 1024).toFixed(2)} MB`;
}

// ── Copy / share / download ────────────────────────────────────────────

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

export function downloadCSV(filename, rows) {
  const csv = rows.map((r) => r.map((c) => `"${String(c ?? '').replace(/"/g, '""')}"`).join(',')).join('\n');
  const blob = new Blob(['\uFEFF' + csv], { type: 'text/csv;charset=utf-8;' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export async function share(text) {
  if (navigator.share) {
    try { await navigator.share({ title: 'AUGECOIN', text }); return; } catch { /* cancelled */ }
  }
  await copy(text);
}

// ── Query params ───────────────────────────────────────────────────────

export function qs(name) {
  return new URLSearchParams(location.search).get(name);
}

// ── Account badges ─────────────────────────────────────────────────────

export function accountTypeLabel(type) {
  const t = Number(type) || 0;
  if (t === 0) return 'Standard';
  return `Type ${t}`;
}

/** Map the account `state` Debug string to a friendly label + color. */
export function accountStateInfo(state) {
  const s = String(state || '').toLowerCase();
  switch (s) {
    case 'normal': return { label: 'Normal', kind: 'success' };
    case 'forsale': return { label: 'For Sale', kind: 'warning' };
    case 'foratomicaccountswap': return { label: 'Atomic Swap', kind: 'info' };
    case 'foratomiccoinswap': return { label: 'Coin Swap', kind: 'info' };
    case 'unknown': return { label: 'Unknown', kind: 'muted' };
    default: return { label: state || '—', kind: 'muted' };
  }
}

/** Derive the canonical AUGECOIN address (Base58) from an Ed25519 pubkey hex. */
export function deriveAddress(pubkeyHex) {
  const hash = blake3_512(hexToBytes(pubkeyHex));
  const payload = hash.slice(0, 24);
  const checksum = blake3_512(new Uint8Array([
    ...new TextEncoder().encode('AUGECOIN-SHORT-ADDRESS-V1'), ...payload,
  ])).slice(0, 4);
  return base58Encode(new Uint8Array([...payload, ...checksum]));
}

/** Alias kept for compatibility — the address is a single Base58 address. */
export const deriveShortAddress = deriveAddress;

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

function hexToBytes(hex) {
  const clean = String(hex).replace(/^0x/, '');
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(clean.substr(i * 2, 2), 16);
  return out;
}
