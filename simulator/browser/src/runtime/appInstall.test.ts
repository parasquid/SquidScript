import { describe, expect, it } from "vitest";
import { compileFallback, DEFAULT_SOURCE } from "../compiler/fallbackCompiler";
import { MemoryVfs } from "../test/memoryVfs";
import { XTEINK_X4_TARGET } from "../target/target";
import { installIrApp, listInstalledApps, loadInstalledApp, uninstallApp } from "./appInstall";

describe("app install loader", () => {
  it("installs executable IR, then loads from simulated /sd", async () => {
    const vfs = new MemoryVfs();
    const ir = compileFallback(DEFAULT_SOURCE, "xteink-x4").ir!;

    await installIrApp(vfs, ir);
    const paths = await vfs.list("/sd/apps/");
    expect(paths).not.toContain("/sd/apps/hello-menu/app.json");
    expect(paths).toContain("/sd/apps/hello-menu/main.ir.json");

    const installed = await loadInstalledApp(vfs, "hello-menu");
    expect(installed.program.id).toBe("hello-menu");
    expect(installed.basePath).toBe("/sd/apps/hello-menu");
  });

  it("lists and uninstalls installed apps", async () => {
    const vfs = new MemoryVfs();
    const ir = compileFallback(DEFAULT_SOURCE, "xteink-x4").ir!;

    await installIrApp(vfs, ir);
    expect(await listInstalledApps(vfs)).toEqual([
      { id: "hello-menu", name: "Hello Menu", basePath: "/sd/apps/hello-menu" }
    ]);

    await uninstallApp(vfs, "hello-menu");
    expect(await listInstalledApps(vfs)).toEqual([]);
  });

  it("rejects an executable stored under a mismatched app id", async () => {
    const vfs = new MemoryVfs();
    const ir = compileFallback(DEFAULT_SOURCE, "xteink-x4").ir!;
    await vfs.write("/sd/apps/other/main.ir.json", JSON.stringify(ir));

    expect(await listInstalledApps(vfs)).toEqual([]);
    await expect(loadInstalledApp(vfs, "other")).rejects.toThrow("Installed path app id does not match IR app id");
  });
});
