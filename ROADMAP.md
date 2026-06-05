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
- Complete BLE object-transfer runtime support after the metadata/handler
  payload slice: stream chunks to staging storage, expose typed progress/error
  payload fields, install completed `.sqbc` uploads through the shared app-store
  pipeline, and verify on ESP32-C3 hardware with skip behavior when host
  Bluetooth is unavailable.
- Investigate BLE re-advertising after host disconnect on ESP32-C3 Zephyr.
  Resolved: `ble_smoke.c` was calling `bt_le_adv_start` directly from the
  delayed restart work and the controller returned `-EALREADY` (handled
  silently as success), so no fresh advertisement went out. The fix forces a
  clean state transition by calling `bt_le_adv_stop` before `bt_le_adv_start`
  in the restart path, with a `BLE advertising stopped before restart` log
  line. Native ztests under `firmware/zephyr/tests/ble-smoke` exercise the
  state machine with function-pointer stubs on `native_sim`, and
  `scripts/zephyr-test-ble-reconnect.sh` drives the real host Bluetooth
  controller on the XIAO ESP32-C3 to flash, connect, disconnect, wait out
  the grace window, and rescan for a fresh advertisement (verified with
  `58:8C:81:AC:52:5A XIAO ESP32-C3 ePaper 4.26 + SD` in
  `target/hardware-tests/ble-reconnect/`). The radio concurrency script's
  `--require-ble-reconnect` flag remains a secondary proof point for the
  same path under Wi-Fi pressure.
- Add external Wi-Fi AP client association and DHCP lease proof through
  Zephyr-native subsystems.
- Investigate ESP32-C3 Wi-Fi AP start after station connect/disconnect.
  Investigated: a regression test now drives a single runtime session through
  AP start, AP stop, station connect, station disconnect, and a second AP
  start (verified end-to-end on the XIAO ESP32-C3 via
  `scripts/zephyr-test-ap-after-station.sh`). The hypothesis that
  `runtime_wifi_configure_ap_ipv4` would fail with "ap ip failed" on the
  second AP start is not reproducible on the current firmware; the test
  passes and `grep -rln "ap ip failed"` over `target/hardware-tests/`
  returns no historical evidence either. Keep the test as a regression
  guard and reopen this entry if a real reproduction surfaces.
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

## Test And Documentation Integrity

Hardware-test and metadata hygiene follow-ups.

- Add a "Concepts" or "Glossary" section near the top of
  `docs/language_spec.md` consolidating the arming model. Define each
  "armed-*" phrase (`armed trigger registration`, `armed app`, `armed
  timer`, `armed stack`, `armed-app metadata`, etc.) as an alias for the
  canonical term, state the declare → arm → fire → launch model in one
  paragraph, and cross-reference sections 28, 30, and 44. Tighten the
  incomplete sentence at `docs/runtime_limits.md:36`. Deferred from the
  2026-06-05 BLE object-transfer design spec.
- Fix the stale XIAO target metadata test. `targets/xiao-esp32c3-gdeq0426t82-sd
  .target.json` now carries a `devices."indicator.default"` entry (`type:
  "not-present"`), but `scripts/tests/test_zephyr_target_metadata.py:74` still
  asserts `assertNotIn("indicator.default", target["devices"])` and currently
  fails. Update the assertion to expect the `not-present` indicator entry (or
  assert on its `type`/`softwareControllable` fields) so the metadata contract
  is actually pinned.
- Make `zephyr-test-ble-reconnect.sh` honest about what it proves, or make it
  prove more. Its usage text and `docs/hardware_target_tests.md:637` claim it
  "confirms the initial BLE advertising log" and "waits for the firmware's
  restart-advertising work item," but the script only drives host-side
  `bluetoothctl`; it never reads serial logs and `BLE_ADVERTISING_LOG_TIMEOUT_
  SECONDS` is set but unused. Either read `device output` for the
  `BLE advertising stopped before restart` / `restarted after disconnect`
  log lines, or soften the docs to claim only host rediscovery.
- Harden the BLE rediscovery check against BlueZ cache false-passes. The final
  rescan at `scripts/zephyr-test-ble-reconnect.sh:189` accepts the device name
  or MAC anywhere in the scan dump, while the initial scan parser already
  filters `[DEL]` lines. Remove the device from the BlueZ cache before the
  rescan, require a fresh `[NEW]`/`[CHG]` event after scan start, and filter
  `[DEL]` echoes so the "fresh advertisement" guarantee actually holds.
