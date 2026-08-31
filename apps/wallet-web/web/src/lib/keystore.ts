/**
 * Encrypted local keystore for the AUGECOIN web wallet (Fase 17).
 *
 * The mnemonic/seed never leaves the browser: it is encrypted with AES-GCM
 * using a PBKDF2-derived key (WebCrypto) and persisted to localStorage. The
 * node is only ever given the account number / admin API key — never the
 * private key.
 */

const KEYSTORE_KEY = 'augecoin_wallet_keystore';
const PBKDF2_ITERATIONS = 200_000;
const SALT_LENGTH = 16;
const IV_LENGTH = 12;

function bufToB64(buf: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < buf.length; i++) binary += String.fromCharCode(buf[i]);
  return btoa(binary);
}

function b64ToBuf(b64: string): Uint8Array {
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

export interface StoredWallet {
  mnemonic: string;
  account: number;
  createdAt: number;
}

interface KeystoreEnvelope {
  salt: string;
  iv: string;
  ciphertext: string;
}

function getCrypto(): Crypto {
  const c = globalThis.crypto;
  if (!c?.subtle) throw new Error('WebCrypto unavailable in this environment');
  return c;
}

async function deriveKey(password: string, salt: Uint8Array): Promise<CryptoKey> {
  const c = getCrypto();
  const baseKey = await c.subtle.importKey(
    'raw',
    new TextEncoder().encode(password),
    'PBKDF2',
    false,
    ['deriveKey']
  );
  return c.subtle.deriveKey(
    { name: 'PBKDF2', salt: salt as BufferSource, iterations: PBKDF2_ITERATIONS, hash: 'SHA-256' },
    baseKey,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt']
  );
}

export async function saveWallet(wallet: StoredWallet, password: string): Promise<void> {
  const c = getCrypto();
  const salt = c.getRandomValues(new Uint8Array(SALT_LENGTH));
  const iv = c.getRandomValues(new Uint8Array(IV_LENGTH));
  const key = await deriveKey(password, salt);
  const plaintext = new TextEncoder().encode(JSON.stringify(wallet));
  const ciphertext = await c.subtle.encrypt({ name: 'AES-GCM', iv: iv as BufferSource }, key, plaintext);
  const envelope: KeystoreEnvelope = {
    salt: bufToB64(salt),
    iv: bufToB64(iv),
    ciphertext: bufToB64(new Uint8Array(ciphertext)),
  };
  localStorage.setItem(KEYSTORE_KEY, JSON.stringify(envelope));
}

export function hasWallet(): boolean {
  return localStorage.getItem(KEYSTORE_KEY) !== null;
}

export async function unlockWallet(password: string): Promise<StoredWallet> {
  const raw = localStorage.getItem(KEYSTORE_KEY);
  if (!raw) throw new Error('no wallet found');
  const envelope = JSON.parse(raw) as KeystoreEnvelope;
  const salt = b64ToBuf(envelope.salt);
  const iv = b64ToBuf(envelope.iv);
  const key = await deriveKey(password, salt);
  const plaintext = await getCrypto().subtle.decrypt(
    { name: 'AES-GCM', iv: iv as BufferSource },
    key,
    b64ToBuf(envelope.ciphertext) as BufferSource
  );
  return JSON.parse(new TextDecoder().decode(plaintext)) as StoredWallet;
}

export function clearWallet(): void {
  localStorage.removeItem(KEYSTORE_KEY);
}
