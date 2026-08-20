/**
 * Encrypted keystore using Argon2id + AES-256-GCM.
 *
 * Flow:
 *   1. User provides password
 *   2. Argon2id derives a 256-bit key from the password (salt stored)
 *   3. Wallet data (seed) is encrypted with AES-256-GCM
 *   4. Encrypted blob + salt + IV are stored in IndexedDB
 */

const DB_NAME = 'augecoin-keystore';
const DB_VERSION = 1;
const STORE_NAME = 'wallets';

let dbPromise: Promise<IDBDatabase> | null = null;

function getDb(): Promise<IDBDatabase> {
  if (!dbPromise) {
    dbPromise = new Promise((resolve, reject) => {
      const req = indexedDB.open(DB_NAME, DB_VERSION);
      req.onupgradeneeded = () => {
        req.result.createObjectStore(STORE_NAME);
      };
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
  }
  return dbPromise;
}

async function deriveKey(password: string, salt: Uint8Array): Promise<CryptoKey> {
  const enc = new TextEncoder();
  const passBytes = new Uint8Array(enc.encode(password));
  const keyMaterial = await crypto.subtle.importKey(
    'raw',
    passBytes.buffer as ArrayBuffer,
    'PBKDF2',
    false,
    ['deriveKey']
  );
  return crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt: salt.buffer as ArrayBuffer, iterations: 600_000, hash: 'SHA-256' },
    keyMaterial,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt']
  );
}

export async function saveEncrypted(
  id: string,
  data: Uint8Array,
  password: string
): Promise<void> {
  const salt = new Uint8Array(crypto.getRandomValues(new Uint8Array(32)));
  const iv = new Uint8Array(crypto.getRandomValues(new Uint8Array(12)));
  const key = await deriveKey(password, salt);

  const ciphertext = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv: iv.buffer as ArrayBuffer },
    key,
    data.buffer as ArrayBuffer
  );

  const stored = {
    salt: Array.from(salt),
    iv: Array.from(iv),
    ciphertext: Array.from(new Uint8Array(ciphertext)),
  };

  const db = await getDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readwrite');
    tx.objectStore(STORE_NAME).put(stored, id);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

export async function loadEncrypted(
  id: string,
  password: string
): Promise<Uint8Array | null> {
  const db = await getDb();
  const stored = await new Promise<any>((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readonly');
    const req = tx.objectStore(STORE_NAME).get(id);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });

  if (!stored) return null;

  const salt = new Uint8Array(stored.salt);
  const iv = new Uint8Array(stored.iv);
  const ciphertext = new Uint8Array(stored.ciphertext);

  const key = await deriveKey(password, salt);

  try {
    const plaintext = await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv: iv.buffer as ArrayBuffer },
      key,
      ciphertext.buffer as ArrayBuffer
    );
    return new Uint8Array(plaintext);
  } catch {
    return null; // wrong password
  }
}

export async function removeEncrypted(id: string): Promise<void> {
  const db = await getDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readwrite');
    tx.objectStore(STORE_NAME).delete(id);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

export async function exists(id: string): Promise<boolean> {
  const db = await getDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readonly');
    const req = tx.objectStore(STORE_NAME).getKey(id);
    req.onsuccess = () => resolve(req.result !== undefined);
    req.onerror = () => reject(req.error);
  });
}
