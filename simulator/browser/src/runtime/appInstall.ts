import { IrJsonLoader } from "../compiler/loaders";
import type { IrProgram, RuntimeProgram } from "../types";
import type { Vfs } from "../storage/vfs";

const IR_EXECUTABLE = "main.ir.json";

export interface InstalledApp {
  program: RuntimeProgram;
  basePath: string;
}

export interface InstalledAppSummary {
  id: string;
  name: string;
  basePath: string;
}

export async function installIrApp(vfs: Vfs, ir: IrProgram): Promise<string> {
  const base = `/sd/apps/${ir.app.id}`;
  await vfs.write(`${base}/${IR_EXECUTABLE}`, JSON.stringify(ir, null, 2));
  return base;
}

export async function loadInstalledApp(vfs: Vfs, appId?: string): Promise<InstalledApp> {
  const resolvedAppId = appId ?? await firstInstalledAppId(vfs);
  if (!resolvedAppId) {
    throw new Error("No installed app found under /sd/apps");
  }

  const basePath = `/sd/apps/${resolvedAppId}`;
  const executableRaw = await vfs.read(`${basePath}/${IR_EXECUTABLE}`);
  if (!executableRaw) {
    throw new Error(`Missing executable ${IR_EXECUTABLE}`);
  }

  const ir = JSON.parse(executableRaw) as IrProgram;
  if (ir.app.id !== resolvedAppId) {
    throw new Error("Installed path app id does not match IR app id");
  }

  return {
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
  const executablePaths = paths.filter((path) => /^\/sd\/apps\/[^/]+\/main\.ir\.json$/.test(path)).sort();
  const apps: InstalledAppSummary[] = [];

  for (const path of executablePaths) {
    const raw = await vfs.read(path);
    if (!raw) continue;
    const id = path.match(/^\/sd\/apps\/([^/]+)\/main\.ir\.json$/)?.[1];
    if (!id) continue;
    try {
      const ir = JSON.parse(raw) as IrProgram;
      if (ir.app.id !== id) continue;
      apps.push({
        id: ir.app.id,
        name: ir.app.name,
        basePath: `/sd/apps/${ir.app.id}`
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
