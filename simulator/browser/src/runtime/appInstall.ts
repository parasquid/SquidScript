import { IrJsonLoader } from "../compiler/loaders";
import type { AppManifest, IrProgram, RuntimeProgram, TargetDefinition } from "../types";
import type { Vfs } from "../storage/vfs";
import { validateBrowserSimManifest } from "./manifestPolicy";

const REQUIRED_PERMISSIONS = ["display.draw", "state.read", "state.write"];

export interface InstalledApp {
  manifest: AppManifest;
  program: RuntimeProgram;
  basePath: string;
}

export interface InstalledAppSummary {
  id: string;
  name: string;
  basePath: string;
}

export function createBrowserSimManifest(ir: IrProgram, target: TargetDefinition): AppManifest {
  return {
    format: "squidapp-v1",
    id: ir.app.id,
    name: ir.app.name,
    version: "0.0.0-browser-sim",
    runtime: { language: "squidscript", version: "0.2" },
    entry: { type: "ir", file: "main.ir.json", browserSimOnly: true },
    permissions: REQUIRED_PERMISSIONS,
    requires: {
      runtime: "squidscript>=0.2",
      display: {
        minWidth: target.display.logical.width,
        minHeight: target.display.logical.height,
        pixelFormats: [target.display.color.defaultPixelFormat]
      },
      keys: target.input.buttons.map((button) => button.logical),
      features: REQUIRED_PERMISSIONS
    }
  };
}

export async function installIrApp(vfs: Vfs, ir: IrProgram, target: TargetDefinition): Promise<string> {
  const manifest = createBrowserSimManifest(ir, target);
  validateManifest(manifest, target);

  const base = `/sd/apps/${manifest.id}`;
  await vfs.write(`${base}/app.json`, JSON.stringify(manifest, null, 2));
  await vfs.write(`${base}/${manifest.entry.file}`, JSON.stringify(ir, null, 2));
  return base;
}

export async function loadInstalledApp(vfs: Vfs, target: TargetDefinition, appId?: string): Promise<InstalledApp> {
  const resolvedAppId = appId ?? await firstInstalledAppId(vfs);
  if (!resolvedAppId) {
    throw new Error("No installed app found under /sd/apps");
  }

  const basePath = `/sd/apps/${resolvedAppId}`;
  const manifestRaw = await vfs.read(`${basePath}/app.json`);
  if (!manifestRaw) {
    throw new Error(`Missing manifest for ${resolvedAppId}`);
  }

  const manifest = JSON.parse(manifestRaw) as AppManifest;
  validateManifest(manifest, target);
  validateBrowserSimManifest(manifest);

  if (manifest.entry.type !== "ir" || !manifest.entry.browserSimOnly) {
    throw new Error("Browser-sim v1 can only run browser-only IR entries");
  }

  const executableRaw = await vfs.read(`${basePath}/${manifest.entry.file}`);
  if (!executableRaw) {
    throw new Error(`Missing executable ${manifest.entry.file}`);
  }

  const ir = JSON.parse(executableRaw) as IrProgram;
  if (ir.app.id !== manifest.id) {
    throw new Error("Manifest app id does not match IR app id");
  }

  return {
    manifest,
    program: new IrJsonLoader().load(ir),
    basePath
  };
}

export async function firstInstalledAppId(vfs: Vfs): Promise<string | null> {
  const apps = await listInstalledApps(vfs);
  return apps[0]?.id ?? null;
}

export async function listInstalledApps(vfs: Vfs): Promise<InstalledAppSummary[]> {
  const paths = await vfs.list("/sd/apps/");
  const manifestPaths = paths.filter((path) => /^\/sd\/apps\/[^/]+\/app\.json$/.test(path)).sort();
  const apps: InstalledAppSummary[] = [];

  for (const path of manifestPaths) {
    const raw = await vfs.read(path);
    if (!raw) continue;
    try {
      const manifest = JSON.parse(raw) as AppManifest;
      apps.push({
        id: manifest.id,
        name: manifest.name,
        basePath: `/sd/apps/${manifest.id}`
      });
    } catch {
      const id = path.match(/^\/sd\/apps\/([^/]+)\/app\.json$/)?.[1];
      if (id) apps.push({ id, name: id, basePath: `/sd/apps/${id}` });
    }
  }

  return apps;
}

export async function uninstallApp(vfs: Vfs, appId: string): Promise<void> {
  await vfs.removePrefix(`/sd/apps/${appId}/`);
}

export function validateManifest(manifest: AppManifest, target: TargetDefinition): void {
  if (manifest.format !== "squidapp-v1") {
    throw new Error("Invalid app manifest format");
  }
  if (manifest.runtime.language !== "squidscript") {
    throw new Error("Unsupported app runtime language");
  }
  if (!target.compatibility.includes("squidscript-0.2")) {
    throw new Error("Target compatibility check failed");
  }
  if (manifest.requires.display.minWidth > target.display.logical.width || manifest.requires.display.minHeight > target.display.logical.height) {
    throw new Error("App display requirements exceed target display");
  }
  if (!manifest.requires.display.pixelFormats.some((format) => target.display.color.supportedPixelFormats.includes(format))) {
    throw new Error("App pixel format requirements are not supported by target");
  }

  const targetKeys = new Set(target.input.buttons.map((button) => button.logical));
  for (const key of manifest.requires.keys) {
    if (!targetKeys.has(key)) throw new Error(`App requires unsupported key ${key}`);
  }

  for (const feature of manifest.requires.features) {
    if (!target.features.includes(feature)) throw new Error(`App requires unsupported feature ${feature}`);
  }

  for (const permission of REQUIRED_PERMISSIONS) {
    if (!manifest.permissions.includes(permission)) throw new Error(`App is missing permission ${permission}`);
  }
}
