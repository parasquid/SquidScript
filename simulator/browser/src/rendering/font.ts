import { measureBitmapText } from "./bitmapFont";

export interface BitmapFont {
  height: number;
}

export function selectBitmapFont(requested: number | undefined, supported: number[], fallback: number): BitmapFont {
  const height = requested ?? fallback;
  const selected = supported.reduce((best, candidate) => (
    Math.abs(candidate - height) < Math.abs(best - height) ? candidate : best
  ), supported[0] ?? fallback);

  return {
    height: selected
  };
}

export function wrapText(_context: CanvasRenderingContext2D, text: string, maxWidth: number, fontHeight = 20): string[] {
  const words = text.split(/\s+/).filter(Boolean);
  const lines: string[] = [];
  let line = "";

  for (const word of words) {
    const next = line ? `${line} ${word}` : word;
    if (measureBitmapText(next, fontHeight) <= maxWidth || !line) {
      line = next;
    } else {
      lines.push(line);
      line = word;
    }
  }

  if (line) lines.push(line);
  return lines.length > 0 ? lines : [""];
}
