# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

## Current Track: Canonical Zephyr Firmware

Goal: keep Zephyr as the canonical firmware architecture while Rust remains
authoritative for compiler, SQBC tooling, and VM semantics.

## Runtime Services

- Decide service priority and target support for spec-recognized APIs that are
  not yet SQBC-backed: `httpServer.*`, `bleTransfer.*`, and remaining `file.*`
  APIs beyond the current file pick/read family. Add each API only as a real
  compiler/SQBC/VM/Zephyr slice with honest unsupported behavior until target
  support is implemented.
- Decide whether the ESP32-C3 Super Mini reference target should expose
  `bleTransfer.*`; if yes, implement and verify it through Zephyr BLE instead
  of relying on MCU radio metadata alone.
- Add external Wi-Fi AP client association and DHCP lease proof through
  Zephyr-native subsystems.
- Treat Wi-Fi scan/connect/AP lifecycle as a future explicit service-state
  machine item when Wi-Fi work is in scope. Keep the current nonblocking
  operation/result/cursor API as the baseline.
- Defer `binbook.*` compiler/FFI/firmware work until the e-paper display is
  available and the BinBook spec has settled enough to avoid optimizing around
  rough draft behavior.

## Display And Output

- Implement an SSD1677/GDEQ0426T82 SquidScript display backend when the display
  breakout is available. Evaluate `ssd1677-driver` first, compare `ssd1677` if
  needed, and adopt a dependency only after proving bounded strip/window
  writes, caller-owned buffers, and bounded/nonblocking BUSY handling for
  constrained firmware RAM.
- Add a generic PWM-capable LED-like device output model beyond
  `service.indicator`, so future target-described GPIO/PWM endpoints can expose
  smooth brightness control without board-specific app code.
- Support multiple `use` entries for one logical indicator when an app
  intentionally wants `service.indicator.write(...)` to drive more than one
  physical output.

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

- Latest observed linker DRAM from `scripts/c3-supermini-build.sh`: 196,048
  bytes.
- Current target configuration: 5,120-byte protocol/main stack and
  17,408-byte VM worker stack.
- Stack harness guardrails: fail if protocol/main unused stack drops below 768
  bytes or VM worker unused stack drops below 384 bytes.
- Current `device resources` reports allocation high-water data and
  `heap_largest_free_supported` / `heap_largest_free_bytes`; ESP32-C3 currently
  reports `0/0` for largest-free-block support because the public Zephyr heap
  stats available in this build do not expose a safe non-mutating largest free
  block query.

Active RAM work:

- Continue SquidScript-owned static DRAM reductions using the classified
  static-buffer report. Prioritize VM runtime storage, response/session
  buffers, logging, LittleFS pools, file caches, and any large unknown symbols.
- Before stack reductions, build with `SQUID_ZEPHYR_STACK_USAGE=1` and run
  `scripts/c3-supermini-stack-usage-report.sh` to review source-known
  cumulative call paths. Check both per-function `.su` size and real hardware
  high-water use because splitting helpers can increase live stack if a larger
  callee remains active under its caller.
- Do not lower the 17,408-byte VM worker stack again without same-build
  input-button or equivalent logical-input fixture evidence proving the
  physical/input app path stays below the proposed budget.
- Keep the 824-byte protocol response buffer until resources output is
  redesigned again, because it is sized to the current largest response.
- Treat `runtime.4` quota cuts as test-first changes: reduce VM records, record
  fields, dynamic string slots, trace/output/drawlog slots, or lifecycle/input
  arrays only when compiler/runtime fixtures and hardware apps show the smaller
  quota still covers current behavior.
- Add a safe largest-free-block heap probe or mitigation path. Candidate
  mitigations include target-safe probes, subsystem allocation-failure
  attribution, fixed arenas, caller-owned buffers, bounded scratch, slabs/pools
  for unavoidable dynamic allocations, and startup-owned long-lived allocations
  instead of mixed-lifetime heap usage.
- Keep Zephyr kernel stacks, system heap, network packet pools, Wi-Fi driver
  storage, and other platform symbols separate unless platform RAM policy is
  explicitly in scope.

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

## Firmware Lockup Triage

- Add a firmware lockup triage pass for ESP32-C3 hardware work. When flashing
  succeeds but serial commands stall, app launch hangs, or input dispatch stops
  responding, check stack exhaustion early with `device resources`, compare
  protocol/main and VM worker stack used/unused values, and inspect recent FFI,
  metadata parsing, storage, and service paths for hidden stack temporaries
  before treating GPIO, flashing, or serial as the primary failure.
- Keep hardware scripts on the shared bounded command helper so command failure
  and timeout reports include captured output and diagnostics.

## Explicit State Machines

- Refactor implicit runtime state-machine concepts into explicit, documented,
  testable abstractions where the transition model is already meaningful.
- Treat the app lifecycle as the first candidate. Document stable states,
  events, failure handling, host command behavior, ownership boundaries, and
  cross-platform contract versus target-specific wiring.
- Follow with device input press/release/debounce/gesture recognition,
  planned-sleep prepare/ready/restore coordination, protocol transfer sessions
  for install/temp/resource uploads, scoped scratch-buffer ownership, and
  reusable timed output patterns for indicator blink/breathe behavior.
- Add Mermaid state or sequence diagrams where they clarify transitions.
- Leave simple trace/output/drawlog buffers as bounded queues rather than
  overfitting them into state machines.
