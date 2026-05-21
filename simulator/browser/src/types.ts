export type LogicalKey = "UP" | "DOWN" | "LEFT" | "RIGHT" | "SELECT" | "BACK" | "POWER";

export interface SourceSpan {
  start: number;
  end: number;
}

export interface Diagnostic {
  code: string;
  severity: "error" | "warning" | "info";
  message: string;
  span: SourceSpan;
}

export interface IrProgram {
  format: "squidscript-ir";
  version: 1;
  app: {
    id: string;
    name: string;
    target: string;
  };
  state: Array<{ name: string; value: unknown }>;
  functions: IrFunction[];
  handlers: Array<{ event: string; statements: IrStatement[] }>;
  screens: IrScreen[];
}

export type IrExpr =
  | { op: "literal"; value: unknown }
  | { op: "state"; name: string }
  | { op: "binary"; left: IrExpr; operator: "+" | "-" | "==" | "!=" | "<" | "<=" | ">" | ">="; right: IrExpr }
  | { op: "unary"; operator: "!"; expr: IrExpr }
  | { op: "field"; target: IrExpr; field: string }
  | { op: "call"; name: string; args: IrExpr[] };

export type IrStatement =
  | { op: "state.load" }
  | { op: "state.save" }
  | { op: "screen.open"; screen: string }
  | { op: "screen.refresh" }
  | { op: "app.exit" }
  | { op: "assign"; name: string; expr: IrExpr }
  | { op: "let"; name: string; expr: IrExpr }
  | { op: "if"; condition: IrExpr; then_statements: IrStatement[]; else_statements: IrStatement[] }
  | { op: "repeat"; count: IrExpr; statements: IrStatement[] }
  | { op: "for"; item: string; list: IrExpr; max?: IrExpr | null; statements: IrStatement[] }
  | { op: "return"; expr?: IrExpr | null }
  | { op: "call"; name: string; args: IrExpr[] }
  | { op: "debug.print"; args: IrExpr[] }
  | { op: "debug.block"; statements: IrStatement[] }
  | { op: "service.display.clear"; color: string }
  | { op: "service.display.text"; text: IrExpr; options: Record<string, unknown> }
  | { op: "service.display.rect"; x: number; y: number; w: number; h: number; options: Record<string, unknown> }
  | { op: "service.display.line"; x1: number; y1: number; x2: number; y2: number; options: Record<string, unknown> };

export interface IrFunction {
  name: string;
  params: string[];
  statements: IrStatement[];
}

export interface IrScreen {
  name: string;
  render: "compose" | "stream";
  statements: IrStatement[];
}

export interface CompileResponse {
  ok: boolean;
  diagnostics: Diagnostic[];
  ir: IrProgram | null;
}

export type CompilerBackend = "wasm";

export interface CompileResult extends CompileResponse {
  backend: CompilerBackend;
  sqbc?: Uint8Array;
}

export type DrawCommand =
  | { op: "clear"; gray: number }
  | { op: "rect"; x: number; y: number; width: number; height: number; gray: number; fill?: boolean }
  | { op: "line"; x1: number; y1: number; x2: number; y2: number; gray: number }
  | { op: "text"; x: number; y: number; text: string; gray: number; fontHeight?: number; align?: "left" | "center" | "right"; valign?: "top" | "middle"; maxWidth?: number; boxHeight?: number };

export interface TargetDefinition {
  format: string;
  id: string;
  name: string;
  display: {
    logical: { width: number; height: number; rotation: number };
    color: { logicalGrayscaleLevels: number; defaultPixelFormat: string; supportedPixelFormats: string[] };
    text: { fontHeights: { supported: number[]; default: number; selection: string } };
    rendering: { screenPolicies: string[]; defaultPolicy: string; supportedModes: string[]; defaultMode: string };
  };
  input: {
    buttons: Array<{ logical: LogicalKey; type: string }>;
    longPress?: Array<{ logical: LogicalKey; durationMs: number; action: string }>;
    chords?: Array<{ logical: LogicalKey[]; name: string; windowMs: number; action: string }>;
  };
  storage: { mount: string; supportsApps: boolean };
  simulator?: { layout?: string; defaultBackend?: string };
  compatibility: string[];
  features: string[];
}
