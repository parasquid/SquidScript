import type { CompileResponse, Diagnostic, IrProgram, IrStatement } from "../types";

export const DEFAULT_SOURCE = `app "hello-menu" target "xteink-x4"

state {
  selected: 0
}

onStart() {
  state.load()
  screen.open("main")
}

onKey("DOWN") {
  selected = selected + 1
  state.save()
  screen.refresh()
}

onKey("UP") {
  selected = selected - 1
  state.save()
  screen.refresh()
}

onKey("SELECT") {
  screen.open("detail")
}

onKey("BACK") {
  app.exit()
}

screen("main", { render: "compose" }) {
  display.clear("gray0")
  display.text("Hello Menu", { x: 20, y: 60, w: 440, h: 48, fontHeight: 32, align: "center" })
  display.text("Say Hello", { x: 32, y: 160, w: 416, h: 48, fontHeight: 22, align: "center" })
  display.text("About", { x: 32, y: 216, w: 416, h: 48, fontHeight: 22, align: "center" })
  display.text("Exit", { x: 32, y: 272, w: 416, h: 48, fontHeight: 22, align: "center" })
  display.text("UP/DOWN select  SELECT open", { x: 20, y: 720, w: 440, h: 32, fontHeight: 18, align: "center", textColor: "gray8" })
}

screen("detail") {
  display.clear("gray0")
  display.text("Hello, Squid!", { x: 20, y: 120, w: 440, h: 64, fontHeight: 32, align: "center" })
  display.text("BACK exits this example", { x: 20, y: 720, w: 440, h: 32, fontHeight: 18, align: "center", textColor: "gray8" })
}
`;

export function compileFallback(source: string, targetId: string): CompileResponse {
  const diagnostics: Diagnostic[] = [];
  const appLine = source.match(/^\s*app\s+"([^"]+)"\s+target\s+"([^"]+)"/m);
  const stateSelected = source.match(/selected:\s*(\d+)/);
  const screens = [...source.matchAll(/screen\("([^"]+)"(?:,\s*\{\s*render:\s*"([^"]+)"\s*\})?\)\s*\{([\s\S]*?)\n\}/g)];

  if (!appLine) {
    diagnostics.push(error("E_APP_REQUIRED", "expected app declaration", 0, Math.min(1, source.length)));
  } else if (appLine[2] !== targetId) {
    diagnostics.push(error("E_TARGET_MISMATCH", "source target does not match selected target", appLine.index ?? 0, appLine[0].length));
  }
  if (screens.length === 0) {
    diagnostics.push(error("E_SCREEN_REQUIRED", "expected at least one screen declaration", 0, Math.min(1, source.length)));
  }

  const ok = diagnostics.every((diagnostic) => diagnostic.severity !== "error");
  const appId = appLine?.[1] ?? "hello-menu";
  const ir: IrProgram | null = ok
    ? {
        format: "squidscript-ir",
        version: 1,
        app: { id: appId, name: titleize(appId), target: targetId },
        state: [{ name: "selected", value: Number(stateSelected?.[1] ?? 0) }],
        handlers: [
          { event: "onStart", statements: [{ op: "state.load" }, { op: "screen.open", screen: "main" }] },
          { event: "onKey.DOWN", statements: [{ op: "assign", name: "selected", expr: { op: "binary", left: { op: "state", name: "selected" }, operator: "+", right: { op: "literal", value: 1 } } }, { op: "state.save" }, { op: "screen.refresh" }] },
          { event: "onKey.UP", statements: [{ op: "assign", name: "selected", expr: { op: "binary", left: { op: "state", name: "selected" }, operator: "-", right: { op: "literal", value: 1 } } }, { op: "state.save" }, { op: "screen.refresh" }] },
          { event: "onKey.SELECT", statements: [{ op: "screen.open", screen: "detail" }] },
          { event: "onKey.BACK", statements: [{ op: "app.exit" }] }
        ],
        screens: screens.map((screen) => ({
          name: screen[1],
          render: (screen[2] ?? "compose") as "compose" | "stream",
          statements: parseDisplayStatements(screen[3])
        }))
      }
    : null;

  return { ok, diagnostics, ir };
}

function parseDisplayStatements(body: string): IrStatement[] {
  const statements: IrStatement[] = [];
  for (const match of body.matchAll(/display\.clear\("([^"]+)"\)|display\.text\("([^"]+)",\s*\{([^}]*)\}\)/g)) {
    if (match[1]) statements.push({ op: "display.clear", color: match[1] });
    if (match[2]) statements.push({ op: "display.text", text: match[2], options: parseOptions(match[3]) });
  }
  return statements;
}

function parseOptions(input: string): Record<string, unknown> {
  const options: Record<string, unknown> = {};
  for (const part of input.split(",")) {
    const match = part.match(/\s*([A-Za-z]+):\s*("[^"]+"|\d+)/);
    if (!match) continue;
    options[match[1]] = match[2].startsWith("\"") ? match[2].slice(1, -1) : Number(match[2]);
  }
  return options;
}

function error(code: string, message: string, start: number, length: number): Diagnostic {
  return { code, severity: "error", message, span: { start, end: start + length } };
}

function titleize(id: string): string {
  return id.split(/[-_]/).filter(Boolean).map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1)}`).join(" ");
}
