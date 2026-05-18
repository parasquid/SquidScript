declare module "./wasm/squidc_wasm.js" {
  export default function init(module?: unknown): Promise<unknown>;
  export function compile_squidscript(source: string, targetId: string): string;
}

