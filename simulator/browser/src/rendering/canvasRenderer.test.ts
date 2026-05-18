import { describe, expect, it } from "vitest";
import { grayToCss } from "./canvasRenderer";
import { glyphFor, measureBitmapText } from "./bitmapFont";
import { selectBitmapFont } from "./font";

describe("renderer helpers", () => {
  it("maps 16-level grayscale deterministically", () => {
    expect(grayToCss(0)).toBe("rgb(0, 0, 0)");
    expect(grayToCss(15)).toBe("rgb(255, 255, 255)");
  });

  it("selects nearest supported font height", () => {
    expect(selectBitmapFont(21, [16, 20, 24], 20).height).toBe(20);
    expect(selectBitmapFont(23, [16, 20, 24], 20).height).toBe(24);
  });

  it("uses deterministic bitmap glyph metrics and fallback glyphs", () => {
    expect(measureBitmapText("ABC", 20)).toBeGreaterThan(measureBitmapText("AB", 20));
    expect(glyphFor("A")).toEqual(glyphFor("a"));
    expect(glyphFor("~")).toHaveLength(7);
  });
});
