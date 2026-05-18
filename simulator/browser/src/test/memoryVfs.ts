import type { Vfs } from "../storage/vfs";

export class MemoryVfs implements Vfs {
  private files = new Map<string, string>();

  async read(path: string): Promise<string | null> {
    return this.files.get(path) ?? null;
  }

  async write(path: string, value: string): Promise<void> {
    this.files.set(path, value);
  }

  async removePrefix(prefix: string): Promise<void> {
    for (const path of this.files.keys()) {
      if (path.startsWith(prefix)) this.files.delete(path);
    }
  }

  async clear(): Promise<void> {
    this.files.clear();
  }

  async list(prefix: string): Promise<string[]> {
    return [...this.files.keys()].filter((path) => path.startsWith(prefix));
  }
}

