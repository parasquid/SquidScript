# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

## Current Track: Zephyr-Only Firmware Migration

Goal: make Zephyr the only real-firmware runtime while keeping Rust authoritative
for compiler, SQBC tooling, and VM semantics.

### 1. Complete Zephyr Toolchain Bring-Up

- Confirm the correct Zephyr board target for the ESP32-C3 Super Mini and
  replace the unverified `esp32c3_devkitm/esp32c3` wrapper default if needed.
- Confirm the Zephyr SDK version for this project after the first hardware
  build succeeds.
- Add a Zephyr hardware diagnostic test that builds, flashes, boots, and reads
  the diagnostic banner over the serial monitor.
- Install or document the missing Twister Python dependencies so
  `firmware/zephyr/tests/protocol` can run through Twister, not only direct
  `west build -t run`.

### 2. Expand Zephyr VM Hosting ABI

- Extend the C ABI beyond resumable dispatch to cover state inspection,
  diagnostics, service result conversion, and explicit error mapping.
- Expand FFI equivalence tests and Zephyr ztests for storage, state, timers,
  display, GPIO, Wi-Fi service records, lifecycle callbacks, and VM errors.

### 3. Rust-Own Firmware Protocol Logic

- Move remaining Zephyr C response payload builders and simple request parsers
  to heap-free Rust `sqdp_` helpers where doing so reduces stack buffers or
  duplicated TLV rules without duplicating Zephyr storage/runtime ownership.
- Keep `squidc`, Python helpers, Zephyr tests, and FFI tests on shared codec
  fixtures so there is one current wire implementation.

### 4. Port Runtime Services To Zephyr

- Implement GPIO, indicator, PWM, timers, app lifecycle, persistent app storage,
  app state, display/draw-log, Wi-Fi scan, AP, station, status, and profile
  provisioning through Zephyr-native subsystems.
- Preserve foreground VM in-memory state across app events without requiring
  every key/timer handler to call `state.load()`, while keeping app trigger
  registration from clobbering the foreground app context.
- Replace `event.on("app.arm")` trigger registration with an explicit
  `app.triggers { ... }` language construct. `app.arm(appId)` should register
  the target app's trigger declarations without replacing the current
  foreground app or keeping a background VM resident. The construct should
  support multiple bounded registrations per app, including different timer
  intervals and future logical button/input triggers, while `event.on(...)`
  remains the handler for the activation event that fires later.
- Design the `app.triggers` compiler/SQBC/VM contract so firmware can load only
  the trigger registration section plus its required constants/functions, not
  the full app, and so unsupported foreground operations in trigger
  registration are rejected. Update lifecycle diagnostics to expose both the
  process return stack and armed trigger stack with enough detail for tests.
- Add a generic PWM-capable LED-like device output model beyond
  `service.indicator`, so future target-described GPIO/PWM endpoints can expose
  smooth brightness control without board-specific app code.
- Keep service behavior non-blocking: use Zephyr timers, work queues, message
  queues, flash-map, NVS, LittleFS, networking, and Wi-Fi management events
  instead of firmware busy waits.
- Defer ESP32-C3 Zephyr RAM optimization until after the current service-parity
  unblock slices. When resumed, identify concrete reductions for the largest
  static allocations, especially VM runtime storage, work stacks,
  response/session buffers, logging, LittleFS pools, and file caches. Keep the
  RAM audit guard meaningful and record tradeoffs before lowering capability.
- Audit remaining firmware, FFI, protocol, and hardware-helper fixed buffers;
  replace accidental stack or harness buffers with caller-owned, borrowed,
  streaming, file-backed, or VM-owned storage where practical.
- Use `device resources` VM worker stack high-water diagnostics to reduce stack
  RAM only when representative ESP32-C3 hardware workloads prove headroom.
  Current state/lifecycle flows use most of the 16 KiB worker stack, so inspect
  C and Rust dispatch paths for large locals, formatting-heavy diagnostics,
  nested FFI/host callbacks, and architecture-specific stack costs before
  changing the stack budget.
- Preserve portable SquidScript service semantics in docs/specs; keep Zephyr
  Kconfig, devicetree, pins, partitions, and driver details in firmware/target
  docs and metadata.

### 5. Replace Hardware Tests

- Convert ESP32-C3 hardware tests to Zephyr-only scripts and run them
  sequentially on the single serial device.
- Include Wi-Fi scan tests without credentials and station tests only when
  credentials are explicitly provided.
- Keep the final visible board-state check last in the hardware suite.

### 6. Remove Obsolete Rust Firmware

- Delete `firmware/squid-firmware` after the Zephyr command surface, storage,
  lifecycle, and hardware tests cover the current required behavior.
- Remove old Rust firmware build/flash/test scripts once their Zephyr
  replacements exist.
- Keep only concise obsolete-reference notes when they help explain removed
  behavior; do not carry old APIs, protocols, storage formats, or compatibility
  paths.
