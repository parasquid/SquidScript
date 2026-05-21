import type { DrawCommand, IrExpr, IrStatement, LogicalKey, RuntimeProgram } from "../types";
import type { Vfs } from "../storage/vfs";

export interface RuntimeSnapshot {
  appId: string;
  running: boolean;
  exited: boolean;
  currentScreen: string;
  state: Record<string, unknown>;
  drawCommands: DrawCommand[];
}

interface ExecResult {
  returned: boolean;
  value?: unknown;
}

export class BrowserRuntime {
  private state: Record<string, unknown> = {};
  private locals: Array<Record<string, unknown>> = [];
  private currentScreen = "";
  private running = false;
  private exited = false;
  private drawCommands: DrawCommand[] = [];

  constructor(
    private readonly program: RuntimeProgram,
    private readonly vfs: Vfs
  ) {}

  async start(): Promise<RuntimeSnapshot> {
    this.running = true;
    this.exited = false;
    this.currentScreen = "";
    this.state = { ...this.program.stateDefaults, ...(await this.loadState()) };
    await this.executeEvent("app.start");
    if (!this.currentScreen) this.currentScreen = this.program.screens.keys().next().value ?? "";
    if (this.drawCommands.length === 0) await this.refresh();
    return this.snapshot();
  }

  async dispatchKey(key: LogicalKey): Promise<RuntimeSnapshot> {
    if (!this.running || this.exited) return this.snapshot();

    await this.executeEvent(`key.${key}`);
    return this.snapshot();
  }

  async resetState(): Promise<RuntimeSnapshot> {
    await this.vfs.removePrefix(`/sd/system/app-state/${this.program.id}`);
    this.state = { ...this.program.stateDefaults };
    this.currentScreen = this.program.screens.keys().next().value ?? "";
    await this.refresh();
    return this.snapshot();
  }

  snapshot(): RuntimeSnapshot {
    return {
      appId: this.program.id,
      running: this.running,
      exited: this.exited,
      currentScreen: this.currentScreen,
      state: { ...this.state },
      drawCommands: [...this.drawCommands]
    };
  }

  private async executeEvent(event: string): Promise<void> {
    const statements = this.program.handlers.get(event);
    if (!statements) {
      await this.refresh();
      return;
    }
    this.locals.push({});
    try {
    await this.executeStatements(statements);
    } finally {
      this.locals.pop();
    }
  }

  private async executeStatements(statements: IrStatement[], renderCommands?: DrawCommand[]): Promise<ExecResult> {
    let needsRefresh = false;
    for (const statement of statements) {
      if (statement.op === "state.load") {
        this.state = { ...this.program.stateDefaults, ...(await this.loadState()) };
      } else if (statement.op === "state.save") {
        await this.saveState();
      } else if (statement.op === "screen.open") {
        this.currentScreen = statement.screen;
        needsRefresh = true;
      } else if (statement.op === "screen.refresh") {
        needsRefresh = true;
      } else if (statement.op === "app.exit") {
        await this.exit();
        needsRefresh = true;
      } else if (statement.op === "assign") {
        this.state[statement.name] = await this.evaluateExpr(statement.expr);
      } else if (statement.op === "let") {
        this.currentLocals()[statement.name] = await this.evaluateExpr(statement.expr);
      } else if (statement.op === "if") {
        const branch = this.isTruthy(await this.evaluateExpr(statement.condition)) ? statement.then_statements : statement.else_statements;
        const result = await this.executeStatements(branch, renderCommands);
        if (result.returned) return result;
      } else if (statement.op === "repeat") {
        const count = Math.max(0, Number(await this.evaluateExpr(statement.count) ?? 0));
        for (let index = 0; index < count; index += 1) {
          const result = await this.executeStatements(statement.statements, renderCommands);
          if (result.returned) return result;
        }
      } else if (statement.op === "for") {
        const list = await this.evaluateExpr(statement.list);
        const values = Array.isArray(list) ? list : [];
        const max = statement.max ? Math.max(0, Number(await this.evaluateExpr(statement.max) ?? values.length)) : values.length;
        for (const value of values.slice(0, max)) {
          this.currentLocals()[statement.item] = value;
          const result = await this.executeStatements(statement.statements, renderCommands);
          if (result.returned) return result;
        }
      } else if (statement.op === "call") {
        await this.callFunction(statement.name, await Promise.all(statement.args.map((arg) => this.evaluateExpr(arg))), renderCommands);
      } else if (statement.op === "return") {
        return { returned: true, value: statement.expr ? await this.evaluateExpr(statement.expr) : undefined };
      } else if (renderCommands) {
        await this.appendDrawCommand(statement, renderCommands);
      }
    }

    if (needsRefresh) await this.refresh();
    return { returned: false };
  }

