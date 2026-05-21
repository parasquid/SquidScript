export type DrawCommand =
  | { op: "clear"; gray: number }
  | { op: "rect"; x: number; y: number; width: number; height: number; gray: number; fill?: boolean }
  | { op: "line"; x1: number; y1: number; x2: number; y2: number; gray: number }
  | { op: "text"; x: number; y: number; text: string; gray: number; fontHeight?: number; align?: "left" | "center" | "right"; valign?: "top" | "middle"; maxWidth?: number; boxHeight?: number };
