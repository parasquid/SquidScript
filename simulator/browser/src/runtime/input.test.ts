import { describe, expect, it } from "vitest";
import { ButtonArbiter } from "./input";

const config = {
  longPress: [{ logical: "POWER" as const, durationMs: 2000, action: "sleep" }],
  chords: [{ logical: ["POWER" as const, "DOWN" as const], windowMs: 120, action: "refresh-display" }]
};

describe("button arbiter", () => {
  it("fires short presses on release", () => {
    const arbiter = new ButtonArbiter(config);
    expect(arbiter.press("DOWN", 0)).toBeNull();
    expect(arbiter.release("DOWN")).toEqual({ type: "short", key: "DOWN" });
  });

  it("makes long press suppress short press", () => {
    const arbiter = new ButtonArbiter(config);
    arbiter.press("POWER", 0);
    expect(arbiter.tick(2100)).toEqual({ type: "long", key: "POWER", action: "sleep" });
    expect(arbiter.release("POWER")).toBeNull();
  });

  it("makes chords suppress component keys", () => {
    const arbiter = new ButtonArbiter(config);
    arbiter.press("POWER", 0);
    expect(arbiter.press("DOWN", 90)).toEqual({ type: "chord", keys: ["POWER", "DOWN"], action: "refresh-display" });
    expect(arbiter.release("POWER")).toBeNull();
    expect(arbiter.release("DOWN")).toBeNull();
  });
});

