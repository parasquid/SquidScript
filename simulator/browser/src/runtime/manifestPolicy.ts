import type { AppManifest } from "../types";

export function validateBrowserSimManifest(manifest: AppManifest): void {
  if (manifest.entry.type !== "ir" || !manifest.entry.browserSimOnly) {
    throw new Error("browser-sim v1 requires browser-only IR entries");
  }
}

export function validateFirmwareManifest(manifest: AppManifest): void {
  if (manifest.entry.type === "ir") {
    throw new Error("Firmware must reject browser-only IR entries");
  }
  if (manifest.entry.type !== "bytecode") {
    throw new Error(`Unsupported firmware entry type ${manifest.entry.type}`);
  }
  if (!manifest.entry.file.endsWith(".sqbc")) {
    throw new Error("Firmware bytecode entries must point to .sqbc files");
  }
}

