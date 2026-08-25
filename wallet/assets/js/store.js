// AUGECOIN Wallet — encrypted keystore (PBKDF2 + AES-256-GCM in IndexedDB),
// keyed by platform e-mail so one browser can hold several platform accounts.
//
// Backwards-compatible: reads the legacy React wallet keystore
// (`augecoin-keystore` / `wallets` / <email>) and migrates it into the new
// vault (`augecoin-wallet` / `vault` / member:<email>). Both used the same
// scheme (PBKDF2-SHA256 600k + AES-256-GCM), so the blob is directly portable.

const DB_NAME = 'augecoin-wallet';
const DB_VERSION = 1;
const STORE = 'vault';

const LEGACY_DB_NAME = 'augecoin-keystore';
const LEGACY_STORE = 'wallets';

let dbPromise = null;
function getDb() {
  if (!dbPromise) {
    dbPromise = new Promise((resolve, reject) => {
      const req = indexedDB.open(DB_NAME, DB_VERSION);
      req.onupgradeneeded = () => req.result.createObjectStore(STORE);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
  }
  return dbPromise;
}

function openNamedDb(name, version, store) {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(name, version);
    req.onupgradeneeded = () => { if (!req.result.objectStoreNames.contains(store)) req.result.createObjectStore(store); };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

function getRecord(db, store, key) {
  return new Promise((resolve) => {
    try {
      const tx = db.transaction(store, 'readonly');
      const rq = tx.objectStore(store).get(key);
      rq.onsuccess = () => resolve(rq.result);
      rq.onerror = () => resolve(null);
    } catch {
      resolve(null);
    }
  });
}

function putRecord(db, store, key, value) {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(store, 'readwrite');
    tx.objectStore(store).put(value, key);
    tx.oncomplete = resolve;
    tx.onerror = () => reject(tx.error);
  });
}

async function deriveKey(password, salt) {
  const keyMaterial = await crypto.subtle.importKey('raw', new TextEncoder().encode(password), 'PBKDF2', false, ['deriveKey']);
  return crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt, iterations: 600000, hash: 'SHA-256' },
    keyMaterial,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt']
  );
}

function keyId(email) {
  return 'member:' + String(email || '').trim().toLowerCase();
}

async function decryptBlob(stored, password) {
  if (!stored || !stored.salt || !stored.iv || !stored.ciphertext) return null;
  const key = await deriveKey(password, new Uint8Array(stored.salt));
  try {
    const plaintext = await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv: new Uint8Array(stored.iv) },
      key,
      new Uint8Array(stored.ciphertext)
    );
    return new TextDecoder().decode(plaintext);
  } catch {
    return null; // wrong password
  }
}

export async function saveMnemonic(email, mnemonic, password) {
  const salt = crypto.getRandomValues(new Uint8Array(32));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const key = await deriveKey(password, salt);
  const ciphertext = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, new TextEncoder().encode(mnemonic));
  const stored = { salt: Array.from(salt), iv: Array.from(iv), ciphertext: Array.from(new Uint8Array(ciphertext)) };
  const db = await getDb();
  await putRecord(db, STORE, keyId(email), stored);
}

/** Load + decrypt the mnemonic, migrating from the legacy React keystore. */
export async function loadMnemonic(email, password) {
  const id = keyId(email);
  const db = await getDb();

  let stored = await getRecord(db, STORE, id);
  if (!stored) {
    // Migration: try the legacy React wallet keystore.
    const legacy = await legacyBlob(email);
    if (legacy) {
      stored = legacy;
      await putRecord(db, STORE, id, legacy); // persist into the new vault
    }
  }

  return decryptBlob(stored, password);
}

/** Does a local encrypted key exist for this e-mail (new or legacy)? */
export async function hasMnemonic(email) {
  const db = await getDb();
  if (await getRecord(db, STORE, keyId(email))) return true;
  return !!(await legacyBlob(email));
}

export async function removeMnemonic(email) {
  const db = await getDb();
  await new Promise((resolve) => {
    const tx = db.transaction(STORE, 'readwrite');
    tx.objectStore(STORE).delete(keyId(email));
    tx.oncomplete = resolve;
    tx.onerror = resolve;
  });
}

async function legacyBlob(email) {
  try {
    const db = await openNamedDb(LEGACY_DB_NAME, 1, LEGACY_STORE);
    // Legacy key was the raw lowercased e-mail (no "member:" prefix).
    return await getRecord(db, LEGACY_STORE, String(email || '').trim().toLowerCase());
  } catch {
    return null;
  }
}
