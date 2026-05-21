import type { Vfs } from "../storage/vfs";

export class MemoryVfs implements Vfs {
  private files = new Map<string, string | Uint8Array>();
  private textEncoder = new TextEncoder();
  private textDecoder = new TextDecoder();

  async read(path: string): Promise<string | null> {
    const value = this.files.get(path);
    if (value === undefined) return null;
    return typeof value === "string" ? value : this.textDecoder.decode(value);
  }

  async write(path: string, value: string): Promise<void> {
    this.files.set(path, value);
  }

  async readBytes(path: string): Promise<Uint8Array | null> {
    const value = this.files.get(path);
    if (value === undefined) return null;
    return typeof value === "string" ? this.textEncoder.encode(value) : value;
  }

  async writeBytes(path: string, value: Uint8Array): Promise<void> {
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
