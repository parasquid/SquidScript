import type { DrawCommand, TargetDefinition } from "../types";
import { drawBitmapText, measureBitmapText } from "./bitmapFont";
import { selectBitmapFont, wrapText } from "./font";

export interface RenderResult {
  ok: boolean;
  width: number;
  height: number;
  commandCount: number;
  firstCommand: string;
  message: string;
}

export function grayToCss(gray: number, levels = 16): string {
  const clamped = Math.max(0, Math.min(levels - 1, Math.round(gray)));
  const value = Math.round(((levels - 1 - clamped) / (levels - 1)) * 255);
  return `rgb(${value}, ${value}, ${value})`;
}

export function renderCommands(canvas: HTMLCanvasElement, commands: DrawCommand[], target: TargetDefinition): RenderResult {
  const context = canvas.getContext("2d");
  const { width, height } = target.display.logical;
  const firstCommand = commands[0]?.op ?? "none";
  if (!context) {
    return { ok: false, width, height, commandCount: commands.length, firstCommand, message: "2D canvas context unavailable" };
  }

  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;

  context.save();
  try {
    context.beginPath();
    context.rect(0, 0, width, height);
    context.clip();

    for (const command of commands) {
      if (command.op === "clear") {
        context.fillStyle = grayToCss(command.gray, target.display.color.logicalGrayscaleLevels);
        context.fillRect(0, 0, width, height);
      }

      if (command.op === "rect") {
        context.strokeStyle = grayToCss(command.gray, target.display.color.logicalGrayscaleLevels);
        context.fillStyle = context.strokeStyle;
        if (command.fill) context.fillRect(command.x, command.y, command.width, command.height);
        else context.strokeRect(command.x, command.y, command.width, command.height);
      }

      if (command.op === "line") {
        context.strokeStyle = grayToCss(command.gray, target.display.color.logicalGrayscaleLevels);
        context.beginPath();
        context.moveTo(command.x1, command.y1);
        context.lineTo(command.x2, command.y2);
        context.stroke();
      }

      if (command.op === "text") {
        const font = selectBitmapFont(command.fontHeight, target.display.text.fontHeights.supported, target.display.text.fontHeights.default);
        context.fillStyle = grayToCss(command.gray, target.display.color.logicalGrayscaleLevels);
        const maxWidth = command.maxWidth ?? width - command.x;
        const lines = wrapText(context, command.text, maxWidth, font.height);
        const lineStep = Math.round(font.height * 1.25);
        const totalHeight = font.height + Math.max(0, lines.length - 1) * lineStep;
        const top = command.valign === "middle" && command.boxHeight
          ? command.y + Math.max(0, Math.round((command.boxHeight - totalHeight) / 2))
          : command.y;
        lines.forEach((line, index) => {
          const lineWidth = measureBitmapText(line, font.height);
          const alignedX = command.align === "center"
            ? command.x - lineWidth / 2
            : command.align === "right"
              ? command.x - lineWidth
              : command.x;
          drawBitmapText(context, line, alignedX, top + font.height + index * lineStep, font.height);
        });
      }
    }

    return { ok: true, width, height, commandCount: commands.length, firstCommand, message: "rendered" };
  } catch (error) {
    return {
      ok: false,
      width,
      height,
      commandCount: commands.length,
      firstCommand,
      message: error instanceof Error ? error.message : String(error)
    };
  } finally {
    context.restore();
  }
}
