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
- Verify BLE install->launch end-to-end on hardware (DoD #6). Depends on the
  in-session launch fault being resolved first (see the "Known issue" in
  `docs/firmware_app_load_install_notes.md`). On the XIAO ESP32-C3, launch the
  `ble-install` receiver (it runs `service.ble.start` in `app.start`), push
  `target/hardware-tests/lazy-sqbc-stress/lazy-main-stress-8192.sqbc` over BLE
  via `tools/ots-push`, and confirm it installs byte-exact AND the installed app
  launches in-session with no `-5` (expect `lazy start 1` in `device output`).
  Confirm advertising comes up only after launch and stops on `service.ble.stop`
  / app exit. Do not use the synthetic `large.sqbc` / `oversized.sqbc` fixtures
  (install-proof only, not launchable). The imperative `service.ble.start/stop`
  redesign that this verifies has shipped; this entry is the remaining hardware
  proof.
- Clean up post-GATT-pivot OTS staleness in docs and roadmap. The GATT-only
  pivot removed `ble_ots.c` / OTS / L2CAP CoC but left stale references. Remove
  the dead "BLE Object Transfer Service (OTS) Initialization" section in
  `docs/hardware_target_tests.md` (`sq_ble_ots_init()`, `tests/ble-ots-init`,
  `scripts/zephyr-test-ble-ots-init.sh`). Remove the OTS-era items still in this
  ROADMAP (the OTS client-pull role, L2CAP CoC availability probe, OACP
  Calculate Checksum, and raise-`BT_MAX_CONN`-for-a-second-OTS-client items).
  Rename the `ble-ots-*` test dirs off the misleading `ots` prefix (they test
  the transport-neutral core now). Decide `tools/ots-push`'s fate (rename — it
  pushes over the custom GATT service, not OTS). Verify remaining BLE doc
  statements (`language_spec`, `runtime_limits`) match the as-built GATT-only +
  imperative code.
- Add a BLE OTS client role so SquidScript apps can pull (not just receive)
  objects from a paired peer. The Zephyr OTS module already exposes
  `bt_ots_client_*` helpers in `include/zephyr/bluetooth/services/ots.h`;
  wrap them in a `ble_ots_client.c` companion to `ble_ots.c` with an
  `service.ble.pull` (or similar) app-facing API that returns a file ref
  to the downloaded object. Use case: a "config sync" app fetches a JSON
  config from a paired phone; a "log pull" app drains a remote log file
  over OTS. Single-session policy applies symmetrically: the device
  either serves or pulls, not both at once. Likely a 2-3 slice
  follow-up after the current object-transfer work is merged.
- Verify L2CAP CoC availability across host platforms. The
  `tools/ots-push/` driver uses `bleak`'s L2CAP CoC support; CoC
  availability varies by platform (Linux BlueZ 5.x supports it, macOS
  Core Bluetooth has limited support, Windows varies). The slice 10
  skip pattern already handles "CoC unsupported" cleanly, but a CI
  matrix that actually probes each platform's CoC capability and reports
  it in the skip message (instead of a generic "unsupported" string)
  would make the CI signal actionable. Add a `tools/ots-push/probe.py`
  that prints the platform, bleak version, and a one-line CoC
  capability verdict; invoke it from the skip path.
- Add `OACP Calculate Checksum` support. The spec explicitly chose not
  to enable `CONFIG_BT_OTS_OACP_CHECKSUM_SUPPORT` because the
  firmware's `sq_app_store_install_from_file_ref` validates the SQBC
  magic on its own. If a future app needs the Zephyr OTS-level CRC32
  (e.g., to deduplicate uploads before staging), enabling this would
  add ~1 KiB of code for the `crc32_ieee` helpers
  (`<zephyr/sys/crc.h>`) and a real `obj_cal_checksum` callback impl
  that computes the CRC32 over the staging file range. The current
  stub returns `-ENOTSUP`; a real implementation would open the
  staging file, seek to the requested offset, and accumulate
  `crc32_ieee_update` over the range. Add a small ztest
  (`ble-ots-checksum`) that pre-stages a known-byte file and asserts
  the callback returns the expected CRC32.
