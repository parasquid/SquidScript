# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

Speculative ideas that are not currently actionable belong in `ICEBOX.md`.

## Current Track: Canonical Zephyr Firmware

Goal: keep Zephyr as the canonical firmware architecture while Rust remains
authoritative for compiler, SQBC tooling, and VM semantics.

## Runtime Services

- Decide service priority and target support for spec-recognized APIs that are
  not yet SQBC-backed: broader `httpServer.*` server APIs beyond app-owned
  `service.http.start("file-upload", ...)`, remaining `service.ble.*` runtime
  pieces, and remaining `file.*` APIs beyond the current pick/read/copy
  family. Add each API only as a real compiler/SQBC/VM/Zephyr slice with
  honest unsupported behavior until target support is implemented.
- Add a `app.uninstall(appId)` builtin mirroring the new `app.install` shape:
  `IrStatement::AppUninstall { app_id }` → `BUILTIN_APP_UNINSTALL` → FFI
  callback `app_uninstall_file` → Zephyr `sq_app_store_uninstall_app` →
  rm the app directory under `/sd/apps/<app_id>/` and clear the registry
  entry. Use case: an installer app replaces itself with a newer version
  without a full device reset, or a manager app removes a misbehaving
  child app. Reject with `-ENOENT` if the app is not installed; reject
  with `-EBUSY` if the app is currently `current_app` (the caller must
  `app.exit` first or `app.launch` a different app). No new firmware cap
  needed; reuses the existing `sq_app_store` mount point.

## Storage And Content

- Allow graceful BinBook page rendering degradation when the renderer supports a richer pixel format than the stored page uses. In particular, the X4 firmware renderer currently accepts GRAY2 packed pages but rejects GRAY1 packed pages as unsupported; convert or expand GRAY1 to the renderer-supported path so GRAY1 BinBooks can still display.
- Add a storage-backed BinBook reading history API so the reader can remember
  per-book page positions without spending fixed app-state slots or inventing
  app-local history tables.
- Investigate BinBook library metadata caching so the reader can show titles,
  authors, page counts, and other metadata for folders of BinBooks without
  opening every document during each library render.
- Add a target-native BinBook output profile for the XIAO ESP32-C3 +
  GDEQ0426T82 SSD1677 backend so generated pages stream in physical panel
  order without requiring a full rotation framebuffer.
- Promote the XIAO ESP32-C3 e-paper target's external SPI SD reader from
  metadata-only to mounted app/content storage after jumper wiring is
  confirmed. Define card-missing boot policy, retained diagnostics, app-store
  recovery behavior, content volume semantics, and shared install validation
  before advertising SD-backed `supportsApps`, `supportsFile`, `sdcard`, or
  file APIs.

## Display And Output

- Validate the XTEINK BinBook reader fast highlight refresh path interactively
  on hardware: move library/menu/chapter highlights repeatedly and confirm the
  SSD1677 fast partial path avoids full-refresh flashing and unacceptable
  ghosting.
- Add a grid-cursor example app (`examples/grid-cursor/`) that renders a 5×5
  grid of cells, each containing a distinct shape (filled rect, outlined rect,
  cross pattern, diagonal lines, text glyph). A cursor highlights one cell at a
  time by inverting its colors; UP/DOWN/LEFT/RIGHT moves the cursor. Uses
  `fast1bpp` refresh so each cursor move exercises the SSD1677 differential
  partial path. This isolates refresh correctness (ghost clearing, shape
  restoration after cursor passes, no mangling) in a minimal visible test case
  separate from the BinBook reader's complexity, and doubles as a user-facing
  example. The hardware test script includes a mid-run device reset and
  relaunch to verify the lifecycle reset (first post-reset refresh is full
  seed, no stale differential).
- Add a GRAY2-aware streaming display compositor for
  `service.display.clear/text/rect/line` on SSD1677 targets, preserving
  source-order composition without a full-screen 2bpp framebuffer.
- Add a generic PWM-capable LED-like device output model beyond
  `service.indicator`, so future target-described GPIO/PWM endpoints can expose
  smooth brightness control without board-specific app code.

## Input, Triggers, And Power

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
- Use the GPIO input configuration affordance to make ESP32-C3 Super Mini
  diagnostic scans less noisy without changing the confirmed GPIO9 BOOT binding
  from active-low pull-up behavior. Use Espruino's split between pin mode
  (`input_pullup`/`input_pulldown`) and watches (`edge: rising/falling/both`,
  `debounce`) as design inspiration, adapted to SquidScript's explicit
  device/input binding model instead of global auto-mode side effects.

## Developer Tooling

- Add a `device content-delete <name>` CLI command and protocol opcode
  (`SQ_OPCODE_CONTENT_DELETE`) so stale content library entries can be removed
  without a full device reflash. Today `content-put` adds files to
  `/SD:/books` but no CLI command can remove them; `storage-format` only clears
  app storage, not the content volume. This blocks test hygiene when stale
  BinBooks with wrong panel dimensions accumulate from prior test runs.
  Follow the `content-check` pattern: firmware handler unlinks the file at
  `SQ_VM_RUNTIME_CONTENT_BOOKS_DIR/<name>`, Rust codec encodes/decodes, CLI
  exposes `device content-delete <name> --port <port>`.
- Audit compiler, SQBC, simulator, examples, and docs for invariant violations
  that should become explicit diagnostics instead of silent ambiguity.
- Promote the XTEINK X4 serial/HTTP/BLE transfer regression scripts into the
  target-aware `squidc hardware test --target xteink-x4` inventory so transfer
  integrity checks are selectable, consistently reported, and harder to skip
  during hardware verification.
