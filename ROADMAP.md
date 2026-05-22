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

### 2. Expand Zephyr VM Hosting ABI

- Extend the C ABI beyond resumable dispatch to cover state inspection,
  diagnostics, service result conversion, and explicit error mapping.
- Expand FFI equivalence tests and Zephyr ztests for storage, state, timers,
  display, GPIO, Wi-Fi service records, lifecycle callbacks, and VM errors.

### 3. Port The Firmware Command Surface

- Implement the Zephyr command owner for app install, temp run, launch, app
  list, key events, output, draw log, state, resources, errors, reset, and
  storage formatting.
- Extend the established Rust/Python/Zephyr framed serial protocol beyond hello
  identity through `squidc` device/run/app/repl paths and hardware helper
  scripts.
- Remove or replace remaining old text serial protocol assumptions in host
  tests and examples.

### 4. Port Runtime Services To Zephyr

- Implement GPIO, indicator, PWM, timers, app lifecycle, persistent app storage,
  app state, display/draw-log, Wi-Fi scan, AP, station, status, and profile
  provisioning through Zephyr-native subsystems.
- Keep service behavior non-blocking: use Zephyr timers, work queues, message
  queues, flash-map, NVS, LittleFS, networking, and Wi-Fi management events
  instead of firmware busy waits.
- Audit remaining firmware, FFI, protocol, and hardware-helper fixed buffers;
  replace accidental stack or harness buffers with caller-owned, borrowed,
  streaming, file-backed, or VM-owned storage where practical.
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
