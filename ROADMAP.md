# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

Speculative ideas that are not currently actionable belong in `ICEBOX.md`.

## Current Track: Native Firmware

- Add native_sim dummy hardware mocks (display, SPI SD) so protocol tests
  can assert on hardware interactions without real devices. Indicator GPIO
  mock is done (`runtime_indicator_breathe` returns 0 for ENODEV); display
  and SPI SD stubs remain so tests can verify call sequences, pin
  assignments, and state transitions.
- Refactor protocol tests to use a proper native_sim target with controlled
  mocked hardware bindings so tests do not leak X4-specific defaults.

Goal: keep the native Rust firmware, compiler, SQBC tooling, and VM semantics
aligned as the sole firmware architecture.

## Runtime Services

- Decide service priority and target support for spec-recognized APIs that are
  not yet SQBC-backed: broader `httpServer.*` server APIs, remaining
  non-upload `service.ble.*` runtime pieces, and remaining `file.*` APIs beyond
  the current pick/read/copy
  family. Add each API only as a real compiler/SQBC/VM/native-firmware slice with
  honest unsupported behavior until target support is implemented.
- Add a `app.uninstall(appId)` builtin mirroring the new `app.install` shape:
  `IrStatement::AppUninstall { app_id }` → `BUILTIN_APP_UNINSTALL` → native
  app-store removal, then clear the registry
  entry. Use case: an installer app replaces itself with a newer version
  without a full device reset, or a manager app removes a misbehaving
  child app. Reject with `-ENOENT` if the app is not installed; reject
  with `-EBUSY` if the app is currently `current_app` (the caller must
  `app.exit` first or `app.launch` a different app). No new firmware cap
  needed; reuses the existing `sq_app_store` mount point.

## Storage And Content

- Support UTF-8 filenames consistently across internal LittleFS and SD FAT
  storage, including transport validation, enumeration, lookup, and display
  fallback for unavailable glyphs.
- **Regenerate tracked BinBook fixtures for the current format.** The BinBook
  format and Rust compiler have changed, and the tracked `.binbook` files plus
  `scripts/generate-test-binbook.py` now fail validation through the current
  `binbook-core` / `binbook inspect --validate --strict`. Replace the
  hand-packed generator with the current BinBook compiler, inventory and
  regenerate every tracked `.binbook`, then update affected tests, scripts,
  and documentation. Verify each regenerated fixture with strict host
  validation and rerun its owning SquidScript test or hardware workflow. Do
  not weaken firmware content checks to accept stale files.
- **Unify serial uploads under `device upload`.** `squidc device upload`
  currently supports HTTP and BLE, while serial file transfer remains exposed
  separately through `device content-put`. Add `--transport serial` with the
  serial port as its destination and give it the same filename validation,
  progress reporting, completion result, and error semantics as the other
  transports. Route serial through the shared upload staging and publication
  lifecycle where practical; review `content-put` and its storage path for
  duplicated transfer or storage behavior and consolidate it rather than
  maintaining parallel implementations. Preserve retry or resume behavior
  where the serial protocol supports it, update CLI help, docs, scripts, and
  tests, and verify byte-exact upload plus published-content behavior on
  attached hardware. Start from the existing serial transfer protocol; do not
  add another protocol unless it cannot satisfy the unified contract.
- Add app-facing file management APIs: rename, move, copy, delete, and
  related result records for firmware-owned file references and logical
  libraries, so upload handlers can organize files beyond the current
  content-specific `file.copy` path.
- Add a storage-backed BinBook reading history API so the reader can remember
  per-book page positions without spending fixed app-state slots or inventing
  app-local history tables.
- Investigate BinBook library metadata caching so the reader can show titles,
  authors, page counts, and other metadata for folders of BinBooks without
  opening every document during each library render.

## Display And Output

