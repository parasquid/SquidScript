import { useEffect, useMemo, useRef } from "react";
import type { DrawCommand, LogicalKey, TargetDefinition } from "../types";
import { loadX4Layout } from "../target/layout";
import { renderCommands } from "../rendering/canvasRenderer";

interface Props {
  target: TargetDefinition;
  commands: DrawCommand[];
  onButtonDown: (key: LogicalKey) => void;
  onButtonUp: (key: LogicalKey) => void;
}

export function DeviceSimulator({ target, commands, onButtonDown, onButtonUp }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const layout = useMemo(() => loadX4Layout(target), [target]);
  const display = layout.elements.find((element) => element.kind === "display");

  useEffect(() => {
    if (canvasRef.current) renderCommands(canvasRef.current, commands, target);
  }, [commands, target]);

  return (
    <section className="device-panel" aria-label="XTEINK X4 device simulator">
      <div className="device-note">{layout.note}</div>
      <div className="device-shell" style={{ aspectRatio: `${layout.canvas.width} / ${layout.canvas.height}` }}>
        <div className="device-body" />
        {display && (
          <canvas
            ref={canvasRef}
            className="device-display"
            aria-label="X4 display"
            style={{
              left: `${(display.x / layout.canvas.width) * 100}%`,
              top: `${(display.y / layout.canvas.height) * 100}%`,
              width: `${(display.width / layout.canvas.width) * 100}%`,
              height: `${(display.height / layout.canvas.height) * 100}%`
            }}
          />
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
    </section>
  );
}
