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
      functions: new Map(),
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

  it("executes handler control flow, locals, functions, and bounded repeat statements", async () => {
    const program: RuntimeProgram = {
      id: "flow-app",
      name: "Flow App",
      target: "xteink-x4",
      stateDefaults: { count: 0 },
      functions: new Map([
        ["getStep", {
          name: "getStep",
          params: [],
          statements: [{ op: "return", expr: { op: "literal", value: 2 } }]
        }],
        ["bump", {
          name: "bump",
          params: ["amount"],
          statements: [
            { op: "assign", name: "count", expr: { op: "binary", left: { op: "state", name: "count" }, operator: "+", right: { op: "state", name: "amount" } } },
            { op: "return", expr: { op: "state", name: "count" } }
          ]
        }]
      ]),
      handlers: new Map([
        ["onStart", [{ op: "screen.open", screen: "main" }]],
        ["onKey.DOWN", [
          { op: "let", name: "step", expr: { op: "call", name: "getStep", args: [] } },
          { op: "repeat", count: { op: "literal", value: 2 }, statements: [{ op: "call", name: "bump", args: [{ op: "state", name: "step" }] }] },
          {
            op: "if",
            condition: { op: "binary", left: { op: "state", name: "count" }, operator: "==", right: { op: "literal", value: 4 } },
            then_statements: [{ op: "screen.open", screen: "done" }],
            else_statements: [{ op: "app.exit" }]
          }
        ]]
      ]),
      screens: new Map([
        ["main", { name: "main", render: "compose", statements: [{ op: "display.clear", color: "gray0" }] }],
        ["done", { name: "done", render: "compose", statements: [{ op: "display.clear", color: "gray0" }] }]
      ])
    };
    const runtime = new BrowserRuntime(program, new MemoryVfs());

    await runtime.start();
    const snapshot = await runtime.dispatchKey("DOWN");

    expect(snapshot.state.count).toBe(4);
    expect(snapshot.currentScreen).toBe("done");
    expect(snapshot.exited).toBe(false);
  });

  it("renders screen-local control flow and helper function draw commands", async () => {
    const program: RuntimeProgram = {
      id: "render-flow",
      name: "Render Flow",
      target: "xteink-x4",
      stateDefaults: { selected: 1 },
      functions: new Map([
        ["drawDivider", {
          name: "drawDivider",
          params: [],
          statements: [{ op: "display.line", x1: 0, y1: 40, x2: 480, y2: 40, options: { color: "gray15" } }]
        }]
      ]),
      handlers: new Map([["onStart", [{ op: "screen.open", screen: "main" }]]]),
      screens: new Map([
        ["main", {
          name: "main",
          render: "compose",
          statements: [
            { op: "display.clear", color: "gray0" },
            { op: "call", name: "drawDivider", args: [] },
            {
              op: "if",
              condition: { op: "binary", left: { op: "state", name: "selected" }, operator: "==", right: { op: "literal", value: 1 } },
              then_statements: [{ op: "display.rect", x: 20, y: 80, w: 120, h: 32, options: { fillColor: "gray4" } }],
              else_statements: []
            }
          ]
        }]
      ])
    };
    const runtime = new BrowserRuntime(program, new MemoryVfs());

    const snapshot = await runtime.start();

    expect(snapshot.drawCommands).toContainEqual({ op: "line", x1: 0, y1: 40, x2: 480, y2: 40, gray: 15 });
    expect(snapshot.drawCommands).toContainEqual({ op: "rect", x: 20, y: 80, width: 120, height: 32, gray: 4, fill: true });
  });
});
