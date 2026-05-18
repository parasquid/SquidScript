import { describe, expect, it } from "vitest";
import { compileFallback, DEFAULT_SOURCE } from "./fallbackCompiler";
import { encodeSqbcForTest, IrJsonLoader, SqbcLoader } from "./loaders";

describe("executable loaders", () => {
  it("loads IR JSON into a runtime program", () => {
    const ir = compileFallback(DEFAULT_SOURCE, "xteink-x4").ir!;
    const program = new IrJsonLoader().load(ir);

    expect(program.id).toBe("hello-menu");
    expect(program.functions.has("drawMenuRow")).toBe(true);
    expect(program.handlers.get("onKey.DOWN")?.[0]).toMatchObject({ op: "if" });
    expect(program.screens.get("menu")?.statements.some((statement) => statement.op === "call" && statement.name === "drawMenuRow")).toBe(true);
  });

  it("loads SQBC containers through the same runtime program boundary", () => {
    const ir = compileFallback(DEFAULT_SOURCE, "xteink-x4").ir!;
    const program = new SqbcLoader().load(encodeSqbcForTest(ir));

    expect(program.id).toBe("hello-menu");
    expect(program.handlers.get("onKey.SELECT")?.[0]).toMatchObject({ op: "if" });
    expect(program.screens.has("hello")).toBe(true);
    expect(program.screens.has("about")).toBe(true);
  });

  it("rejects invalid SQBC", () => {
    expect(() => new SqbcLoader().load(new ArrayBuffer(0))).toThrow("Invalid SQBC");
  });
});
