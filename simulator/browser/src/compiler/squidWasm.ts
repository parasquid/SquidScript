import wasmUrl from "./wasm/squidc_wasm_bg.wasm?url";
import init, * as wasm from "./wasm/squidc_wasm.js";

let initPromise: Promise<void> | null = null;

export async function loadSquidWasm(): Promise<typeof wasm> {
  initPromise ??= initWasm();
  await initPromise;
  return wasm;
}

async function initWasm(): Promise<void> {
  if (isNodeRuntime()) {
    const { readFileSync } = await import("node:fs");
    const { join } = await import("node:path");
    const bytes = readFileSync(join(process.cwd(), "src/compiler/wasm/squidc_wasm_bg.wasm"));
    wasm.initSync({ module: bytes });
    return;
  }
  await init({ module_or_path: wasmUrl });
}

function isNodeRuntime(): boolean {
  return typeof process !== "undefined" && !!process.versions?.node;
}
