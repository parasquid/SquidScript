import type { CompileResult, CompileResponse } from "../types";
import { loadSquidWasm } from "./wasmModule";

export async function compileSquid(source: string, targetId: string): Promise<CompileResult> {
  const wasmResult = await loadWasmCompiler();
  if (!wasmResult.ok) {
    return {
      ok: false,
      diagnostics: [{
        code: "E_WASM_UNAVAILABLE",
        severity: "error",
        message: `Rust/WASM compiler is unavailable: ${wasmResult.error}`,
        span: { start: 0, end: 0 }
      }],
      ir: null,
      backend: "wasm"
    };
  }

  const wasm = wasmResult.wasm;
  const response = JSON.parse(wasm.compile_squidscript(source, targetId)) as CompileResponse;
  if (!response.ok) return { ...response, backend: "wasm" };
  return { ...response, sqbc: wasm.compile_sqbc(source, targetId), backend: "wasm" };
}

async function loadWasmCompiler(): Promise<
  { ok: true; wasm: Awaited<ReturnType<typeof loadSquidWasm>> } | { ok: false; error: string }
> {
  try {
    return { ok: true, wasm: await loadSquidWasm() };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}