  private async exit(): Promise<void> {
    await this.saveState();
    this.exited = true;
    this.running = false;
  }

  private async refresh(): Promise<void> {
    if (this.exited) {
      this.drawCommands = [
        { op: "clear", gray: 0 },
        { op: "text", x: 240, y: 380, text: "App exited", gray: 5, fontHeight: 24, align: "center", maxWidth: 420 }
      ];
      return;
    }

    const screen = this.program.screens.get(this.currentScreen);
    const commands = screen ? await this.renderScreen(screen.statements) : [{ op: "clear" as const, gray: 0 }];
    this.drawCommands = commands;
  }

  private async renderScreen(statements: IrStatement[]): Promise<DrawCommand[]> {
    const commands: DrawCommand[] = [];
    this.locals.push({});
    try {
      await this.executeStatements(statements, commands);
    } finally {
      this.locals.pop();
    }
    return commands.length > 0 ? commands : [{ op: "clear", gray: 0 }];
  }

  private async appendDrawCommand(statement: IrStatement, commands: DrawCommand[]): Promise<void> {
    if (statement.op === "display.clear") {
      commands.push({ op: "clear", gray: colorToGray(statement.color) });
    } else if (statement.op === "display.text") {
      const text = await this.evaluateValue(statement.text);
      const x = await this.numberOption(statement.options, "x", 0);
      const y = await this.numberOption(statement.options, "y", 0);
      const w = await this.numberOption(statement.options, "w", 480);
      const h = await this.numberOption(statement.options, "h", 0);
      const backgroundColor = await this.stringOption(statement.options, "backgroundColor");
      if (backgroundColor && h > 0) {
        commands.push({ op: "rect", x, y, width: w, height: h, gray: colorToGray(backgroundColor), fill: true });
      }
      const align = await this.alignOption(statement.options);
      const valign = await this.valignOption(statement.options);
      commands.push({
        op: "text",
        x: align === "center" ? x + w / 2 : align === "right" ? x + w : x,
        y,
        text: String(text ?? ""),
        gray: colorToGray(await this.stringOption(statement.options, "textColor") ?? "gray15"),
        fontHeight: await this.numberOption(statement.options, "fontHeight", undefined),
        align,
        valign,
        maxWidth: w,
        boxHeight: h || undefined
      });
    } else if (statement.op === "display.rect") {
      const fillColor = await this.stringOption(statement.options, "fillColor");
      commands.push({
        op: "rect",
        x: statement.x,
        y: statement.y,
        width: statement.w,
        height: statement.h,
        gray: colorToGray(fillColor ?? await this.stringOption(statement.options, "strokeColor") ?? "gray15"),
        fill: Boolean(fillColor)
      });
    } else if (statement.op === "display.line") {
      commands.push({
        op: "line",
          x1: statement.x1,
          y1: statement.y1,
          x2: statement.x2,
          y2: statement.y2,
          gray: colorToGray(await this.stringOption(statement.options, "color") ?? "gray15")
      });
    }
  }

  private async evaluateExpr(expr: IrExpr): Promise<unknown> {
    if (expr.op === "literal") return expr.value;
    if (expr.op === "state") return this.resolveName(expr.name);
    if (expr.op === "call") return this.callFunction(expr.name, await Promise.all(expr.args.map((arg) => this.evaluateExpr(arg))));
    if (expr.op === "unary") return !this.isTruthy(await this.evaluateExpr(expr.expr));
    if (expr.op === "field") {
      const target = await this.evaluateExpr(expr.target);
      if (!isRecordValue(target) || !Object.hasOwn(target, expr.field)) {
        throw new Error(`Missing record field ${expr.field}`);
      }
      return target[expr.field];
    }

    const leftValue = await this.evaluateExpr(expr.left);
    const rightValue = await this.evaluateExpr(expr.right);
    const left = Number(leftValue ?? 0);
    const right = Number(rightValue ?? 0);
    if (expr.operator === "+") return left + right;
    if (expr.operator === "-") return left - right;
    if (expr.operator === "==") return leftValue === rightValue;
    if (expr.operator === "!=") return leftValue !== rightValue;
    if (expr.operator === "<") return left < right;
    if (expr.operator === "<=") return left <= right;
    if (expr.operator === ">") return left > right;
    return left >= right;
  }

