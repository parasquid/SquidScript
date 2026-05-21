declare module "./wasm/squidc_wasm.js" {
  export default function init(module?: unknown): Promise<unknown>;
  export function initSync(module?: unknown): unknown;
  export function compile_squidscript(source: string, targetId: string): string;
  export function compile_sqbc(source: string, targetId: string): Uint8Array;
  export function read_sqbc_app_id(bytes: Uint8Array): string;
  export class WasmVm {
    constructor(bytes: Uint8Array, stateBytes: Uint8Array);
    dispatch(event: string): string;
    snapshot(): string;
    state_bytes(): Uint8Array;
    clear_state_dirty(): void;
  }
}
