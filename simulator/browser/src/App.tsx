import { useEffect, useMemo, useState } from "react";
import { Play, RefreshCw, RotateCcw, Save, Trash2, Upload, Wrench } from "lucide-react";
import { compileSquid } from "./compiler/compileService";
import { DEFAULT_SOURCE } from "./compiler/fallbackCompiler";
import { createDebugEvent, formatDebugData, type DebugEvent } from "./debug/log";
import { installIrApp, listInstalledApps, loadInstalledApp, uninstallApp, type InstalledAppSummary } from "./runtime/appInstall";
import { ButtonArbiter, type ButtonOutcome } from "./runtime/input";
import { BrowserRuntime, type RuntimeSnapshot } from "./runtime/runtime";
import { createBrowserVfs, type Vfs } from "./storage/vfs";
import { keyboardToLogicalKey, validateTarget, XTEINK_X4_TARGET } from "./target/target";
import type { CompileResult, CompilerBackend, LogicalKey } from "./types";
import { DeviceSimulator } from "./ui/DeviceSimulator";
import "./styles.css";

const SOURCE_KEY = "squidscript-browser-sim:editor-source";
const IDLE_COMMANDS = [
  { op: "clear" as const, gray: 0 },
  { op: "text" as const, x: 240, y: 360, text: "No app running", gray: 8, fontHeight: 24, align: "center" as const, maxWidth: 420 }
];

