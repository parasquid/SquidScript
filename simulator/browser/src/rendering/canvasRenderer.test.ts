import { describe, expect, it } from "vitest";
import { grayToCss, renderCommands } from "./canvasRenderer";
import { glyphFor, measureBitmapText } from "./bitmapFont";
import { selectBitmapFont } from "./font";
import { XTEINK_X4_TARGET } from "../target/target";

describe("renderer helpers", () => {
  it("maps 16-level grayscale deterministically", () => {
    expect(grayToCss(0)).toBe("rgb(255, 255, 255)");
    expect(grayToCss(15)).toBe("rgb(0, 0, 0)");
  });

  it("selects nearest supported font height", () => {
    expect(selectBitmapFont(21, [16, 20, 24], 20).height).toBe(20);
    expect(selectBitmapFont(23, [16, 20, 24], 20).height).toBe(24);
  });

  it("uses deterministic bitmap glyph metrics and fallback glyphs", () => {
    expect(measureBitmapText("ABC", 20)).toBeGreaterThan(measureBitmapText("AB", 20));
    expect(glyphFor("A")).toEqual(glyphFor("a"));
    expect(glyphFor(",")).toEqual(["00000", "00000", "00000", "00000", "00000", "00100", "01000"]);
    expect(glyphFor("!")).toEqual(["00100", "00100", "00100", "00100", "00100", "00000", "00100"]);
    expect(glyphFor("~")).toHaveLength(7);
  });

  it("renders source-order commands including real line commands", () => {
    const calls: string[] = [];
    const context = {
      fillStyle: "",
      strokeStyle: "",
      save: () => calls.push("save"),
      beginPath: () => calls.push("beginPath"),
      rect: (x: number, y: number, w: number, h: number) => calls.push(`clipRect:${x}:${y}:${w}:${h}`),
      clip: () => calls.push("clip"),
      fillRect: (x: number, y: number, w: number, h: number) => calls.push(`fillRect:${x}:${y}:${w}:${h}`),
      strokeRect: (x: number, y: number, w: number, h: number) => calls.push(`strokeRect:${x}:${y}:${w}:${h}`),
      moveTo: (x: number, y: number) => calls.push(`moveTo:${x}:${y}`),
      lineTo: (x: number, y: number) => calls.push(`lineTo:${x}:${y}`),
      stroke: () => calls.push("stroke"),
      restore: () => calls.push("restore")
    };
    const canvas = {
      width: 0,
      height: 0,
      getContext: () => context
    } as unknown as HTMLCanvasElement;

    renderCommands(canvas, [
      { op: "clear", gray: 0 },
      { op: "line", x1: 1, y1: 2, x2: 3, y2: 4, gray: 15 },
      { op: "rect", x: 5, y: 6, width: 7, height: 8, gray: 4, fill: true }
    ], XTEINK_X4_TARGET);

    expect(calls).toEqual(expect.arrayContaining(["moveTo:1:2", "lineTo:3:4", "fillRect:5:6:7:8"]));
    expect(calls.indexOf("moveTo:1:2")).toBeLessThan(calls.indexOf("fillRect:5:6:7:8"));
  });
});
