import { describe, expect, it } from "vitest";
import { compileSquid } from "../compiler/compileService";
import { DEFAULT_SOURCE } from "../compiler/defaultSource";
import { MemoryVfs } from "../test/memoryVfs";
import { XTEINK_X4_TARGET } from "../target/target";
import { BrowserRuntime } from "./runtime";

describe("browser SQBC runtime", () => {
  it("runs Hello Menu through the shared WASM VM", async () => {
    const compiled = await compileSquid(DEFAULT_SOURCE, XTEINK_X4_TARGET.id);
    expect(compiled.ok, JSON.stringify(compiled.diagnostics)).toBe(true);
    expect(compiled.sqbc).toBeInstanceOf(Uint8Array);

    const runtime = new BrowserRuntime({
      app: { id: compiled.ir!.app.id, name: compiled.ir!.app.name },
      sqbc: compiled.sqbc!,
      basePath: "/sd/apps/hello-menu"
    }, new MemoryVfs());

    let snapshot = await runtime.start();
    expect(snapshot.running).toBe(true);
    expect(snapshot.currentScreen).toBe("menu");
    expect(snapshot.drawCommands).toContainEqual(expect.objectContaining({ op: "text", text: "Hello Menu", x: 240, align: "center" }));
    expect(snapshot.drawCommands).toContainEqual(expect.objectContaining({ op: "rect", x: 32, y: 160, width: 416, height: 48, gray: 15, fill: true }));

    snapshot = await runtime.dispatchKey("DOWN");
    expect(snapshot.state.selected).toBe(1);
    expect(snapshot.currentScreen).toBe("menu");
    expect(snapshot.drawCommands).toContainEqual(expect.objectContaining({ op: "rect", x: 32, y: 216, width: 416, height: 48, gray: 15, fill: true }));

    snapshot = await runtime.dispatchKey("SELECT");
    expect(snapshot.currentScreen).toBe("about");
    expect(snapshot.state.view).toBe("about");

    snapshot = await runtime.dispatchKey("BACK");
    expect(snapshot.exited).toBe(false);
    expect(snapshot.currentScreen).toBe("menu");

    snapshot = await runtime.dispatchKey("BACK");
    expect(snapshot.exited).toBe(true);
  });
});
