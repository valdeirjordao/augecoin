// AUGECOIN Wallet — local history persistence (IndexedDB).
//
// Medium-term fix for "the transaction history disappears": the chain client
// only scans the last ~120 blocks, so older operations fall out of the window
// as the chain advances. This module persists every operation the wallet has
// ever seen, keyed by the owner's public key, and supports incremental sync
// (only scan blocks newer than the last persisted height) so the history grows
// over time instead of being recomputed from scratch on every load.

const DB_NAME = 'augecoin-history';
const DB_VERSION = 1;
const OPS_STORE = 'operations';
const META_STORE = 'meta';

let dbPromise = null;

function getDb() {
  if (!dbPromise) {
    dbPromise = new Promise((resolve, reject) => {
      const req = indexedDB.open(DB_NAME, DB_VERSION);
      req.onupgradeneeded = () => {
        const db = req.result;
        if (!db.objectStoreNames.contains(OPS_STORE)) {
          db.createObjectStore(OPS_STORE, { keyPath: 'id' });
        }
        if (!db.objectStoreNames.contains(META_STORE)) {
          db.createObjectStore(META_STORE);
        }
      };
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
  }
  return dbPromise;
}

const opId = (pubKey, opHash) => `${pubKey}:${opHash}`;

/** All persisted operations for a public key (unsorted). */
export async function loadCachedHistory(pubKey) {
  if (!pubKey) return [];
  try {
    const db = await getDb();
    return await new Promise((resolve) => {
      const tx = db.transaction(OPS_STORE, 'readonly');
      const range = IDBKeyRange.bound(pubKey + ':', pubKey + ':\uffff');
      const req = tx.objectStore(OPS_STORE).getAll(range);
      req.onsuccess = () => resolve(req.result || []);
      req.onerror = () => resolve([]);
    });
  } catch {
    return [];
  }
}

/** Merge operations into the store, de-duplicating by opHash. */
export async function saveHistoryItems(pubKey, items) {
  if (!pubKey || !items || !items.length) return;
  const db = await getDb();
  await new Promise((resolve, reject) => {
    const tx = db.transaction(OPS_STORE, 'readwrite');
    const store = tx.objectStore(OPS_STORE);
    for (const it of items) store.put({ id: opId(pubKey, it.opHash), ...it });
    tx.oncomplete = resolve;
    tx.onerror = () => reject(tx.error);
  });
}

/** Read the meta record ({ lastScannedHeight }) for a public key. */
export async function getMeta(pubKey) {
  if (!pubKey) return null;
  try {
    const db = await getDb();
    return await new Promise((resolve) => {
      const tx = db.transaction(META_STORE, 'readonly');
      const req = tx.objectStore(META_STORE).get(pubKey);
      req.onsuccess = () => resolve(req.result || null);
      req.onerror = () => resolve(null);
    });
  } catch {
    return null;
  }
}

export async function setMeta(pubKey, value) {
  if (!pubKey) return;
  const db = await getDb();
  await new Promise((resolve) => {
    const tx = db.transaction(META_STORE, 'readwrite');
    tx.objectStore(META_STORE).put(value, pubKey);
    tx.oncomplete = resolve;
    tx.onerror = resolve;
  });
}

/** Remove all persisted history for a public key (used on logout). */
export async function clearHistory(pubKey) {
  if (!pubKey) return;
  try {
    const db = await getDb();
    await new Promise((resolve) => {
      const tx = db.transaction(OPS_STORE, 'readwrite');
      const range = IDBKeyRange.bound(pubKey + ':', pubKey + ':\uffff');
      const req = tx.objectStore(OPS_STORE).delete(range);
      req.onsuccess = () => resolve();
      req.onerror = () => resolve();
    });
    await setMeta(pubKey, null);
  } catch {
    /* best-effort */
  }
}