- Raise `CONFIG_BT_MAX_CONN` from 1 to 2 to allow a second OTS client
  to connect while a transfer is in progress. The current
  single-connection cap means a second BT connection mid-transfer is
  rejected by Zephyr's BT stack before OACP; raising to 2 lets a second
  peer queue up while the first is mid-transfer (still rejected at
  the app level by the single-session policy, but with a clean GATT
  disconnect instead of a link-layer rejection). Cost: ~8 KiB
  additional RAM for the second connection's GATT/ATT buffers, plus
  the per-connection OTS context (~640 bytes per the slice 7 RAM
  budget). Verify on XIAO via `scripts/zephyr-ram-audit.sh` that the
  new `dram0_0_seg` stays under the 65% profile threshold.
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
- Stop a trigger from self-relaunching the app that already owns it. When a
  trigger fires and its target app is already `current_app`, the `due_app` path
  (`app_lifecycle.c:409-420`) currently does `push_return(current_app)` + start,
  so the foreground app gets stacked onto its own return stack and its handler
  re-runs from a reset context. Repeated firings accumulate duplicate frames
  (bounded by `SQ_VM_RUNTIME_RETURN_STACK_MAX`, then `push_return` errors) and
  force redundant work — e.g. a long-press-summoned e-ink sleep/wallpaper app
  would do a full e-ink refresh on every repeat press during its pre-sleep
  window. Fix: detect `due_app == current_app` and do NOT relaunch or push the
  return stack. Chosen behavior — deliver a distinct re-trigger event (working
  name `app.retrigger`, or `<event>.again`) to the already-current app, with a
  silent no-op default when the app declares no handler for it. The no-op
  default makes the common case ("the screen is already up, ignore the repeat")
  require zero app code and avoids redundant e-ink refreshes; apps that want to
  react (reset a countdown, sleep immediately) opt in via a handler, and because
  the re-trigger event is distinct from the launch event they can act without
  re-running launch-time work. Out of scope: re-entrant delivery into an app
  that is actively `RUNNING` (still gated at `IDLE` today) — that is the
  separate VM-semantics change tracked by the queued event-delivery item above.
  Spec the new event name, target policy, and the no-op default in
  `docs/language_spec.md`, and add a ztest for the `due_app == current_app`
  case (no return-stack growth, correct event delivered).
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

## Developer Tooling

- Add a SquidScript source formatter. Recommended shape: a top-level
  `squidc fmt` command (mirrors `cargo fmt` / `gofmt`; formatting is a
  source-file operation that does not belong under the existing `app` / `device`
  / `target` / `protocol` subcommand groups in
  `compiler/rust/crates/squidc-cli/src/main.rs`). Accept one or more `.squid`
  paths (and/or globs/directories), default to rewriting files in place, and
  support `--check` (exit non-zero with a diff on unformatted files, for CI /
  pre-commit) and `--stdin`/stdout for editor integration. Build it on the
  existing parser so formatting is lossless and idempotent (`fmt` of formatted
  output is a no-op); decide whether comments and blank-line grouping are
  preserved. Open questions: canonical style rules (indentation width, brace
  style, `app.triggers` / `device { ... }` block layout, trailing commas),
  whether to format embedded literals, and whether a `--check` mode should be
  wired into an existing test/CI script.
- Add a `squidc`-native BLE app-upload command so uploading a `.sqbc` over BLE
  no longer needs the separate Python `ots-push` tool. Drive the custom GATT
  app-transfer service (`firmware/zephyr/src/ble_app_transfer.c`) — control
  `BEGIN`/`ABORT`, chunked data writes, status notify — from Rust via a
  cross-platform BLE crate (e.g. `btleplug`, Linux/macOS/Windows). Likely shape:
  `squidc app push <device> <file> [--profile <id>]`. Retire `tools/ots-push/`
  once at parity; keep the Web Bluetooth uploader (`tools/ble-web-uploader/`) as
  the no-app/browser path. Independent of the BLE control-write MTU fix — the
  on-wire protocol work applies to any client.

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
- Increase the device debug/error entry ring to 12. Update
  `firmware/zephyr/runtime_limits.json`, regenerate
  `firmware/zephyr/src/runtime_limits.h`, and reconcile `vm_runtime.h`,
  `docs/runtime_limits.md`, `docs/firmware_app_load_install_notes.md`, and the
  protocol ztests so the build-time source, generated header, C fallback macros,
  docs, and ring-overflow behavior all agree on
  `SQ_VM_RUNTIME_DEVICE_ERROR_MAX == 12`.

## ESP32-C3 RAM Hardening

Current ESP32-C3 RAM baseline:

- Latest observed linker DRAM from
  `cargo run -p squidc -- target build --target esp32c3-super-mini`: 239,232
  bytes.
- XIAO ESP32-C3 e-paper dev target after the BLE object-transfer
  work (slices 2-10): 243,832 bytes (`dram0_0_seg` from
  `scripts/zephyr-ram-audit.sh`), a delta of +4,088 bytes vs the
  Super Mini baseline. The bulk of the increase is the Zephyr
  `bt_ots` module itself (L2CAP CoC, GATT dynamic DB, SMP, EXPERIMENTAL
  auto-selects), not the SquidScript-owned additions. The
  SquidScript-owned delta is small: the 2-entry trigger table
  (~1,280 bytes), the pending event slot, the in-flight session
  struct, and the OTS callback dispatch. The `ram_static_top_bytes`
  went from 131,084 to 131,924 (+840 bytes for SquidScript-owned
  symbols); the rest is Zephyr OTS internals. If RAM becomes tight,
  `CONFIG_BT_OTS_OACP_WRITE_SUPPORT` and the other OTS Kconfig
  features can be selectively disabled to claw back a few hundred
  bytes, and `CONFIG_BT_MAX_CONN=1` (already set) keeps the
  per-connection buffers at the minimum.
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
