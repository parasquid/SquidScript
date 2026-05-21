import { describe, expect, it } from "vitest";
import { strToU8, zipSync } from "fflate";
import { compileSquid } from "../compiler/compileService";
import { DEFAULT_SOURCE } from "../compiler/defaultSource";
import { MemoryVfs } from "../test/memoryVfs";
import { XTEINK_X4_TARGET } from "../target/target";
import { installSqbcApp, installSquidPackage, listInstalledApps, loadInstalledApp, uninstallApp } from "./appInstall";

describe("SQBC app install loader", () => {
  it("installs executable SQBC, then loads from simulated /sd", async () => {
    const vfs = new MemoryVfs();
    const compiled = await compiledHello();

    await installSqbcApp(vfs, compiled.ir!.app, compiled.sqbc!);
    const paths = await vfs.list("/sd/apps/");
    expect(paths).toContain("/sd/apps/hello-menu/main.sqbc");

    const installed = await loadInstalledApp(vfs, "hello-menu");
    expect(installed.app.id).toBe("hello-menu");
    expect(installed.sqbc.length).toBeGreaterThan(0);
    expect(installed.basePath).toBe("/sd/apps/hello-menu");
  });

  it("lists and uninstalls installed apps", async () => {
    const vfs = new MemoryVfs();
    const compiled = await compiledHello();

    await installSqbcApp(vfs, compiled.ir!.app, compiled.sqbc!);
    expect(await listInstalledApps(vfs)).toEqual([
      { id: "hello-menu", name: "hello-menu", basePath: "/sd/apps/hello-menu" }
    ]);

    await uninstallApp(vfs, "hello-menu");
    expect(await listInstalledApps(vfs)).toEqual([]);
  });

  it("rejects an executable stored under a mismatched app id", async () => {
    const vfs = new MemoryVfs();
    const compiled = await compiledHello();
    await vfs.writeBytes("/sd/apps/other/main.sqbc", compiled.sqbc!);

    expect(await listInstalledApps(vfs)).toEqual([]);
    await expect(loadInstalledApp(vfs, "other")).rejects.toThrow("Installed path app id does not match SQBC app id");
  });

  it("installs a canonical .squid.zip package with executable and resources", async () => {
    const vfs = new MemoryVfs();
    const compiled = await compiledHello();
    const icon = new Uint8Array([0, 1, 2, 255]);

    const result = await installSquidPackage(vfs, packageBytes({
      "main.sqbc": compiled.sqbc!,
      "web/index.html": "<script type=\"module\" src=\"./js/app.js\"></script>",
      "web/js/app.js": "fetch('./data/menu.json')",
      "web/css/app.css": "body { color: black; }",
      "web/data/menu.json": "{\"items\":[\"hello\"]}",
      "resources/icon.bmp": icon
    }), { filename: "hello-menu.squid.zip" });

    expect(result).toEqual({
      appId: "hello-menu",
      appName: "hello-menu",
      basePath: "/sd/apps/hello-menu",
      files: [
        "main.sqbc",
        "resources/icon.bmp",
        "web/css/app.css",
        "web/data/menu.json",
        "web/index.html",
        "web/js/app.js"
      ]
    });
    expect(await vfs.readBytes("/sd/apps/hello-menu/main.sqbc")).toEqual(compiled.sqbc);
    expect(await vfs.read("/sd/apps/hello-menu/web/data/menu.json")).toBe("{\"items\":[\"hello\"]}");
    expect(await vfs.readBytes("/sd/apps/hello-menu/resources/icon.bmp")).toEqual(icon);
  });

  it("rejects package imports without the canonical extension", async () => {
    const vfs = new MemoryVfs();
    const compiled = await compiledHello();

    await expect(installSquidPackage(vfs, packageBytes({
      "main.sqbc": compiled.sqbc!
    }), { filename: "hello-menu.zip" })).rejects.toThrow("Package filename must end with .squid.zip");
  });

  it("rejects package imports with no executable", async () => {
    const vfs = new MemoryVfs();

    await expect(installSquidPackage(vfs, packageBytes({
      "web/index.html": "<h1>Hello</h1>"
    }), { filename: "hello-menu.squid.zip" })).rejects.toThrow("Package is missing executable main.sqbc");
  });

  it.each([
    ["absolute path", "/main.sqbc"],
    ["parent traversal", "web/../main.sqbc"],
    ["installer path", "sd/apps/hello-menu/main.sqbc"],
    ["system path", "system/app-state/hello-menu/state.sqst"],
    ["backslash path", "web\\index.html"]
  ])("rejects package entry with %s", async (_name, path) => {
    const vfs = new MemoryVfs();

    await expect(installSquidPackage(vfs, packageBytes({
      [path]: new Uint8Array([1])
    }), { filename: "hello-menu.squid.zip" })).rejects.toThrow("Invalid package entry path");
  });

  it("rejects duplicate normalized package entries", async () => {
    const vfs = new MemoryVfs();
    const compiled = await compiledHello();

    await expect(installSquidPackage(vfs, packageBytes({
      "main.sqbc": compiled.sqbc!,
      "web//data.json": "{}",
      "web/data.json": "{}"
    }), { filename: "hello-menu.squid.zip" })).rejects.toThrow("Duplicate package entry path");
  });

  it("rejects a package filename app id mismatch when an expected id is provided", async () => {
    const vfs = new MemoryVfs();
    const compiled = await compiledHello();

    await expect(installSquidPackage(vfs, packageBytes({
      "main.sqbc": compiled.sqbc!
    }), { filename: "other.squid.zip", expectedAppId: "other" })).rejects.toThrow("Package app id does not match expected app id");
  });
});

async function compiledHello() {
  const compiled = await compileSquid(DEFAULT_SOURCE, XTEINK_X4_TARGET.id);
  expect(compiled.ok, JSON.stringify(compiled.diagnostics)).toBe(true);
  expect(compiled.sqbc).toBeInstanceOf(Uint8Array);
  return compiled;
}

function packageBytes(entries: Record<string, string | Uint8Array>): Uint8Array {
  return zipSync(Object.fromEntries(Object.entries(entries).map(([path, value]) => [
    path,
    typeof value === "string" ? strToU8(value) : value
  ])));
}
