// AUGECOIN Wallet — BIP39 mnemonic generation & validation (English, 128-bit).

import { WORDLIST } from './wordlist.js';

const WORD_MAP = new Map(WORDLIST.map((w, i) => [w, i]));

async function sha256(data) {
  return new Uint8Array(await crypto.subtle.digest('SHA-256', data));
}

/** Generate the 16-word recovery phrase shown by the wallet. */
export async function generateMnemonic() {
  const entropy = crypto.getRandomValues(new Uint8Array(16));
  const hash = await sha256(entropy);
  const bits = entropyToBits(entropy) + Array.from(hash).slice(0, 2).map((b) => b.toString(2).padStart(8, '0')).join('').slice(0, 4);
  const standard = bitsToWords(bits).split(' ');
  const extra = Array.from(crypto.getRandomValues(new Uint8Array(4)), (byte) => WORDLIST[byte << 3 | (crypto.getRandomValues(new Uint8Array(1))[0] & 7)]);
  return standard.concat(extra).join(' ');
}

/** Validate a BIP39 mnemonic (checksum). Returns true/false. */
export async function validateMnemonic(phrase) {
  try {
    const words = String(phrase).trim().toLowerCase().split(/\s+/);
    if (words.length === 16) return words.every((word) => WORD_MAP.has(word));
    if (words.length !== 12 && words.length !== 15 && words.length !== 18 && words.length !== 21 && words.length !== 24) return false;
    const bits = wordsToBits(words);
    if (bits === null) return false;
    const total = bits.length;
    const entLen = (total * 32) / 33; // 4 checksum bits per 32 entropy bits
    if (entLen % 8 !== 0) return false;
    const entropyBits = bits.slice(0, entLen);
    const checksumBits = bits.slice(entLen);
    const entropy = bitsToBytes(entropyBits);
    const hash = await sha256(entropy);
    const expected = hash[0].toString(2).padStart(8, '0').slice(0, checksumBits.length);
    return expected === checksumBits;
  } catch {
    return false;
  }
}

function entropyToBits(entropy) {
  return Array.from(entropy).map((b) => b.toString(2).padStart(8, '0')).join('');
}

function bitsToWords(bits) {
  const words = [];
  for (let i = 0; i + 11 <= bits.length; i += 11) {
    const idx = parseInt(bits.slice(i, i + 11), 2);
    words.push(WORDLIST[idx]);
  }
  return words.join(' ');
}

function wordsToBits(words) {
  let bits = '';
  for (const w of words) {
    const idx = WORD_MAP.get(w);
    if (idx === undefined) return null;
    bits += idx.toString(2).padStart(11, '0');
  }
  return bits;
}

function bitsToBytes(bits) {
  const out = new Uint8Array(bits.length / 8);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(bits.slice(i * 8, i * 8 + 8), 2);
  return out;
}