- **Investigate hardware decompression for binbook page turns.** Current
  PackBits/LZ4 decompression is CPU-bound on ESP32-C3 (~0.5s for a BW-only
  plane, ~2.5s for full grayscale). The ESP32-C3 has no DMA-based
  decompressor, but investigate: (a) whether the SPI SD reader can DMA
  directly into the display controller's RAM without a full CPU-side
  decompress pass, (b) whether the SSD1677's built-in LUT or command
  sequence can accept pre-formatted RAM writes that skip the framebuffer
  intermediate, (c) whether ESP32-C3's cache/alignment features improve
  LZ4 throughput beyond the current ~50MB/s software rate. Per-plane
  binbook format is now implemented; BW-only path decompresses only the
  BW plane (~0.5s). Further optimization requires hardware investigation.
- **Investigate binbook delta rendering.** Store XOR deltas between
  consecutive pages as separate compressed planes in the binbook format
  (plane slot 3). On page turn, decompress only the delta and XOR it into
  the current framebuffer — avoids decompressing a full page when the
  visual change is small (e.g., text scrolling, cursor movement). Requires:
  (a) delta plane encoding in the binbook writer, (b) delta decompression
  + XOR path in firmware, (c) keyframe interval strategy (full page every
  N deltas for random access), (d) measurement of delta size vs full page
  for typical page transitions. Per-plane binbook format is now implemented
  (plane slot 3 is reserved for deltas). Design intent
  documented in `docs/specs/2026-06-23-binbook-per-plane-blob-format.md`.
- Investigate dirty-region tracking for the composed display flush path: only
  re-render rows that changed since last flush. Biggest win for static content
  with small changes (e.g., cursor movement). Needs change detection between
  successive op sets — compare current ops with previous ops to identify which
  rows changed, then only stream and refresh those rows.
- Investigate custom LUT (lookup table) for SSD1677 partial refresh to reduce
  refresh time from ~506ms. Load a faster waveform via command 0x32 (105 bytes)
  with shorter TP timing values, and change 0x22 from 0xFC to 0xEC (add LUT_LOAD
  bit). Risk: increased ghosting or reduced contrast. Needs hardware
  experimentation with different waveform tables.
- Validate the XTEINK BinBook reader fast highlight refresh path interactively
  on hardware: move library/menu/chapter highlights repeatedly and confirm the
  SSD1677 fast partial path avoids full-refresh flashing and unacceptable
  ghosting.
- Add a GRAY2-aware streaming display compositor for
  `service.display.clear/text/rect/line` on SSD1677 targets, preserving
  source-order composition without a full-screen 2bpp framebuffer.
- Add a generic PWM-capable LED-like device output model beyond
  `service.indicator`, so future target-described GPIO/PWM endpoints can expose
  smooth brightness control without board-specific app code.
- **Investigate SSD1677 ghosting on composed (fast1bpp) path.** Phase 2
  differential cleanup is broken: the handler sees `display_dirty == false` and
  skips writing the previous frame to RED_RAM (`vm_runtime.c:545`). The composed
  path also has no cadence counter (unlike BinBook's every-5-partials full
  refresh). Ghosting accumulates unchecked. Fix phase 2 and/or add a composed
  cadence counter. Explore whether a black-to-white transition clears ghosting
  without a full GRAY2 waveform cycle.

## Input, Triggers, And Power

- Run deferred native X4 physical parity checks: remove/reinsert SD and verify
  missing-SD fallback plus mixed-volume precedence; exercise all six ADC keys
  and POWER short/long/double gestures; verify timerless sleep with physical
  POWER wake; capture live panel evidence for armed input-trigger redraw. Use
  the existing opt-in hardware runners and record observed evidence without
  changing firmware-owned event-only policy.
- Add a bounded queued event-delivery path so trigger events are not dropped
  when the VM is busy. Today the main loop (`main.c:148-171`) is the sole event
  dispatcher and only checks armed timers / input when the VM is `IDLE`
  (`device_protocol.c:1368-1370`, `vm_runtime_indicator_gpio.c:507`); the VM is
  single-job (`sq_vm_runtime_submit_work` returns `-EBUSY`, `vm_runtime.c:146`)
  with no pending-event buffer, so any timer or input event that arrives while a
  foreground app is running is silently dropped — a button press fully inside
  the run window is not even latched. Add a bounded, thread-safe pending-event
  queue (e.g. `k_msgq`) that input edges and timers enqueue and the poll loop
  drains when the VM returns to `IDLE`, with a documented overflow policy
  (drop-oldest vs drop-newest vs coalesce-by-event). Out of scope: delivering
  events into an already-running app (re-entrant in-app dispatch) is a separate
  VM-semantics change.
