// Browser storage (IndexedDB), usable from the page and from the worker.
//
// Database "elf2":
//   roms:  {sha256, name, base, bytes, enabled}   keyed by SHA-256 of the image
//   state: arbitrary values by key; "snapshot" holds the auto-saved machine state

const DB_NAME = 'elf2';
const DB_VERSION = 1;

export function openDb() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains('roms')) db.createObjectStore('roms', { keyPath: 'sha256' });
      if (!db.objectStoreNames.contains('state')) db.createObjectStore('state');
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

function run(db, store, mode, fn) {
  return new Promise((resolve, reject) => {
    const t = db.transaction(store, mode);
    let result;
    const req = fn(t.objectStore(store));
    if (req) req.onsuccess = () => { result = req.result; };
    t.oncomplete = () => resolve(result);
    t.onerror = () => reject(t.error);
    t.onabort = () => reject(t.error);
  });
}

export const getAllRoms = (db) => run(db, 'roms', 'readonly', (s) => s.getAll());
export const getRom = (db, sha) => run(db, 'roms', 'readonly', (s) => s.get(sha));
export const putRom = (db, rom) => run(db, 'roms', 'readwrite', (s) => s.put(rom));
export const deleteRom = (db, sha) => run(db, 'roms', 'readwrite', (s) => s.delete(sha));
export const getState = (db, key) => run(db, 'state', 'readonly', (s) => s.get(key));
export const putState = (db, key, value) => run(db, 'state', 'readwrite', (s) => s.put(value, key));
export const deleteState = (db, key) => run(db, 'state', 'readwrite', (s) => s.delete(key));

export async function sha256Hex(bytes) {
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('');
}

export const hex4 = (v) => v.toString(16).toUpperCase().padStart(4, '0');
