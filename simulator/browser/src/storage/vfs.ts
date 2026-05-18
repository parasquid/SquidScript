export interface Vfs {
  read(path: string): Promise<string | null>;
  write(path: string, value: string): Promise<void>;
  removePrefix(prefix: string): Promise<void>;
  clear(): Promise<void>;
  list(prefix: string): Promise<string[]>;
}

export class LocalStorageVfs implements Vfs {
  constructor(private readonly root = "squidscript-browser-sim") {}

  async read(path: string): Promise<string | null> {
    return localStorage.getItem(this.key(path));
  }

  async write(path: string, value: string): Promise<void> {
    localStorage.setItem(this.key(path), value);
  }

  async removePrefix(prefix: string): Promise<void> {
    const keys = this.keys(prefix);
    for (const key of keys) localStorage.removeItem(key);
  }

  async clear(): Promise<void> {
    await this.removePrefix("/");
  }

  async list(prefix: string): Promise<string[]> {
    return this.keys(prefix).map((key) => key.slice(`${this.root}:`.length));
  }

  private keys(prefix: string): string[] {
    const storagePrefix = this.key(prefix);
    return Array.from({ length: localStorage.length }, (_, index) => localStorage.key(index))
      .filter((key): key is string => key !== null && key.startsWith(storagePrefix));
  }

  private key(path: string): string {
    return `${this.root}:${path}`;
  }
}

export class IndexedDbVfs implements Vfs {
  private readonly dbName = "squidscript-browser-sim";
  private readonly storeName = "files";

  async read(path: string): Promise<string | null> {
    const db = await this.open();
    return this.tx<string | null>(db, "readonly", (store, resolve) => {
      const request = store.get(path);
      request.onsuccess = () => resolve((request.result as string | undefined) ?? null);
    });
  }

  async write(path: string, value: string): Promise<void> {
    const db = await this.open();
    await this.tx<void>(db, "readwrite", (store, resolve) => {
      const request = store.put(value, path);
      request.onsuccess = () => resolve();
    });
  }

  async removePrefix(prefix: string): Promise<void> {
    const paths = await this.list(prefix);
    const db = await this.open();
    await Promise.all(paths.map((path) => this.tx<void>(db, "readwrite", (store, resolve) => {
      const request = store.delete(path);
      request.onsuccess = () => resolve();
    })));
  }

  async clear(): Promise<void> {
    const db = await this.open();
    await this.tx<void>(db, "readwrite", (store, resolve) => {
      const request = store.clear();
      request.onsuccess = () => resolve();
    });
  }

  async list(prefix: string): Promise<string[]> {
    const db = await this.open();
    return this.tx<string[]>(db, "readonly", (store, resolve) => {
      const request = store.getAllKeys();
      request.onsuccess = () => resolve((request.result as IDBValidKey[]).map(String).filter((path) => path.startsWith(prefix)));
    });
  }

  private open(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open(this.dbName, 1);
      request.onerror = () => reject(request.error);
      request.onupgradeneeded = () => request.result.createObjectStore(this.storeName);
      request.onsuccess = () => resolve(request.result);
    });
  }

  private tx<T>(db: IDBDatabase, mode: IDBTransactionMode, body: (store: IDBObjectStore, resolve: (value: T) => void) => void): Promise<T> {
    return new Promise((resolve, reject) => {
      const transaction = db.transaction(this.storeName, mode);
      const store = transaction.objectStore(this.storeName);
      transaction.onerror = () => reject(transaction.error);
      body(store, resolve);
    });
  }
}

export function createBrowserVfs(): Vfs {
  return "indexedDB" in globalThis ? new IndexedDbVfs() : new LocalStorageVfs();
}

