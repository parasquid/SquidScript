import type { CompileResult, CompileResponse } from "../types";
import { compileFallback } from "./fallbackCompiler";

type WasmCompiler = {
  default?: (module?: unknown) => Promise<unknown>;
  compile_squidscript: (source: string, targetId: string) => string;
};

export async function compileSquid(source: string, targetId: string): Promise<CompileResult> {
  const wasm = await loadWasmCompiler();
  if (!wasm) {
    return { ...compileFallback(source, targetId), backend: "fallback" };
  }

  await wasm.default?.();
  const response = JSON.parse(wasm.compile_squidscript(source, targetId)) as CompileResponse;
  return { ...response, backend: "wasm" };
}

async function loadWasmCompiler(): Promise<WasmCompiler | null> {
  try {
    return (await import("./wasm/squidc_wasm.js")) as WasmCompiler;
  } catch {
    return null;
  }
}
