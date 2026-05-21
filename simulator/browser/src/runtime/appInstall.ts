import { unzipSync } from "fflate";
import type { Vfs } from "../storage/vfs";
import { normalizePackageEntryPath } from "../storage/paths";
import { loadSquidWasm } from "../compiler/squidWasm";

const SQBC_EXECUTABLE = "main.sqbc";
const PACKAGE_EXTENSION = ".squid.zip";

export interface InstalledApp {
  app: { id: string; name: string };
  sqbc: Uint8Array;
  basePath: string;
}

export interface InstalledAppSummary {
  id: string;
  name: string;
  basePath: string;
}

export interface PackageInstallOptions {
  filename: string;
  expectedAppId?: string;
}

export interface PackageInstallResult {
  appId: string;
  appName: string;
  basePath: string;
  files: string[];
}

export async function installSqbcApp(vfs: Vfs, app: { id: string; name: string }, sqbc: Uint8Array): Promise<string> {
  assertPackageAppId(app.id);
  const base = `/sd/apps/${app.id}`;
  await vfs.removePrefix(`${base}/`);
  await vfs.writeBytes(`${base}/${SQBC_EXECUTABLE}`, sqbc);
  return base;
}

export async function installSquidPackage(vfs: Vfs, packageBytes: Uint8Array, options: PackageInstallOptions): Promise<PackageInstallResult> {
  if (!options.filename.endsWith(PACKAGE_EXTENSION)) {
    throw new Error(`Package filename must end with ${PACKAGE_EXTENSION}`);
  }

  const entries = unzipSync(packageBytes);
  const normalizedEntries = new Map<string, Uint8Array>();
  for (const [path, bytes] of Object.entries(entries)) {
    const normalized = normalizePackageEntryPath(path);
    if (normalizedEntries.has(normalized)) {
      throw new Error(`Duplicate package entry path: ${normalized}`);
    }
    normalizedEntries.set(normalized, bytes);
  }

  const executableBytes = normalizedEntries.get(SQBC_EXECUTABLE);
  if (!executableBytes) {
    throw new Error(`Package is missing executable ${SQBC_EXECUTABLE}`);
  }

  const appId = await readSqbcAppId(executableBytes);
  assertPackageAppId(appId);
  if (options.expectedAppId && appId !== options.expectedAppId) {
    throw new Error("Package app id does not match expected app id");
  }

  const basePath = `/sd/apps/${appId}`;
  await vfs.removePrefix(`${basePath}/`);
  const files = [...normalizedEntries.keys()].sort();
  for (const path of files) {
    await vfs.writeBytes(`${basePath}/${path}`, normalizedEntries.get(path)!);
  }

  return {
    appId,
    appName: appId,
    basePath,
    files
  };
}

export async function loadInstalledApp(vfs: Vfs, appId?: string): Promise<InstalledApp> {
  const resolvedAppId = appId ?? await firstInstalledAppId(vfs);
  if (!resolvedAppId) {
    throw new Error("No installed app found under /sd/apps");
  }

  const basePath = `/sd/apps/${resolvedAppId}`;
  const sqbc = await vfs.readBytes(`${basePath}/${SQBC_EXECUTABLE}`);
  if (!sqbc) {
    throw new Error(`Missing executable ${SQBC_EXECUTABLE}`);
  }

  const actualAppId = await readSqbcAppId(sqbc);
  if (actualAppId !== resolvedAppId) {
    throw new Error("Installed path app id does not match SQBC app id");
  }

  return {
    app: { id: actualAppId, name: actualAppId },
    sqbc,
    basePath
  };
}

export async function firstInstalledAppId(vfs: Vfs): Promise<string | null> {
  const apps = await listInstalledApps(vfs);
  return apps[0]?.id ?? null;
}

export async function listInstalledApps(vfs: Vfs): Promise<InstalledAppSummary[]> {
  const paths = await vfs.list("/sd/apps/");
  const executablePaths = paths.filter((path) => /^\/sd\/apps\/[^/]+\/main\.sqbc$/.test(path)).sort();
  const apps: InstalledAppSummary[] = [];

  for (const path of executablePaths) {
    const raw = await vfs.readBytes(path);
    if (!raw) continue;
    const id = path.match(/^\/sd\/apps\/([^/]+)\/main\.sqbc$/)?.[1];
    if (!id) continue;
    try {
      const appId = await readSqbcAppId(raw);
      if (appId !== id) continue;
      apps.push({
        id: appId,
        name: appId,
        basePath: `/sd/apps/${appId}`
      });
    } catch {
      apps.push({ id, name: id, basePath: `/sd/apps/${id}` });
    }
  }

  return apps;
}

export async function uninstallApp(vfs: Vfs, appId: string): Promise<void> {
  await vfs.removePrefix(`/sd/apps/${appId}/`);
}

function assertPackageAppId(appId: string): void {
  if (!/^[a-z0-9][a-z0-9_-]*$/.test(appId)) {
    throw new Error(`Invalid package app id: ${appId}`);
  }
}

async function readSqbcAppId(bytes: Uint8Array): Promise<string> {
  const wasm = await loadSquidWasm();
  return wasm.read_sqbc_app_id(bytes);
}
