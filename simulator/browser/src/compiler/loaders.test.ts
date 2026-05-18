import { describe, expect, it } from "vitest";
import { compileFallback, DEFAULT_SOURCE } from "./fallbackCompiler";
import { encodeSqbcForTest, IrJsonLoader, SqbcLoader } from "./loaders";

describe("executable loaders", () => {
  it("loads IR JSON into a runtime program", () => {
    const ir = compileFallback(DEFAULT_SOURCE, "xteink-x4").ir!;
    const program = new IrJsonLoader().load(ir);

    expect(program.id).toBe("hello-menu");
    expect(program.handlers.get("onKey.DOWN")).toEqual([
      { op: "assign", name: "selected", expr: { op: "binary", left: { op: "state", name: "selected" }, operator: "+", right: { op: "literal", value: 1 } } },
      { op: "state.save" },
      { op: "screen.refresh" }
    ]);
    expect(program.screens.get("main")?.statements.some((statement) => statement.op === "display.text")).toBe(true);
  });

  it("loads SQBC containers through the same runtime program boundary", () => {
    const ir = compileFallback(DEFAULT_SOURCE, "xteink-x4").ir!;
    const program = new SqbcLoader().load(encodeSqbcForTest(ir));

    expect(program.id).toBe("hello-menu");
    expect(program.handlers.get("onKey.DOWN")?.at(-1)).toEqual({ op: "screen.refresh" });
    expect(program.screens.has("detail")).toBe(true);
  });

  it("rejects invalid SQBC", () => {
    expect(() => new SqbcLoader().load(new ArrayBuffer(0))).toThrow("Invalid SQBC");
  });
});
