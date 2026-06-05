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
- Investigate ESP32-C3 Wi-Fi AP start after station connect/disconnect. Current
  XIAO radio concurrency evidence shows station reaches disconnected status, but
  a later AP start can report `ap ip failed`; isolate whether Zephyr interface
  mode/IP state needs explicit reset after station use.
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
- Use the GPIO input configuration affordance to make ESP32-C3 Super Mini
  diagnostic scans less noisy without changing the confirmed GPIO9 BOOT binding
  from active-low pull-up behavior. Use Espruino's split between pin mode
  (`input_pullup`/`input_pulldown`) and watches (`edge: rising/falling/both`,
  `debounce`) as design inspiration, adapted to SquidScript's explicit
  device/input binding model instead of global auto-mode side effects.

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
