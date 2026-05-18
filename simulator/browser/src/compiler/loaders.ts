import type { IrProgram, RuntimeProgram } from "../types";

export interface ExecutableLoader<TInput> {
  load(input: TInput): RuntimeProgram;
}

export class IrJsonLoader implements ExecutableLoader<IrProgram> {
  load(input: IrProgram): RuntimeProgram {
    if (input.format !== "squidscript-ir" || input.version !== 1) {
      throw new Error("Unsupported IR format");
    }

    return {
      id: input.app.id,
      name: input.app.name,
      target: input.app.target,
      stateDefaults: Object.fromEntries(input.state.map((entry) => [entry.name, entry.value])),
      functions: new Map((input.functions ?? []).map((fn) => [fn.name, fn])),
      handlers: new Map(input.handlers.map((handler) => [handler.event, handler.statements])),
      screens: new Map(input.screens.map((screen) => [screen.name, screen]))
    };
  }
}

export class SqbcLoader implements ExecutableLoader<ArrayBuffer> {
  load(input: ArrayBuffer): RuntimeProgram {
    const bytes = new Uint8Array(input);
    if (bytes.length < 12) throw new Error("Invalid SQBC: too short");
    const magic = new TextDecoder().decode(bytes.slice(0, 4));
    if (magic !== "SQBC") throw new Error("Invalid SQBC magic");

    const view = new DataView(input);
    const version = view.getUint32(4, true);
    if (version !== 1) throw new Error(`Unsupported SQBC version ${version}`);

    const length = view.getUint32(8, true);
    if (length !== bytes.length - 12) throw new Error("Invalid SQBC payload length");

    const ir = JSON.parse(new TextDecoder().decode(bytes.slice(12))) as IrProgram;
    return new IrJsonLoader().load(ir);
  }
}

export function encodeSqbcForTest(ir: IrProgram): ArrayBuffer {
  const payload = new TextEncoder().encode(JSON.stringify(ir));
  const bytes = new Uint8Array(12 + payload.length);
  bytes.set(new TextEncoder().encode("SQBC"), 0);
  new DataView(bytes.buffer).setUint32(4, 1, true);
  new DataView(bytes.buffer).setUint32(8, payload.length, true);
  bytes.set(payload, 12);
  return bytes.buffer;
}
