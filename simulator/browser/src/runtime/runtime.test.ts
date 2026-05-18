import { describe, expect, it } from "vitest";
import { compileFallback, DEFAULT_SOURCE } from "../compiler/fallbackCompiler";
import { IrJsonLoader } from "../compiler/loaders";
import { MemoryVfs } from "../test/memoryVfs";
import { BrowserRuntime } from "./runtime";
import type { RuntimeProgram } from "../types";

describe("browser runtime", () => {
  it("runs onStart, dispatches keys, refreshes, saves state, and exits", async () => {
    const ir = compileFallback(DEFAULT_SOURCE, "xteink-x4").ir!;
    const runtime = new BrowserRuntime(new IrJsonLoader().load(ir), new MemoryVfs());

    let snapshot = await runtime.start();
    expect(snapshot.running).toBe(true);
    expect(snapshot.currentScreen).toBe("main");
    expect(snapshot.drawCommands.length).toBeGreaterThan(0);
    expect(snapshot.drawCommands).toContainEqual(expect.objectContaining({ op: "text", text: "Hello Menu", x: 240, align: "center" }));

    snapshot = await runtime.dispatchKey("DOWN");
    expect(snapshot.state.selected).toBe(1);

    snapshot = await runtime.dispatchKey("SELECT");
    expect(snapshot.currentScreen).toBe("detail");

    snapshot = await runtime.dispatchKey("BACK");
    expect(snapshot.exited).toBe(true);
  });

  it("dispatches behavior from IR handlers instead of hardcoded keys", async () => {
    const program: RuntimeProgram = {
      id: "custom-app",
      name: "Custom App",
      target: "xteink-x4",
      stateDefaults: { count: 0 },
      handlers: new Map([
        ["onStart", [{ op: "screen.open", screen: "main" }]],
        ["onKey.UP", [{ op: "assign", name: "count", expr: { op: "binary", left: { op: "state", name: "count" }, operator: "+", right: { op: "literal", value: 1 } } }, { op: "screen.refresh" }]],
        ["onKey.SELECT", [{ op: "app.exit" }]]
      ]),
      screens: new Map([
        ["main", { name: "main", render: "compose", statements: [{ op: "display.clear", color: "gray0" }, { op: "display.text", text: "Custom", options: { x: 24, y: 36, w: 400, fontHeight: 24 } }] }]
      ])
    };
    const runtime = new BrowserRuntime(program, new MemoryVfs());

    let snapshot = await runtime.start();
    expect(snapshot.currentScreen).toBe("main");
    expect(snapshot.state.count).toBe(0);

    snapshot = await runtime.dispatchKey("DOWN");
    expect(snapshot.state.count).toBe(0);

    snapshot = await runtime.dispatchKey("UP");
    expect(snapshot.state.count).toBe(1);

    snapshot = await runtime.dispatchKey("SELECT");
    expect(snapshot.exited).toBe(true);
  });
});