  private async callFunction(name: string, args: unknown[], renderCommands?: DrawCommand[]): Promise<unknown> {
    const fn = this.program.functions.get(name);
    if (!fn) {
      if (isFallibleBuiltin(name)) return { ok: false, error: "unsupported" };
      return undefined;
    }
    const frame: Record<string, unknown> = {};
    fn.params.forEach((param, index) => {
      frame[param] = args[index];
    });
    this.locals.push(frame);
    try {
      const result = await this.executeStatements(fn.statements, renderCommands);
      return result.value;
    } finally {
      this.locals.pop();
    }
  }

  private currentLocals(): Record<string, unknown> {
    if (this.locals.length === 0) this.locals.push({});
    return this.locals[this.locals.length - 1];
  }

  private resolveName(name: string): unknown {
    for (let index = this.locals.length - 1; index >= 0; index -= 1) {
      if (Object.hasOwn(this.locals[index], name)) return this.locals[index][name];
    }
    return this.state[name] ?? 0;
  }

  private isTruthy(value: unknown): boolean {
    return value !== false && value !== null && value !== undefined && value !== 0 && value !== "";
  }

  private async optionValue(options: Record<string, unknown>, key: string): Promise<unknown> {
    const value = options[key];
    return this.evaluateValue(value);
  }

  private async evaluateValue(value: unknown): Promise<unknown> {
    return isIrExpr(value) ? this.evaluateExpr(value) : value;
  }

  private async numberOption(options: Record<string, unknown>, key: string, fallback: number): Promise<number>;
  private async numberOption(options: Record<string, unknown>, key: string, fallback: undefined): Promise<number | undefined>;
  private async numberOption(options: Record<string, unknown>, key: string, fallback: number | undefined): Promise<number | undefined> {
    const value = await this.optionValue(options, key);
    return typeof value === "number" ? value : fallback;
  }

  private async stringOption(options: Record<string, unknown>, key: string): Promise<string | undefined> {
    const value = await this.optionValue(options, key);
    return typeof value === "string" ? value : undefined;
  }

  private async alignOption(options: Record<string, unknown>): Promise<"left" | "center" | "right" | undefined> {
    const align = await this.stringOption(options, "align");
    return align === "left" || align === "center" || align === "right" ? align : undefined;
  }

  private async valignOption(options: Record<string, unknown>): Promise<"top" | "middle" | undefined> {
    const valign = await this.stringOption(options, "valign");
    return valign === "top" || valign === "middle" ? valign : undefined;
  }

  private async loadState(): Promise<Record<string, unknown>> {
    const raw = await this.vfs.read(`/sd/system/app-state/${this.program.id}/state.json`);
    if (!raw) return {};
    try {
      return JSON.parse(raw) as Record<string, unknown>;
    } catch {
      return {};
    }
  }

  private async saveState(): Promise<void> {
    await this.vfs.write(`/sd/system/app-state/${this.program.id}/state.json`, JSON.stringify(this.state));
  }
}

function colorToGray(color: string): number {
  if (color === "black") return 15;
  if (color === "white") return 0;
  const match = color.match(/^gray(\d+)$/);
  if (!match) return 15;
  return Math.min(15, Math.max(0, Number(match[1])));
}

function isIrExpr(value: unknown): value is IrExpr {
  return typeof value === "object"
    && value !== null
    && "op" in value
    && ["literal", "state", "binary", "unary", "field", "call"].includes(String((value as { op?: unknown }).op));
}

function isRecordValue(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isFallibleBuiltin(name: string): boolean {
  return [
    "content.pickFile",
    "content.readText",
    "content.readLines",
    "data.read",
    "library.list",
    "library.volumes",
    "library.mkdir",
    "library.rename",
    "library.move",
    "library.delete",
    "library.installUpload",
    "binbook.open",
    "binbook.inspect",
    "wifi.connect",
    "wifi.disconnect",
    "wifi.scan",
    "wifi.startAP",
    "wifi.stopAP",
    "wifi.setIP",
    "wifi.setAPIP",
    "wifi.setHostname",
    "wifi.openSetup",
    "httpServer.start",
    "httpServer.stop",
    "httpServer.poll",
    "bleTransfer.start",
    "bleTransfer.stop",
    "bleTransfer.poll"
  ].includes(name);
}
