import type { DrawCommand, LogicalKey } from "../types";
import type { Vfs } from "../storage/vfs";
import type { InstalledApp } from "./appInstall";
import { loadSquidWasm } from "../compiler/wasmModule";

export interface RuntimeSnapshot {
  appId: string;
  running: boolean;
  exited: boolean;
  currentScreen: string;
  state: Record<string, unknown>;
  drawCommands: DrawCommand[];
}

interface WasmRuntimeSnapshot {
  running: boolean;
  exited: boolean;
  currentScreen: string;
  state: Record<string, unknown>;
  drawCommands: DrawCommand[];
  stateDirty: boolean;
}

type WasmModule = typeof import("../compiler/wasm/squidc_wasm.js");
type WasmVm = InstanceType<WasmModule["WasmVm"]>;

export class BrowserRuntime {
  private vm: WasmVm | null = null;
  private lastSnapshot: RuntimeSnapshot;

  constructor(
    private readonly installed: InstalledApp,
    private readonly vfs: Vfs
  ) {
    this.lastSnapshot = {
      appId: installed.app.id,
      running: false,
      exited: false,
      currentScreen: "",
      state: {},
      drawCommands: []
    };
  }

  async start(): Promise<RuntimeSnapshot> {
    const wasm = await loadWasmRuntime();
    const stateBytes = await this.loadStateBytes();
    this.vm = new wasm.WasmVm(this.installed.sqbc, stateBytes);
    return this.dispatchEvent("app.start");
  }

  async dispatchKey(key: LogicalKey): Promise<RuntimeSnapshot> {
    if (!this.vm || this.lastSnapshot.exited) return this.lastSnapshot;
    return this.dispatchEvent(`key.${key}`);
  }

  async resetState(): Promise<RuntimeSnapshot> {
    await this.vfs.removePrefix(this.statePrefix());
    const wasm = await loadWasmRuntime();
    this.vm = new wasm.WasmVm(this.installed.sqbc, new Uint8Array());
    return this.dispatchEvent("app.start");
  }

  snapshot(): RuntimeSnapshot {
    return {
      ...this.lastSnapshot,
      state: { ...this.lastSnapshot.state },
      drawCommands: [...this.lastSnapshot.drawCommands]
    };
  }

  private async dispatchEvent(event: string): Promise<RuntimeSnapshot> {
    if (!this.vm) throw new Error("Runtime is not started");
    const raw = event === "snapshot" ? this.vm.snapshot() : this.vm.dispatch(event);
    const snapshot = JSON.parse(raw) as WasmRuntimeSnapshot;
    if (snapshot.stateDirty) {
      await this.vfs.writeBytes(this.statePath(), this.vm.state_bytes());
      this.vm.clear_state_dirty();
    }
    this.lastSnapshot = {
      appId: this.installed.app.id,
      running: snapshot.running,
      exited: snapshot.exited,
      currentScreen: snapshot.currentScreen,
      state: snapshot.state,
      drawCommands: snapshot.drawCommands.length > 0 ? snapshot.drawCommands : this.lastSnapshot.drawCommands
    };
    return this.snapshot();
  }

  private async loadStateBytes(): Promise<Uint8Array> {
    return await this.vfs.readBytes(this.statePath()) ?? new Uint8Array();
  }

  private statePrefix(): string {
    return `/sd/system/app-state/${this.installed.app.id}`;
  }

  private statePath(): string {
    return `${this.statePrefix()}/state.sqst`;
  }
}

async function loadWasmRuntime(): Promise<WasmModule> {
  return await loadSquidWasm();
}
