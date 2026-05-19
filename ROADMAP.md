# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

## Current Track: ESP32-C3 Persistent Reference Runtime

Goal: turn the ESP32-C3 Super Mini reference firmware from a RAM-only
development harness into a persistent SquidScript app platform prototype.

### 1. Commit Current Cleanup

- Commit the docs/script/test rename work.
- Keep the hardware target test naming and aggregate hardware test script.
- Keep `kind` removed from app manifests.
- Keep hardware target tests documented as requiring escalated execution
  outside the Codex sandbox.
- Verify before commit:
  - `cargo test`
  - `(cd firmware/squid-firmware && cargo test --target x86_64-unknown-linux-gnu)`
  - `PYTHONPATH=scripts python3 -m unittest discover scripts/tests`
  - `npm test`
  - `npm run build`
  - `bash -n` on shell scripts
  - `git diff --check`

### 2. Plan Persistent App Registry

- Define the minimal persistent storage model for ESP32-C3 Super Mini installed
  SQBC apps.
- Decide where app bytes and metadata live in flash/storage.
- Preserve the developer protocol surface: `INSTALL.APP`, `RUN.APP`,
  `RUN.EVENT`, and `APP.LIST`.
- Define corrupt/missing app behavior.
- Decide how the current RAM registry becomes an index/cache over persistent
  entries.

### 3. Implement Persistent App Registry

- Persist installed SQBC bytes plus app metadata: app id, length, hash, and
  validity marker.
- Keep host-testable registry/storage behavior separate from ESP32-C3 hardware
  code.
- Make `APP.LIST` report persistent entries.
- Ensure reset or power loss no longer clears installed apps.

### 4. Boot Root `main.sqbc`

- On firmware startup, load installed app `main` from persistent storage.
- Dispatch `event.on("app.start")` automatically.
- If `main` exits, restart `main`.
- If `main` is missing or invalid, stay in dev shell mode and report a clear
  serial error.
- Keep `squidc run examples/foo.squid` compiling/uploading as app `main`, then
  starting it.

### 5. Persist App State

- Make `state.load` and `state.save` survive reset for primitive state values.
- Store state per app id.
- Preserve current `STATE.GET` and `STATE.IMPORT` developer protocol behavior.
- Treat corrupt state as recoverable: ignore bad state, fall back to defaults,
  and report an error.

### 6. Expand Hardware Regression Tests

- Add a persistence hardware target test that installs `main`, resets/reboots
  firmware, and verifies `main` starts without re-upload.
- Add a state persistence hardware target test that mutates/saves state,
  resets/reboots, and verifies `state.load` restores the value.
- Keep blinky as the final visible hardware target test.
