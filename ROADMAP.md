# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

## Current Track: ESP32-C3 Persistent Reference Runtime

Goal: finish turning the ESP32-C3 Super Mini reference firmware into a
persistent SquidScript app platform prototype.

### 1. Boot Root `main.sqbc`

- On firmware startup, load installed app `main` from persistent storage.
- Dispatch `event.on("app.start")` automatically.
- If `main` exits, restart `main`.
- If `main` is missing or invalid, stay in dev shell mode and report a clear
  serial error.
- Keep `squidc run examples/foo.squid` as a volatile quick-check path that
  uploads with `RUN.TEMP` without overwriting persistent `main`.

### 2. Persist App State

- Make `state.load` and `state.save` survive reset for primitive state values.
- Store state per app id.
- Preserve current `STATE.GET` and `STATE.IMPORT` developer protocol behavior.
- Treat corrupt state as recoverable: ignore bad state, fall back to defaults,
  and report an error.

### 3. Expand Hardware Regression Tests

- Add a state persistence hardware target test that mutates/saves state,
  resets/reboots, and verifies `state.load` restores the value.
- Keep blinky as the final visible hardware target test.

### 4. Finish SQBC v3 Chunk Loader

- Keep `squidc run`/`RUN.TEMP` RAM-backed before 1.0 so rapid iteration does
  not write flash.
- Replace installed-app execution's contiguous SQBC RAM buffer with
  metadata-first loading from LittleFS.
- Load handler/function/screen chunks on demand through the chunk cache.
- Use `@preload` handler flags as cache priority hints, not correctness
  guarantees.
- Use `squidc device resources` and firmware ELF section sizes to compare RAM
  before and after loader changes.
- Defer release-profile trimming or disabling temp execution until after 1.0
  planning.
