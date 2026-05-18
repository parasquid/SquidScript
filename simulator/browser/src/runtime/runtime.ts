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

export class BrowserRuntime {
  private state: Record<string, unknown> = {};
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
    await this.executeEvent("onStart");
    if (!this.currentScreen) this.currentScreen = this.program.screens.keys().next().value ?? "";
    if (this.drawCommands.length === 0) this.refresh();
    return this.snapshot();
  }

  async dispatchKey(key: LogicalKey): Promise<RuntimeSnapshot> {
    if (!this.running || this.exited) return this.snapshot();

    await this.executeEvent(`onKey.${key}`);
    return this.snapshot();
  }

  async resetState(): Promise<RuntimeSnapshot> {
    await this.vfs.removePrefix(`/sd/system/app-state/${this.program.id}`);
    this.state = { ...this.program.stateDefaults };
    this.currentScreen = this.program.screens.keys().next().value ?? "";
    this.refresh();
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
      this.refresh();
      return;
    }
    await this.executeStatements(statements);
  }

  private async executeStatements(statements: IrStatement[]): Promise<void> {
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
        this.state[statement.name] = this.evaluateExpr(statement.expr);
      }
    }

    if (needsRefresh) this.refresh();
  }

  private async exit(): Promise<void> {
    await this.saveState();
    this.exited = true;
    this.running = false;
  }

  private refresh(): void {
    if (this.exited) {
      this.drawCommands = [
        { op: "clear", gray: 15 },
        { op: "text", x: 240, y: 380, text: "App exited", gray: 5, fontHeight: 24, align: "center", maxWidth: 420 }
      ];
      return;
    }

    const screen = this.program.screens.get(this.currentScreen);
    const commands = screen ? this.renderScreen(screen.statements) : [{ op: "clear" as const, gray: 15 }];
    this.drawCommands = commands;
  }

  private renderScreen(statements: IrStatement[]): DrawCommand[] {
    const commands: DrawCommand[] = [];
    for (const statement of statements) {
      if (statement.op === "display.clear") {
        commands.push({ op: "clear", gray: colorToGray(statement.color) });
      } else if (statement.op === "display.text") {
        const x = numberOption(statement.options, "x", 0);
        const y = numberOption(statement.options, "y", 0);
        const w = numberOption(statement.options, "w", 480);
        const h = numberOption(statement.options, "h", 0);
        const backgroundColor = stringOption(statement.options, "backgroundColor");
        if (backgroundColor && h > 0) {
          commands.push({ op: "rect", x, y, width: w, height: h, gray: colorToGray(backgroundColor), fill: true });
        }
        const align = alignOption(statement.options);
        commands.push({
          op: "text",
          x: align === "center" ? x + w / 2 : align === "right" ? x + w : x,
          y,
          text: statement.text,
          gray: colorToGray(stringOption(statement.options, "textColor") ?? "gray15"),
          fontHeight: numberOption(statement.options, "fontHeight", undefined),
          align,
          maxWidth: w
        });
      } else if (statement.op === "display.rect") {
        const fillColor = stringOption(statement.options, "fillColor");
        commands.push({
          op: "rect",
          x: statement.x,
          y: statement.y,
          width: statement.w,
          height: statement.h,
          gray: colorToGray(fillColor ?? stringOption(statement.options, "strokeColor") ?? "gray15"),
          fill: Boolean(fillColor)
        });
      } else if (statement.op === "display.line") {
        const x = Math.min(statement.x1, statement.x2);
        const y = Math.min(statement.y1, statement.y2);
        commands.push({
          op: "rect",
          x,
          y,
          width: Math.max(1, Math.abs(statement.x2 - statement.x1)),
          height: Math.max(1, Math.abs(statement.y2 - statement.y1)),
          gray: colorToGray(stringOption(statement.options, "color") ?? "gray15"),
          fill: true
        });
      }
    }
    return commands.length > 0 ? commands : [{ op: "clear", gray: 15 }];
  }

  private evaluateExpr(expr: IrExpr): unknown {
    if (expr.op === "literal") return expr.value;
    if (expr.op === "state") return this.state[expr.name] ?? 0;

    const left = Number(this.evaluateExpr(expr.left) ?? 0);
    const right = Number(this.evaluateExpr(expr.right) ?? 0);
    return expr.operator === "+" ? left + right : left - right;
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
  if (color === "black") return 0;
  if (color === "white") return 15;
  const match = color.match(/^gray(\d+)$/);
  if (!match) return 15;
  return Math.min(15, Math.max(0, Number(match[1])));
}

function numberOption(options: Record<string, unknown>, key: string, fallback: number): number;
function numberOption(options: Record<string, unknown>, key: string, fallback: undefined): number | undefined;
function numberOption(options: Record<string, unknown>, key: string, fallback: number | undefined): number | undefined {
  const value = options[key];
  return typeof value === "number" ? value : fallback;
}

function stringOption(options: Record<string, unknown>, key: string): string | undefined {
  const value = options[key];
  return typeof value === "string" ? value : undefined;
}

function alignOption(options: Record<string, unknown>): "left" | "center" | "right" | undefined {
  const align = stringOption(options, "align");
  return align === "left" || align === "center" || align === "right" ? align : undefined;
}
