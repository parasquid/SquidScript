import type { LogicalKey } from "../types";

export type ButtonOutcome =
  | { type: "short"; key: LogicalKey }
  | { type: "long"; key: LogicalKey; action: string }
  | { type: "chord"; keys: LogicalKey[]; action: string };

export interface ButtonTimingConfig {
  longPress: Array<{ logical: LogicalKey; durationMs: number; action: string }>;
  chords: Array<{ logical: LogicalKey[]; windowMs: number; action: string }>;
}

interface PressState {
  key: LogicalKey;
  startedAt: number;
  consumed: boolean;
}

export class ButtonArbiter {
  private pressed = new Map<LogicalKey, PressState>();

  constructor(private readonly config: ButtonTimingConfig) {}

  press(key: LogicalKey, now = performance.now()): ButtonOutcome | null {
    for (const state of this.pressed.values()) {
      const chord = this.config.chords.find((candidate) => candidate.logical.includes(state.key) && candidate.logical.includes(key));
      if (chord && now - state.startedAt <= chord.windowMs) {
        state.consumed = true;
        this.pressed.set(key, { key, startedAt: now, consumed: true });
        return { type: "chord", keys: chord.logical, action: chord.action };
      }
    }

    this.pressed.set(key, { key, startedAt: now, consumed: false });
    return null;
  }

  tick(now = performance.now()): ButtonOutcome | null {
    for (const state of this.pressed.values()) {
      if (state.consumed) continue;
      const longPress = this.config.longPress.find((candidate) => candidate.logical === state.key);
      if (longPress && now - state.startedAt >= longPress.durationMs) {
        state.consumed = true;
        return { type: "long", key: state.key, action: longPress.action };
      }
    }
    return null;
  }

  release(key: LogicalKey): ButtonOutcome | null {
    const state = this.pressed.get(key);
    this.pressed.delete(key);
    if (!state || state.consumed) return null;
    return { type: "short", key };
  }
}