- Add transfer throughput regression reporting for XTEINK X4 serial, HTTP, and
  BLE uploads. Record bytes, elapsed time, and effective bytes/sec for each
  transport, keep thresholds advisory until enough hardware samples define
  stable budgets, and use the data to catch speed regressions separately from
  content-integrity failures.
- Extend `squidc hardware test --target esp32c3-super-mini` so the Super Mini
  regression target uses the same target-aware hardware-test architecture as
  the XIAO default dev target, with checks selected from target metadata and
  exclusions for capabilities that require unavailable hardware.
- Reduce repo-owned Python tooling by folding generators and serial helpers into
  `squidc` Rust subcommands while keeping Zephyr `west`/`twister` Python as an
  external firmware toolchain dependency. Context: Python remains unavoidable
  for the Zephyr build/test stack, but repo-owned scripts such as target/code
  generators, markdown generation, serial helpers, Python unit tests, and small
  inline shell-wrapper Python snippets can move to Rust over time so project
  tooling is easier to install, test, and keep consistent.

## Test Hygiene

- Restore the native protocol ztest suite (`scripts/zephyr-test-protocol.sh`)
  to green. 33 pre-existing failures on `native_sim/native/64` stem from
  `SQ_TARGET_INDICATOR_DEFAULT_HAS_GPIO=0` for the c3-super-mini native test
  target. The indicator tests
  (`test_indicator_pattern_state_machine_transitions`,
  `test_device_protocol_poll_advances_running_runtime_poll`) fail in isolation
  because `sq_vm_runtime_indicator_breathe` returns `-ENODEV` without an active
  indicator GPIO binding, and their failure cascades into `mount_test_fs`
  pollution for approximately 30 dependent storage/launch/lifecycle tests that
  run after them in the suite. Fix by providing a `native_sim` indicator GPIO
  overlay so the test target has `HAS_GPIO=1`, or by making the indicator tests
  skip breathe/write assertions when no binding is active. The X4 hardware
  target does not exhibit this because it has the indicator GPIO.

## ESP32-C3 RAM Hardening

Current ESP32-C3 RAM baseline:

- XTEINK X4 linker DRAM: 375,440 / 378,640 bytes (99.15%), 3,200 B headroom.
- C3 Super Mini linker DRAM: 239,232 bytes.
- XIAO ESP32-C3 linker DRAM: 261,008 / 260,988 bytes.
- Current target configuration: 4,864-byte protocol/main stack,
  24,576-byte VM worker stack, 4,096-byte display worker stack.
- Stack harness guardrails: fail if protocol/main unused stack drops below 768
  bytes or VM worker unused stack drops below 384 bytes.
- `device resources` reports `heap_largest_free_supported=1` and a real
  `heap_largest_free_bytes` via a bounded `k_heap_alloc` binary-search probe,
  plus display-worker stack high-water (IDs 53-55). See
  `docs/specs/2026-06-20-x4-ram-reduction-design.md` for the full design.
- Static buffer attribution (corrected classifier,
  `scripts/zephyr-static-buffer-report.sh`): platform 133,234 B,
  SquidScript 137,836 B, unknown 16,871 B. The four display-op arrays
  (72,224 B) dominate the SquidScript group.

RAM follow-up triggers:

- Compact the XTEINK X4 display-op representation (tagged union + out-of-band
  BinBook page + typed color + page ring) to free ~55 KiB static DRAM and clear
  the 48 KiB headroom target. Design at
  `docs/specs/2026-06-20-x4-ram-reduction-design.md`; Plan 1 telemetry and
  baseline are committed (`edd5ab8`); Plan 2 is the active reduction slice.
- Revisit ESP32-C3 RAM optimization after the display-op compaction lands:
  remeasure linker DRAM, protocol response size, stack high-water, and
  SquidScript-owned static buffers; then decide whether to shrink response
  buffers, cap metrics, stacks, or subsystem feature buffers based on evidence.
- Do not lower the 24,576-byte VM worker stack again without same-build
  input-button or equivalent logical-input fixture evidence proving the
  physical/input app path stays below the proposed budget. Before any future
  stack reduction, build with `--stack-usage` and run the `.su` parser against
  the target build dir; check both per-function `.su` size and real hardware
  high-water use because splitting helpers can increase live stack if a larger
  callee remains active under its caller.
- Keep heap fragmentation work evidence-driven. The bounded `k_heap_alloc`
  probe now reports a real largest-free-block value; future mitigation work
  should target a concrete allocation failure or subsystem-specific pool/slab
  redesign rather than adding speculative RAM counters.
- Keep Zephyr kernel stacks, system heap, network packet pools, Wi-Fi/BLE
  driver storage, and other platform symbols separate unless platform RAM
  policy is explicitly in scope.

RAM verification notes:

- Use `scripts/xteink-x4-measure-ram-workloads.sh` for the X4 same-build
  RAM-confidence path. The X4 workload harness records storage-format,
  grid-cursor, binbook-reader, system-resources, and Wi-Fi AP start/stop rows
  under `target/hardware-tests/x4-ram-workloads/summary.tsv`, including
  display-stack and heap largest-free-block columns.
- The XIAO RAM workload harness (`scripts/xiao-esp32c3-measure-ram-workloads.sh`)
  records storage-format, e-paper GRAY2, system-resource, and Wi-Fi AP start/stop
  rows under `target/hardware-tests/xiao-ram-workloads/summary.tsv`.
- Real ESP32-C3 Zephyr Wi-Fi scan/list coverage passes through the driver scan
  callback with bounded redacted AP rows. Future Wi-Fi scan RAM work should
  focus on result pagination/cursor behavior and broader service-state modeling,
  not the old unsupported scan path.
