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
  | { op: "display.clear"; color: string }
  | { op: "display.text"; text: IrExpr; options: Record<string, unknown> }
  | { op: "display.rect"; x: number; y: number; w: number; h: number; options: Record<string, unknown> }
  | { op: "display.line"; x1: number; y1: number; x2: number; y2: number; options: Record<string, unknown> };

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

export type CompilerBackend = "wasm" | "fallback";

export interface CompileResult extends CompileResponse {
  backend: CompilerBackend;
}

export interface AppManifest {
  format: "squidapp-v1";
  id: string;
  name: string;
  kind: "app";
  version: string;
  runtime: {
    language: "squidscript";
    version: string;
  };
  entry: {
    type: "ir" | "bytecode";
    file: string;
    browserSimOnly?: boolean;
  };
  permissions: string[];
  requires: {
    runtime: string;
    display: {
      minWidth: number;
      minHeight: number;
      pixelFormats: string[];
    };
    keys: LogicalKey[];
    features: string[];
  };
}

export interface RuntimeProgram {
  id: string;
  name: string;
  target: string;
  stateDefaults: Record<string, unknown>;
  functions: Map<string, IrFunction>;
  handlers: Map<string, IrStatement[]>;
  screens: Map<string, IrScreen>;
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
