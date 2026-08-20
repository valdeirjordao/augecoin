import * as SecureStore from 'expo-secure-store';
import { hmac } from '@noble/hashes/hmac';
import { sha256 } from '@noble/hashes/sha256';
import { gcm } from '@noble/ciphers/aes';
import { randomBytes } from '@noble/ciphers/webcrypto';

const KEYSTORE_KEY = 'augecoin_wallet';
const PBKDF2_ITERATIONS = 100;
const SALT_LENGTH = 32;
const IV_LENGTH = 12;
const ENC_KEY_LENGTH = 32;

function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return globalThis.btoa(binary);
}

function base64ToBytes(base64: string): Uint8Array {
  const binary = globalThis.atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

function pbkdf2Sha256(
  password: Uint8Array,
  salt: Uint8Array,
  c: number,
  dkLen: number
): Uint8Array {
  const hLen = 32;
  const blockCount = Math.ceil(dkLen / hLen);
  const result = new Uint8Array(blockCount * hLen);

  for (let block = 1; block <= blockCount; block++) {
    const saltBlock = new Uint8Array(salt.length + 4);
    saltBlock.set(salt);
    new DataView(saltBlock.buffer).setUint32(salt.length, block, false);

    let u = hmac(sha256, password, saltBlock);
    let t = new Uint8Array(u);

    for (let i = 1; i < c; i++) {
      u = hmac(sha256, password, u);
      for (let j = 0; j < hLen; j++) {
        t[j] ^= u[j];
      }
    }

    result.set(t, (block - 1) * hLen);
  }

  return result.slice(0, dkLen);
}

function deriveKey(password: string, salt: Uint8Array): Uint8Array {
  const encoder = new TextEncoder();
  return pbkdf2Sha256(encoder.encode(password), salt, PBKDF2_ITERATIONS, ENC_KEY_LENGTH);
}

export async function saveWallet(
  seed: Uint8Array,
  password: string
): Promise<void> {
  const salt = randomBytes(SALT_LENGTH);
  const iv = randomBytes(IV_LENGTH);
  const key = deriveKey(password, salt);
  const cipher = gcm(key, iv);
  const ciphertext = cipher.encrypt(seed);

  const payload = {
    salt: bytesToBase64(salt),
    iv: bytesToBase64(iv),
    ciphertext: bytesToBase64(ciphertext),
  };

  await SecureStore.setItemAsync(KEYSTORE_KEY, JSON.stringify(payload));
}

export async function loadWallet(
  password: string
): Promise<Uint8Array | null> {
  const raw = await SecureStore.getItemAsync(KEYSTORE_KEY);
  if (!raw) return null;

  let payload: { salt: string; iv: string; ciphertext: string };
  try {
    payload = JSON.parse(raw);
  } catch {
    return null;
  }

  const salt = base64ToBytes(payload.salt);
  const iv = base64ToBytes(payload.iv);
  const ciphertext = base64ToBytes(payload.ciphertext);
  const key = deriveKey(password, salt);

  try {
    const cipher = gcm(key, iv);
    return cipher.decrypt(ciphertext);
  } catch {
    return null;
  }
}

export async function deleteWallet(): Promise<void> {
  await SecureStore.deleteItemAsync(KEYSTORE_KEY);
}

export async function walletExists(): Promise<boolean> {
  const raw = await SecureStore.getItemAsync(KEYSTORE_KEY);
  return raw !== null;
}
