import { describe, expect, it } from "vitest";
import { compileFallback, DEFAULT_SOURCE } from "../compiler/fallbackCompiler";
import { MemoryVfs } from "../test/memoryVfs";
import { XTEINK_X4_TARGET } from "../target/target";
import { createBrowserSimManifest, installIrApp, listInstalledApps, loadInstalledApp, uninstallApp, validateManifest } from "./appInstall";

describe("app install loader", () => {
  it("installs manifest and executable, then loads from simulated /sd", async () => {
    const vfs = new MemoryVfs();
    const ir = compileFallback(DEFAULT_SOURCE, "xteink-x4").ir!;

    await installIrApp(vfs, ir, XTEINK_X4_TARGET);
    const paths = await vfs.list("/sd/apps/");
    expect(paths).toContain("/sd/apps/hello-menu/app.json");
    expect(paths).toContain("/sd/apps/hello-menu/main.ir.json");

    const installed = await loadInstalledApp(vfs, XTEINK_X4_TARGET, "hello-menu");
    expect(installed.program.id).toBe("hello-menu");
    expect(installed.basePath).toBe("/sd/apps/hello-menu");
  });

  it("lists and uninstalls installed apps", async () => {
    const vfs = new MemoryVfs();
    const ir = compileFallback(DEFAULT_SOURCE, "xteink-x4").ir!;

    await installIrApp(vfs, ir, XTEINK_X4_TARGET);
    expect(await listInstalledApps(vfs)).toEqual([
      { id: "hello-menu", name: "Hello Menu", basePath: "/sd/apps/hello-menu" }
    ]);

    await uninstallApp(vfs, "hello-menu");
    expect(await listInstalledApps(vfs)).toEqual([]);
  });

  it("enforces required permissions", () => {
    const ir = compileFallback(DEFAULT_SOURCE, "xteink-x4").ir!;
    const manifest = createBrowserSimManifest(ir, XTEINK_X4_TARGET);
    manifest.permissions = manifest.permissions.filter((permission) => permission !== "state.write");

    expect(() => validateManifest(manifest, XTEINK_X4_TARGET)).toThrow("state.write");
  });
});
