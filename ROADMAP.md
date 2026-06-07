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
  not yet SQBC-backed: `httpServer.*`, remaining `service.ble.*` runtime
  pieces, and remaining `file.*` APIs beyond the current file pick/read family.
  Add each API only as a real compiler/SQBC/VM/Zephyr slice with honest
  unsupported behavior until target support is implemented.
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
- Treat Wi-Fi scan/connect/AP lifecycle as a future explicit service-state
  machine item when Wi-Fi work is in scope. Keep the current nonblocking
  operation/result/cursor API as the baseline.
- Defer `binbook.*` compiler/FFI/firmware work until the e-paper display is
  available and the BinBook spec has settled enough to avoid optimizing around
  rough draft behavior.

## Storage And Content

- Promote the XIAO ESP32-C3 e-paper target's external SPI SD reader from
  metadata-only to mounted app/content storage after jumper wiring is
  confirmed. Define card-missing boot policy, retained diagnostics, app-store
  recovery behavior, content volume semantics, and shared install validation
  before advertising SD-backed `supportsApps`, `supportsFile`, `sdcard`, or
  file APIs.

## Display And Output

- Implement an SSD1677/GDEQ0426T82 SquidScript display backend when the display
  breakout is available. Evaluate `ssd1677-driver` first, compare `ssd1677` if
  needed, and adopt a dependency only after proving bounded strip/window
  writes, caller-owned buffers, and bounded/nonblocking BUSY handling for
  constrained firmware RAM.
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

- Reduce repo-owned Python tooling by folding generators and serial helpers into
  `squidc` Rust subcommands while keeping Zephyr `west`/`twister` Python as an
  external firmware toolchain dependency. Context: Python remains unavoidable
  for the Zephyr build/test stack, but repo-owned scripts such as target/code
  generators, markdown generation, serial helpers, Python unit tests, and small
  inline shell-wrapper Python snippets can move to Rust over time so project
  tooling is easier to install, test, and keep consistent.

## ESP32-C3 RAM Hardening

Current ESP32-C3 RAM baseline:

- Latest observed linker DRAM from
  `cargo run -p squidc -- target build --target esp32c3-super-mini`: 239,232
  bytes.
- Latest observed linker DRAM from
  `cargo run -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd`:
  256,992 bytes. The custom BLE file-transfer path keeps SquidScript-owned state
  bounded: a small profile table, one pending event slot, and one in-flight
  staging session. If RAM becomes tight, audit the target Bluetooth feature set
  and connection/buffer counts before increasing firmware-owned static buffers.
- Current target configuration: 4,864-byte protocol/main stack and
  24,576-byte VM worker stack.
- Stack harness guardrails: fail if protocol/main unused stack drops below 768
  bytes or VM worker unused stack drops below 384 bytes.
- Current `device resources` reports allocation high-water data and
  `heap_largest_free_supported` / `heap_largest_free_bytes`; ESP32-C3 currently
  reports `0/0` for largest-free-block support because the public Zephyr heap
  stats available in this build do not expose a safe non-mutating largest free
  block query.

RAM follow-up triggers:

- Revisit ESP32-C3 RAM optimization after runtime caps and diagnostics settle:
  remeasure linker DRAM, protocol response size, stack high-water, and
  SquidScript-owned static buffers; then decide whether to shrink response
  buffers, cap metrics, stacks, or subsystem feature buffers based on evidence.
- Continue SquidScript-owned static DRAM reductions only when new evidence
  identifies a larger target than the current measured groups: 123,310 bytes
  platform-owned, 31,616 bytes SquidScript-owned, and 10,729 bytes unknown.
  Current SquidScript-owned buffers are guarded by tests or protocol bounds:
  `runtime.4` is 11,920 bytes, the protocol response buffer is 1,088 bytes,
  and app-store/session/storage scratch buffers are explicitly capped.
- Do not lower the 24,576-byte VM worker stack again without same-build
  input-button or equivalent logical-input fixture evidence proving the
  physical/input app path stays below the proposed budget. Before any future
  stack reduction, build with `SQUID_ZEPHYR_STACK_USAGE=1` and run
  `scripts/c3-supermini-stack-usage-report.sh`; check both per-function `.su`
  size and real hardware high-water use because splitting helpers can increase
  live stack if a larger callee remains active under its caller.
- Keep heap fragmentation work evidence-driven. This Zephyr build does not
  expose a safe non-mutating largest-free-block query, so current diagnostics
  report heap allocation high-water plus explicit `heap_largest_free_supported`
  / `heap_largest_free_bytes` fields. Future mitigation work should target a
  concrete allocation failure, target-safe heap probe, or subsystem-specific
  pool/slab redesign rather than adding speculative RAM counters.
- Keep Zephyr kernel stacks, system heap, network packet pools, Wi-Fi/BLE
  driver storage, and other platform symbols separate unless platform RAM
  policy is explicitly in scope.

RAM verification notes:

- Use `scripts/c3-supermini-test-hardware-non-scan.sh` for the same-build
  RAM-confidence path. `--skip-physical-input` is allowed only for unattended
  stack/RAM coverage and does not validate the physical GPIO9 press row.
- Logical input dispatch stack coverage can use host-injected
  `device key SELECT` events. Physical GPIO9 tests validate the electrical and
  binding path that queues the same logical event.
- Real ESP32-C3 Zephyr Wi-Fi scan/list coverage passes through the driver scan
  callback with bounded redacted AP rows. Future Wi-Fi scan RAM work should
  focus on result pagination/cursor behavior and broader service-state modeling,
  not the old unsupported scan path.
