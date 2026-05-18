import { describe, expect, it } from "vitest";
import { compileFallback, DEFAULT_SOURCE } from "./fallbackCompiler";

describe("fallback compiler", () => {
  it("compiles Hello Menu to stable IR JSON", () => {
    const result = compileFallback(DEFAULT_SOURCE, "xteink-x4");

    expect(result.ok).toBe(true);
    expect(result.ir).toMatchObject({
      format: "squidscript-ir",
      version: 1,
      app: { id: "hello-menu", target: "xteink-x4" },
      state: [{ name: "selected", value: 0 }],
      screens: [
        { name: "menu", render: "compose" },
        { name: "hello", render: "compose" },
        { name: "about", render: "compose" }
      ]
    });
    expect(result.ir?.functions.map((fn) => fn.name)).toEqual(["drawMenuRow"]);
    expect(result.ir?.handlers.find((handler) => handler.event === "onKey.DOWN")?.statements[0]).toMatchObject({ op: "if" });
  });

  it("returns diagnostics with source spans", () => {
    const result = compileFallback("screen(\"main\") {}\n", "xteink-x4");

    expect(result.ok).toBe(false);
    expect(result.diagnostics.some((diagnostic) => diagnostic.code === "E_APP_REQUIRED" && diagnostic.span.end >= diagnostic.span.start)).toBe(true);
  });

  it("requires at least one screen", () => {
    const result = compileFallback('app "empty" target "xteink-x4"\n', "xteink-x4");

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("E_SCREEN_REQUIRED");
  });
});