export default function App() {
  const vfs = useMemo<Vfs>(() => createBrowserVfs(), []);
  const buttonArbiter = useMemo(() => new ButtonArbiter({
    longPress: XTEINK_X4_TARGET.input.longPress ?? [],
    chords: XTEINK_X4_TARGET.input.chords ?? []
  }), []);
  const [source, setSource] = useState(() => localStorage.getItem(SOURCE_KEY) ?? DEFAULT_SOURCE);
  const [compiled, setCompiled] = useState<CompileResult | null>(null);
  const [installedAppId, setInstalledAppId] = useState<string | null>(null);
  const [installedApps, setInstalledApps] = useState<InstalledAppSummary[]>([]);
  const [storageFiles, setStorageFiles] = useState<string[]>([]);
  const [compilerBackend, setCompilerBackend] = useState<CompilerBackend | "unknown">("unknown");
  const [runtime, setRuntime] = useState<BrowserRuntime | null>(null);
  const [snapshot, setSnapshot] = useState<RuntimeSnapshot | null>(null);
  const [status, setStatus] = useState("Ready");
  const [debugEvents, setDebugEvents] = useState<DebugEvent[]>([]);

  function log(scope: string, message: string, data?: Record<string, unknown>): void {
    const event = createDebugEvent(scope, message, data);
    setDebugEvents((events) => [event, ...events].slice(0, 80));
    console.debug(`[browser-sim:${scope}] ${message}`, data ?? {});
  }

  useEffect(() => {
    validateTarget(XTEINK_X4_TARGET);
    void refreshApps();
  }, []);

  useEffect(() => {
    localStorage.setItem(SOURCE_KEY, source);
  }, [source]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const key = keyboardToLogicalKey(event);
      if (!key) return;
      event.preventDefault();
      void sendKey(key);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  async function compile(): Promise<void> {
    log("compile", "starting compile", { target: XTEINK_X4_TARGET.id, sourceBytes: source.length });
    const result = await compileSquid(source, XTEINK_X4_TARGET.id);
    setCompiled(result);
    setCompilerBackend(result.backend);
    setStatus(result.ok ? `Compiled main.ir.json with ${result.backend.toUpperCase()}` : `Compile failed with ${result.backend.toUpperCase()}`);
    log("compile", result.ok ? "compile succeeded" : "compile failed", {
      backend: result.backend,
      diagnostics: result.diagnostics.length,
      appId: result.ir?.app.id
    });
  }

  async function refreshApps(): Promise<void> {
    const apps = await listInstalledApps(vfs);
    setInstalledApps(apps);
    if (!installedAppId && apps[0]) setInstalledAppId(apps[0].id);
    log("app-registry", "refreshed installed apps", { count: apps.length });
    await refreshStorageFiles();
  }

  async function refreshStorageFiles(): Promise<void> {
    const files = (await vfs.list("/sd/")).sort();
    setStorageFiles(files);
    log("storage", "refreshed storage listing", { count: files.length });
  }

  async function upload(): Promise<void> {
    const ir = compiled?.ir;
    if (!compiled?.ok || !ir) {
      setStatus("Compile before upload");
      log("upload", "upload blocked without compiled IR");
      return;
    }

    log("upload", "installing IR app", { appId: ir.app.id });
    const base = await installIrApp(vfs, ir, XTEINK_X4_TARGET);
    setInstalledAppId(ir.app.id);
    await refreshApps();
    setStatus(`Uploaded ${base}`);
    log("upload", "installed app files", { base, files: ["app.json", "main.ir.json"] });
    await refreshStorageFiles();
  }

  async function run(): Promise<void> {
    let installed;
    try {
      log("run", "loading installed app", { requestedAppId: installedAppId ?? "first-installed" });
      installed = await loadInstalledApp(vfs, XTEINK_X4_TARGET, installedAppId ?? undefined);
    } catch (error) {
      setStatus(error instanceof Error ? error.message : "Upload an app before run");
      log("run", "load failed", { error: error instanceof Error ? error.message : String(error) });
      return;
    }

    const program = installed.program;
    const nextRuntime = new BrowserRuntime(program, vfs);
    setRuntime(nextRuntime);
    setSnapshot(await nextRuntime.start());
    setInstalledAppId(installed.manifest.id);
    setStatus(`Running ${program.name} from ${installed.basePath}`);
    log("run", "runtime started", { appId: program.id, basePath: installed.basePath, entryType: installed.manifest.entry.type });
  }

  async function uninstallSelectedApp(): Promise<void> {
    if (!installedAppId) {
      setStatus("No installed app selected");
      return;
    }

    await uninstallApp(vfs, installedAppId);
    setRuntime(null);
    setSnapshot(null);
    setStatus(`Uninstalled /sd/apps/${installedAppId}`);
    log("app-registry", "uninstalled app", { appId: installedAppId });
    setInstalledAppId(null);
    await refreshApps();
  }

  async function sendKey(key: LogicalKey): Promise<void> {
    if (!runtime) return;
    const nextSnapshot = await runtime.dispatchKey(key);
    setSnapshot(nextSnapshot);
    log("input", "key dispatched", {
      key,
      currentScreen: nextSnapshot.currentScreen,
      state: nextSnapshot.state,
      exited: nextSnapshot.exited
    });
  }

  function handleButtonDown(key: LogicalKey): void {
    const outcome = buttonArbiter.press(key);
    if (outcome) void handleButtonOutcome(outcome);
    const longPress = XTEINK_X4_TARGET.input.longPress?.find((candidate) => candidate.logical === key);
    if (longPress) {
      window.setTimeout(() => {
        const thresholdOutcome = buttonArbiter.tick();
        if (thresholdOutcome) void handleButtonOutcome(thresholdOutcome);
      }, longPress.durationMs);
    }
  }

  function handleButtonUp(key: LogicalKey): void {
    const outcome = buttonArbiter.release(key);
    if (outcome) void handleButtonOutcome(outcome);
  }

  async function handleButtonOutcome(outcome: ButtonOutcome): Promise<void> {
    log("input", "button outcome", outcome.type === "short"
      ? { type: outcome.type, key: outcome.key }
      : { type: outcome.type, action: outcome.action });

    if (outcome.type === "short") {
      await sendKey(outcome.key);
    } else if (outcome.type === "chord" && outcome.action === "refresh-display") {
      await sendKey("POWER");
    }
  }

  async function resetAppState(): Promise<void> {
    if (runtime) setSnapshot(await runtime.resetState());
    if (installedAppId) await vfs.removePrefix(`/sd/system/app-state/${installedAppId}`);
    setStatus("Reset app state");
    log("state", "reset app state", { appId: installedAppId });
  }

  async function resetStorage(): Promise<void> {
    await vfs.clear();
    setInstalledAppId(null);
    setInstalledApps([]);
    setStorageFiles([]);
    setRuntime(null);
    setSnapshot(null);
    setStatus("Reset simulated /sd");
    log("storage", "cleared simulated /sd");
  }

  async function resetSimulatorState(): Promise<void> {
    await vfs.clear();
    localStorage.removeItem(SOURCE_KEY);
    setSource(DEFAULT_SOURCE);
    setCompiled(null);
    setCompilerBackend("unknown");
    setInstalledAppId(null);
    setInstalledApps([]);
    setStorageFiles([]);
    setRuntime(null);
    setSnapshot(null);
    setDebugEvents([]);
    setStatus("Reset simulator");
  }

  async function resetSimulator(): Promise<void> {
    await resetSimulatorState();
    log("simulator", "reset browser simulator state");
  }

  async function cleanLaunchHelloMenu(): Promise<void> {
    await resetSimulatorState();
    log("simulator", "reset browser simulator state");

    log("compile", "starting compile", { target: XTEINK_X4_TARGET.id, sourceBytes: DEFAULT_SOURCE.length });
    const result = await compileSquid(DEFAULT_SOURCE, XTEINK_X4_TARGET.id);
    setCompiled(result);
    setCompilerBackend(result.backend);
    log("compile", result.ok ? "compile succeeded" : "compile failed", {
      backend: result.backend,
      diagnostics: result.diagnostics.length,
      appId: result.ir?.app.id
    });
    if (!result.ok || !result.ir) {
      setStatus(`Compile failed with ${result.backend.toUpperCase()}`);
      return;
    }

    log("upload", "installing IR app", { appId: result.ir.app.id });
    const base = await installIrApp(vfs, result.ir, XTEINK_X4_TARGET);
    setInstalledAppId(result.ir.app.id);
    await refreshApps();
    log("upload", "installed app files", { base, files: ["app.json", "main.ir.json"] });
    await refreshStorageFiles();

    log("run", "loading installed app", { requestedAppId: result.ir.app.id });
    const installed = await loadInstalledApp(vfs, XTEINK_X4_TARGET, result.ir.app.id);
    const nextRuntime = new BrowserRuntime(installed.program, vfs);
    setRuntime(nextRuntime);
    setSnapshot(await nextRuntime.start());
    setInstalledAppId(installed.manifest.id);
    setStatus(`Running ${installed.program.name} from ${installed.basePath}`);
    log("run", "runtime started", { appId: installed.program.id, basePath: installed.basePath, entryType: installed.manifest.entry.type });
  }

  const commands = snapshot?.drawCommands ?? IDLE_COMMANDS;

  return (
    <main className="app-shell">
      <div
        className="simulator-stage"
        data-testid="runtime-state"
        data-app-id={snapshot?.appId ?? ""}
        data-current-screen={snapshot?.currentScreen ?? "idle"}
        data-selected={snapshot?.state.selected ?? ""}
        data-view={snapshot?.state.view ?? ""}
        data-exited={snapshot?.exited ? "true" : "false"}
      >
        <DeviceSimulator
          target={XTEINK_X4_TARGET}
          commands={commands}
          onButtonDown={handleButtonDown}
          onButtonUp={handleButtonUp}
        />
      </div>
      <section className="editor-pane" aria-label="SquidScript editor">
        <div className="toolbar">
          <button onClick={() => void compile()}><Wrench size={16} />Compile</button>
          <button onClick={() => void upload()}><Upload size={16} />Upload</button>
          <button onClick={() => void run()}><Play size={16} />Run</button>
          <button onClick={() => void refreshApps()}><RefreshCw size={16} />Refresh Apps</button>
          <button onClick={() => void refreshStorageFiles()}><RefreshCw size={16} />Refresh Storage</button>
          <button onClick={() => void uninstallSelectedApp()}><Trash2 size={16} />Uninstall</button>
          <button onClick={() => void resetAppState()}><RotateCcw size={16} />Reset App State</button>
          <button onClick={() => void resetStorage()}><Save size={16} />Reset Storage</button>
          <button onClick={() => void resetSimulator()}><RotateCcw size={16} />Reset Simulator</button>
          <button onClick={() => void cleanLaunchHelloMenu()}><Play size={16} />Clean Launch</button>
        </div>
        <div className="app-picker" aria-label="installed apps">
          <select aria-label="installed app selector" value={installedAppId ?? ""} onChange={(event) => setInstalledAppId(event.target.value || null)}>
            <option value="">No installed app</option>
            {installedApps.map((app) => (
              <option key={app.id} value={app.id}>{app.name} ({app.id})</option>
            ))}
          </select>
          <span>{installedApps.length} installed</span>
        </div>
        <textarea aria-label="Squid source" spellCheck={false} value={source} onChange={(event) => setSource(event.target.value)} />
        <div className="compiler-row" aria-label="compiler backend">Compiler: {compilerBackend.toUpperCase()}</div>
        <div className="status-row" role="status">{status}</div>
        <div className="diagnostics" aria-label="diagnostics">
          {(compiled?.diagnostics.length ?? 0) === 0 ? (
            <div className="diagnostic-empty">No diagnostics</div>
          ) : compiled?.diagnostics.map((diagnostic, index) => (
            <div key={`${diagnostic.code}-${index}`} className={`diagnostic diagnostic-${diagnostic.severity}`}>
              <strong>{diagnostic.code}</strong> {diagnostic.message} <span>{diagnostic.span.start}:{diagnostic.span.end}</span>
            </div>
          ))}
        </div>
        <div className="debug-log" aria-label="debug log">
          {debugEvents.length === 0 ? (
            <div className="debug-empty">No debug events</div>
          ) : debugEvents.map((event, index) => (
            <div key={`${event.at}-${event.scope}-${index}`} className="debug-event">
              <span>{event.at}</span>
              <strong>{event.scope}</strong>
              {event.message}
              <code>{formatDebugData(event.data)}</code>
            </div>
          ))}
        </div>
        <div className="storage-panel" aria-label="storage files">
          {storageFiles.length === 0 ? (
            <div className="storage-empty">No /sd files</div>
          ) : storageFiles.map((path) => (
            <code key={path}>{path}</code>
          ))}
        </div>
      </section>
    </main>
  );
}