- Strengthen or rename the `test_multiple_disconnects_only_one_restart_runs`
  ztest (`firmware/zephyr/tests/ble-smoke/src/main.c:146`). It calls
  `sq_ble_smoke_sm_handle_disconnect()` twice then manually invokes
  `sq_ble_smoke_sm_run_restart()` once, so it proves a single restart
  invocation behaves correctly, not that the real `k_work_schedule` delayed
  work coalesces/cancels to one execution. Drive the work queue (or assert on
  the pending-work state) to actually prove the coalescing the name claims.
- Reconnect the host Wi-Fi interface in `zephyr-test-ap-after-station.sh`
  cleanup. The `cleanup()` at line 64 only downs/deletes the temporary AP
  connection and leaves the host radio disassociated, whereas
  `zephyr-test-radio-concurrency.sh:92` reconnects the interface
  (`nmcli device connect "$HOST_WIFI_IFACE"`). Mirror that so the test does not
  leave the developer's machine off Wi-Fi.
- Resolve the runtime-limits "source of truth" ambiguity and add a drift guard.
  `docs/runtime_limits.md:5` calls `firmware/zephyr/runtime_limits.json` the
  build-time tuning source while `firmware/zephyr/src/vm_runtime.h:4` says the
  cap macros are the source of truth, and nothing regenerates
  `runtime_limits.h` from the JSON and diffs it. Add a test or build rule that
  runs `scripts/generate-runtime-limits-header.py` and fails on a mismatch with
  the committed header, and reconcile the wording so one document is clearly
  authoritative.

## Build-Time And Runtime Caps

- **Runtime-tunable cap overrides**: implement the design in
  `docs/runtime_limits.md` "Runtime-Tunable Overrides (Design)" section.
  Storage: new `/device/runtime.sqdc` file (parallel SQDC format to
  `/device/active.sqdc`); build-time `runtime_limits.json` stays the
  maximum; runtime active count is the override. Boot applies on
  `sq_vm_runtime_init` from a new `sq_vm_runtime_load_runtime_caps`
  call. Registration gates (`count < SQ_VM_RUNTIME_*_MAX`) change to
  `count < runtime->active_*_max`. Wire surface: new
  `SQ_OPCODE_RUNTIME_CAP_SET/GET` ops, `runtime.active_caps` resource
  metric, CLI `squidc device runtime-cap get/set/clear`. Validation:
  reject out-of-range, reject values that would orphan active entries
  (the host stops the foreground app first). TDD: failing ztest for
  boot-apply, out-of-range, and orphan-rejection paths before
  production code. Open questions: armed-app sleep state depth, CLI
  side-by-side display, opt-in vs always-on reporting, app-facing
  `system.info()` exposure — see `docs/runtime_limits.md` "Open
  Questions."

## ESP32-C3 RAM Hardening

Current ESP32-C3 RAM baseline:

- Latest observed linker DRAM from
  `cargo run -p squidc -- target build --target esp32c3-super-mini`: 239,232
  bytes.
- Current target configuration: 4,864-byte protocol/main stack and
  16,640-byte VM worker stack.
- Stack harness guardrails: fail if protocol/main unused stack drops below 768
  bytes or VM worker unused stack drops below 384 bytes.
- Current `device resources` reports allocation high-water data and
  `heap_largest_free_supported` / `heap_largest_free_bytes`; ESP32-C3 currently
  reports `0/0` for largest-free-block support because the public Zephyr heap
  stats available in this build do not expose a safe non-mutating largest free
  block query.

RAM follow-up triggers:

- Continue SquidScript-owned static DRAM reductions only when new evidence
  identifies a larger target than the current measured groups: 123,310 bytes
  platform-owned, 31,616 bytes SquidScript-owned, and 10,729 bytes unknown.
  Current SquidScript-owned buffers are guarded by tests or protocol bounds:
  `runtime.4` is 11,920 bytes, the protocol response buffer is 1,088 bytes,
  and app-store/session/storage scratch buffers are explicitly capped.
- Do not lower the 16,640-byte VM worker stack again without same-build
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