- Extend the `app.triggers` model beyond current timer metadata declarations to
  future logical button/input triggers while keeping `event.on(...)` as the
  handler for the activation event that fires later.
- Extend planned-sleep wake sources beyond the current ESP32-C3 timer-wake
  slice. Investigate safe GPIO wake for physical inputs without using
  BOOT/GPIO9 as the default wake source, and keep wake trigger metadata derived
  from installed app trigger declarations rather than persisted VM state.
- Design and implement richer logical input events for press and release
  phases, long press, double tap, and chords. Specify naming, target policy,
  precedence, debounce/timing windows, and whether recognized long/chord/double
  gestures suppress component short press/release events.
- Design and implement BLE HID input sources as system-owned pairing with
  declarative foreground app demand. Firmware should map HID reports from page
  turners, keyboards, and similar controllers to logical key events, gate
  scanning/connection on active input demand for energy, and include a
  `ble-hid-tester` example that prints received logical keys to serial and
  display. Keep app-owned BLE file-transfer lifecycle separate from background
  input services.
- Add a way for app or device input configuration to set long-press duration
  thresholds, likely through `device { input ... }` binding metadata or a
  related input config block, while preserving target defaults and target-owned
  system actions such as long `POWER` sleep.
- Add a SquidScript GPIO input configuration affordance for raw hardware
  diagnostics and target-specific local inputs. It should let code or device
  binding metadata request input bias such as pull-up, pull-down, or floating
  where the target supports it, while keeping portable service APIs separate
  from board-specific GPIO names.
- Support non-GPIO input bindings in the device config language and firmware:
  matrix keyboards (row/column scanning with debounce and ghosting rules),
  ADC-ladder / resistor-network buttons (one analog pin, N voltage thresholds
  producing N logical keys), and I2C GPIO expanders (e.g., MCP23017). The
  target definition reference and target profile architecture docs already
  advertise `adc-ladder-button`, `matrix`, and `adc-button-ladder` as valid
  input types, but `runtime_device_config` only accepts `mode ==
  "gpio-button"` today (rejects others as "invalid binding"). This entry
  needs language spec phrasing, target JSON examples, device config validation
  acceptance, a reader per type, polling/debounce hooks in
  `sq_vm_runtime_poll_input_buttons`, and verified end-to-end on a target
  that exercises each path. The `SQ_VM_RUNTIME_INPUT_BUTTON_MAX` cap currently
  sizes the GPIO slot table; matrix/ADC/expander inputs may share the same
  cap or get separate per-type caps depending on storage shape. Fix the
  doc-code gap by either delivering the feature or downgrading the docs to
  say "GPIO-only" until readers land.
## Developer Tooling

- Audit compiler, SQBC, simulator, examples, and docs for invariant violations
  that should become explicit diagnostics instead of silent ambiguity.
- Add transfer throughput regression reporting for XTEINK X4 serial, HTTP, and
  BLE uploads. Record bytes, elapsed time, and effective bytes/sec for each
  transport, keep thresholds advisory until enough hardware samples define
  stable budgets, and use the data to catch speed regressions separately from
  content-integrity failures.
- Add authenticated network delivery for native X4 OTA images after serial OTA
  is complete. Reuse the inactive-slot writer, image validation, health
  confirmation, and rollback state machine; add signed-image policy, resumable
  transfer, interrupted-download cleanup, and recovery hardware gates rather
  than creating a separate firmware-update path.
- Reduce repo-owned Python tooling by folding generators and serial helpers into
  `squidc` Rust subcommands. Repo-owned scripts such as target/code generators,
  markdown generation, serial helpers, Python unit tests, and small inline
  shell-wrapper Python snippets can move to Rust over time so project tooling
  is easier to install, test, and keep consistent.
- **Unified constant definitions across C and Rust.** Identify constants
  duplicated across C and Rust codebases and establish a single source of truth
  to prevent drift. Options: generate C headers from Rust constants, use a
  shared definition file both languages consume, or maintain one canonical set
  with automated generation/validation of the other.
