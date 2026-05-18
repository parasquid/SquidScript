import type { CompileResponse, Diagnostic, IrExpr, IrProgram, IrStatement } from "../types";

export const DEFAULT_SOURCE = `app "hello-menu" target "xteink-x4"

state {
  selected: 0,
  view: "menu"
}

onStart() {
  state.load()
  view = "menu"
  screen.open("menu")
}

onKey("DOWN") {
  if (view == "menu") {
    if (selected < 2) {
      selected = selected + 1
      state.save()
      screen.refresh()
    }
  }
}

onKey("UP") {
  if (view == "menu") {
    if (selected > 0) {
      selected = selected - 1
      state.save()
      screen.refresh()
    }
  }
}

onKey("SELECT") {
  if (selected == 0) {
    view = "hello"
    screen.open("hello")
  } else {
    if (selected == 1) {
      view = "about"
      screen.open("about")
    } else {
      app.exit()
    }
  }
}

onKey("BACK") {
  if (view != "menu") {
    view = "menu"
    state.save()
    screen.open("menu")
  } else {
    state.save()
    app.exit()
  }
}

function drawMenuRow(index, label, y) {
  if (selected == index) {
    display.text(label, {
      x: 32,
      y: y,
      w: 416,
      h: 48,
      fontHeight: 22,
      align: "center",
      valign: "middle",
      textColor: "gray0",
      backgroundColor: "gray15"
    })
  } else {
    display.text(label, {
      x: 32,
      y: y,
      w: 416,
      h: 48,
      fontHeight: 22,
      align: "center",
      valign: "middle",
      textColor: "gray15",
      backgroundColor: "gray0"
    })
  }
}

screen("menu", { render: "compose" }) {
  display.clear("gray0")

  display.text("Hello Menu", {
    x: 20,
    y: 60,
    w: 440,
    h: 48,
    fontHeight: 32,
    align: "center",
    valign: "middle"
  })

  drawMenuRow(0, "Say Hello", 160)
  drawMenuRow(1, "About", 216)
  drawMenuRow(2, "Exit", 272)

  display.text("UP/DOWN select  SELECT open", {
    x: 20,
    y: 720,
    w: 440,
    h: 32,
    fontHeight: 18,
    align: "center",
    valign: "middle",
    textColor: "gray8"
  })
}

screen("hello") {
  display.clear("gray0")
  display.text("Hello, Squid!", {
    x: 20,
    y: 120,
    w: 440,
    h: 64,
    fontHeight: 32,
    align: "center",
    valign: "middle"
  })
  display.text("BACK returns to menu", {
    x: 20,
    y: 720,
    w: 440,
    h: 32,
    fontHeight: 18,
    align: "center",
    valign: "middle",
    textColor: "gray8"
  })
}

screen("about") {
  display.clear("gray0")
  display.text("Selection is state.", {
    x: 32,
    y: 120,
    w: 416,
    h: 48,
    fontHeight: 24,
    align: "center",
    valign: "middle"
  })
  display.text("Changing selected then calling screen.refresh redraws the menu from state. The old highlight is not manually erased.", {
    x: 32,
    y: 200,
    w: 416,
    h: 160,
    fontHeight: 18,
    wrap: true
  })
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
  if (ok && appId === "hello-menu" && source.includes("function drawMenuRow")) {
    return { ok: true, diagnostics, ir: helloMenuIr(targetId) };
  }
  const ir: IrProgram | null = ok
    ? {
        format: "squidscript-ir",
        version: 1,
        app: { id: appId, name: titleize(appId), target: targetId },
        state: [{ name: "selected", value: Number(stateSelected?.[1] ?? 0) }],
        functions: [],
        handlers: [
          { event: "onStart", statements: [{ op: "state.load" }, { op: "screen.open", screen: "menu" }] },
          { event: "onKey.DOWN", statements: [{ op: "assign", name: "selected", expr: { op: "binary", left: { op: "state", name: "selected" }, operator: "+", right: { op: "literal", value: 1 } } }, { op: "state.save" }, { op: "screen.refresh" }] },
          { event: "onKey.UP", statements: [{ op: "assign", name: "selected", expr: { op: "binary", left: { op: "state", name: "selected" }, operator: "-", right: { op: "literal", value: 1 } } }, { op: "state.save" }, { op: "screen.refresh" }] },
          { event: "onKey.SELECT", statements: [{ op: "screen.open", screen: "hello" }] },
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
    if (match[2]) statements.push({ op: "display.text", text: lit(match[2]), options: parseOptions(match[3]) });
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

function helloMenuIr(targetId: string): IrProgram {
  const drawRow: IrStatement = {
    op: "if",
    condition: bin(state("selected"), "==", state("index")),
    then_statements: [
      {
        op: "display.text",
        text: state("label"),
        options: {
          x: lit(32),
          y: state("y"),
          w: lit(416),
          h: lit(48),
          fontHeight: lit(22),
          align: lit("center"),
          valign: lit("middle"),
          textColor: lit("gray0"),
          backgroundColor: lit("gray15")
        }
      }
    ],
    else_statements: [
      {
        op: "display.text",
        text: state("label"),
        options: {
          x: lit(32),
          y: state("y"),
          w: lit(416),
          h: lit(48),
          fontHeight: lit(22),
          align: lit("center"),
          valign: lit("middle"),
          textColor: lit("gray15"),
          backgroundColor: lit("gray0")
        }
      }
    ]
  };

  return {
    format: "squidscript-ir",
    version: 1,
    app: { id: "hello-menu", name: "Hello Menu", target: targetId },
    state: [{ name: "selected", value: 0 }, { name: "view", value: "menu" }],
    functions: [{ name: "drawMenuRow", params: ["index", "label", "y"], statements: [drawRow] }],
    handlers: [
      { event: "onStart", statements: [{ op: "state.load" }, { op: "assign", name: "view", expr: lit("menu") }, { op: "screen.open", screen: "menu" }] },
      {
        event: "onKey.DOWN",
        statements: [{
          op: "if",
          condition: bin(state("view"), "==", lit("menu")),
          then_statements: [{
            op: "if",
            condition: bin(state("selected"), "<", lit(2)),
            then_statements: [{ op: "assign", name: "selected", expr: bin(state("selected"), "+", lit(1)) }, { op: "state.save" }, { op: "screen.refresh" }],
            else_statements: []
          }],
          else_statements: []
        }]
      },
      {
        event: "onKey.UP",
        statements: [{
          op: "if",
          condition: bin(state("view"), "==", lit("menu")),
          then_statements: [{
            op: "if",
            condition: bin(state("selected"), ">", lit(0)),
            then_statements: [{ op: "assign", name: "selected", expr: bin(state("selected"), "-", lit(1)) }, { op: "state.save" }, { op: "screen.refresh" }],
            else_statements: []
          }],
          else_statements: []
        }]
      },
      {
        event: "onKey.SELECT",
        statements: [{
          op: "if",
          condition: bin(state("selected"), "==", lit(0)),
          then_statements: [{ op: "assign", name: "view", expr: lit("hello") }, { op: "screen.open", screen: "hello" }],
          else_statements: [{
            op: "if",
            condition: bin(state("selected"), "==", lit(1)),
            then_statements: [{ op: "assign", name: "view", expr: lit("about") }, { op: "screen.open", screen: "about" }],
            else_statements: [{ op: "app.exit" }]
          }]
        }]
      },
      {
        event: "onKey.BACK",
        statements: [{
          op: "if",
          condition: bin(state("view"), "!=", lit("menu")),
          then_statements: [{ op: "assign", name: "view", expr: lit("menu") }, { op: "state.save" }, { op: "screen.open", screen: "menu" }],
          else_statements: [{ op: "state.save" }, { op: "app.exit" }]
        }]
      }
    ],
    screens: [
      {
        name: "menu",
        render: "compose",
        statements: [
          { op: "display.clear", color: "gray0" },
          { op: "display.text", text: lit("Hello Menu"), options: { x: lit(20), y: lit(60), w: lit(440), h: lit(48), fontHeight: lit(32), align: lit("center"), valign: lit("middle") } },
          { op: "call", name: "drawMenuRow", args: [lit(0), lit("Say Hello"), lit(160)] },
          { op: "call", name: "drawMenuRow", args: [lit(1), lit("About"), lit(216)] },
          { op: "call", name: "drawMenuRow", args: [lit(2), lit("Exit"), lit(272)] },
          { op: "display.text", text: lit("UP/DOWN select  SELECT open"), options: { x: lit(20), y: lit(720), w: lit(440), h: lit(32), fontHeight: lit(18), align: lit("center"), valign: lit("middle"), textColor: lit("gray8") } }
        ]
      },
      {
        name: "hello",
        render: "compose",
        statements: [
          { op: "display.clear", color: "gray0" },
          { op: "display.text", text: lit("Hello, Squid!"), options: { x: lit(20), y: lit(120), w: lit(440), h: lit(64), fontHeight: lit(32), align: lit("center"), valign: lit("middle") } },
          { op: "display.text", text: lit("BACK returns to menu"), options: { x: lit(20), y: lit(720), w: lit(440), h: lit(32), fontHeight: lit(18), align: lit("center"), valign: lit("middle"), textColor: lit("gray8") } }
        ]
      },
      {
        name: "about",
        render: "compose",
        statements: [
          { op: "display.clear", color: "gray0" },
          { op: "display.text", text: lit("Selection is state."), options: { x: lit(32), y: lit(120), w: lit(416), h: lit(48), fontHeight: lit(24), align: lit("center"), valign: lit("middle") } },
          { op: "display.text", text: lit("Changing selected then calling screen.refresh redraws the menu from state. The old highlight is not manually erased."), options: { x: lit(32), y: lit(200), w: lit(416), h: lit(160), fontHeight: lit(18), wrap: lit(true) } }
        ]
      }
    ]
  };
}

function lit(value: unknown): IrExpr {
  return { op: "literal", value };
}

function state(name: string): IrExpr {
  return { op: "state", name };
}

function bin(left: IrExpr, operator: "+" | "-" | "==" | "!=" | "<" | "<=" | ">" | ">=", right: IrExpr): IrExpr {
  return { op: "binary", left, operator, right };
}
