import { describe, expect, it } from "vitest";
import { compileFallback, DEFAULT_SOURCE } from "../compiler/fallbackCompiler";
import { XTEINK_X4_TARGET } from "../target/target";
import { createBrowserSimManifest } from "./appInstall";
import { validateBrowserSimManifest, validateFirmwareManifest } from "./manifestPolicy";

describe("manifest policy", () => {
  it("allows browser-sim IR manifests only in browser-sim", () => {
    const ir = compileFallback(DEFAULT_SOURCE, "xteink-x4").ir!;
    const manifest = createBrowserSimManifest(ir, XTEINK_X4_TARGET);

    expect(() => validateBrowserSimManifest(manifest)).not.toThrow();
    expect(() => validateFirmwareManifest(manifest)).toThrow("Firmware must reject browser-only IR entries");
  });

  it("allows firmware bytecode manifests", () => {
    const ir = compileFallback(DEFAULT_SOURCE, "xteink-x4").ir!;
    const manifest = createBrowserSimManifest(ir, XTEINK_X4_TARGET);
    manifest.entry = { type: "bytecode", file: "main.sqbc" };

    expect(() => validateFirmwareManifest(manifest)).not.toThrow();
    expect(() => validateBrowserSimManifest(manifest)).toThrow("browser-sim v1 requires");
  });
});

