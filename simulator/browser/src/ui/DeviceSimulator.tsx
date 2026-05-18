import { useEffect, useMemo, useRef, useState } from "react";
import type { DrawCommand, LogicalKey, TargetDefinition } from "../types";
import { loadX4Layout } from "../target/layout";
import { renderCommands, type RenderResult } from "../rendering/canvasRenderer";

interface Props {
  target: TargetDefinition;
  commands: DrawCommand[];
  onButtonDown: (key: LogicalKey) => void;
  onButtonUp: (key: LogicalKey) => void;
}

export function DeviceSimulator({ target, commands, onButtonDown, onButtonUp }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [renderResult, setRenderResult] = useState<RenderResult | null>(null);
  const [cssSize, setCssSize] = useState({ width: 0, height: 0 });
  const layout = useMemo(() => loadX4Layout(target), [target]);
  const display = layout.elements.find((element) => element.kind === "display");

  useEffect(() => {
    if (canvasRef.current) setRenderResult(renderCommands(canvasRef.current, commands, target));
  }, [commands, target]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const updateCssSize = () => setCssSize({ width: canvas.clientWidth, height: canvas.clientHeight });
    updateCssSize();

    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(updateCssSize);
    observer?.observe(canvas);
    window.addEventListener("resize", updateCssSize);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", updateCssSize);
    };
  }, [display]);

  return (
    <section className="device-panel" aria-label="XTEINK X4 device simulator">
      <div className="device-note">{layout.note}</div>
      <div className="device-shell" style={{ aspectRatio: `${layout.canvas.width} / ${layout.canvas.height}` }}>
        <div className="device-body" />
        {display && (
          <>
            <canvas
              ref={canvasRef}
              className="device-display"
              aria-label="X4 display"
              data-render-ok={renderResult?.ok ? "true" : "false"}
              data-command-count={renderResult?.commandCount ?? 0}
              data-first-command={renderResult?.firstCommand ?? "pending"}
              style={{
                left: `${(display.x / layout.canvas.width) * 100}%`,
                top: `${(display.y / layout.canvas.height) * 100}%`,
                width: `${(display.width / layout.canvas.width) * 100}%`,
                height: `${(display.height / layout.canvas.height) * 100}%`
              }}
            />
            {renderResult && !renderResult.ok && (
              <div
                className="device-display-error"
                role="alert"
                style={{
                  left: `${(display.x / layout.canvas.width) * 100}%`,
                  top: `${(display.y / layout.canvas.height) * 100}%`,
                  width: `${(display.width / layout.canvas.width) * 100}%`,
                  height: `${(display.height / layout.canvas.height) * 100}%`
                }}
              >
                Display render failed: {renderResult.message}
              </div>
            )}
          </>
        )}
        {layout.elements.filter((element) => element.kind === "button").map((button) => (
          <button
            key={button.id}
            className="device-button"
            style={{
              left: `${(button.x / layout.canvas.width) * 100}%`,
              top: `${(button.y / layout.canvas.height) * 100}%`,
              width: `${(button.width / layout.canvas.width) * 100}%`,
              height: `${(button.height / layout.canvas.height) * 100}%`
            }}
            onPointerDown={(event) => {
              event.currentTarget.setPointerCapture(event.pointerId);
              if (button.logical) onButtonDown(button.logical);
            }}
            onPointerUp={(event) => {
              event.currentTarget.releasePointerCapture(event.pointerId);
              if (button.logical) onButtonUp(button.logical);
            }}
            onPointerCancel={() => button.logical && onButtonUp(button.logical)}
          >
            {button.label}
          </button>
        ))}
      </div>
      <div className="display-diagnostics" aria-label="display diagnostics">
        {renderResult
          ? `Display ${renderResult.width}x${renderResult.height}: ${renderResult.commandCount} commands, first ${renderResult.firstCommand}, ${renderResult.message}; CSS ${cssSize.width}x${cssSize.height}`
          : "Display render pending"}
      </div>
    </section>
  );
}
