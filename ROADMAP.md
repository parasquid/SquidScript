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
  `system.memory()` and `system.storage("apps")` now have Zephyr FFI host
  callbacks and hardware coverage; keep future service additions on the same
  caller-owned-buffer pattern.

### 3. Rust-Own Firmware Protocol Logic

- Move remaining Zephyr C response payload builders and simple request parsers
  to heap-free Rust `sqdp_` helpers where doing so reduces stack buffers or
  duplicated TLV rules without duplicating Zephyr storage/runtime ownership.
- Keep `squidc`, Python helpers, Zephyr tests, and FFI tests on shared codec
  fixtures so there is one current wire implementation.

### 4. Port Runtime Services To Zephyr

- Add external Wi-Fi AP client association/DHCP lease proof through
  Zephyr-native subsystems.
- Decide whether the ESP32-C3 Super Mini reference target should expose
  `bleTransfer.*`; if yes, implement and verify it through Zephyr BLE instead
  of relying on MCU radio metadata alone.
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
- Add SQBC lazy bytecode loading for installed apps to reduce firmware RAM.
  Keep a small always-resident SQBC header/index with section, function,
  trigger, constant, and entrypoint metadata; load function bodies or bounded
  bytecode chunks from LittleFS only when the VM enters code that is not
  resident. Model this as a resumable VM storage request through the existing
  Zephyr host boundary with caller-owned fixed buffers, so app arm/trigger
  registration can inspect only the trigger section and later activation can
  load the target function/chunk without keeping a background VM resident.
  Preserve current SQBC semantics; do not add compatibility versioning for
  old bytecode.
- Add a generic PWM-capable LED-like device output model beyond
  `service.indicator`, so future target-described GPIO/PWM endpoints can expose
  smooth brightness control without board-specific app code.
- Extend top-level `device {}` bindings so apps can rebind logical devices
  such as `indicator` at app/package level. Keep the current `.sqdevice`
  package resource form for rich persisted bindings, and add a simple inline
  GPIO form such as `indicator { use "gpio:GPIO10" }` for one-pin external LED
  cases such as Seeed XIAO ESP32C3. Omitted binding names should continue to
  mean `"default"`; support multiple `use` entries for one logical indicator
  when the app intentionally wants `service.indicator.write(...)` to drive more
  than one physical output. Validate inline GPIO bindings against target
  metadata, preserve `service.indicator.*` as the runtime API, and have
  compiler/SQBC/Zephyr normalize inline and `.sqdevice` bindings into the same
  device-binding model.
- Keep service behavior non-blocking: use Zephyr timers, work queues, message
  queues, flash-map, NVS, LittleFS, networking, and Wi-Fi management events
  instead of firmware busy waits.
- Defer ESP32-C3 Zephyr RAM optimization until after the current service-parity
  unblock slices. When resumed, identify concrete reductions for the largest
  static allocations, especially VM runtime storage, work stacks,
  response/session buffers, logging, LittleFS pools, and file caches. Keep the
  RAM audit guard meaningful and record tradeoffs before lowering capability.
  The default ESP32-C3 Super Mini firmware builds with Zephyr ESP32 Wi-Fi
  scan/status/AP/station support, AP DHCPv4 server support, one volatile
  station profile, and station DHCP/IP status reporting at
  `dram0_0_seg=200416` linker bytes, with `scripts/zephyr-ram-audit.sh`
  reporting `dram0_0_seg=200400` bytes, or 48.9% of the target definition's
  400 KiB internal SRAM, after bounding native-network packet/buffer pools and
  measured Wi-Fi socket/event, ESP timer task, and network RX stack budgets for
  current low-throughput service traffic. `device resources` now exposes live
  Zephyr heap telemetry; the first representative Wi-Fi/control workload measured
  `ram_heap_max_allocated_bytes=36764`, so the Zephyr system heap is bounded to
  49152 bytes while retaining roughly 12 KiB of observed high-water headroom.
  The Zephyr VM runtime now reuses the VM initialization scratch transfer
  buffer as the later storage-completion transfer buffer, reducing resident
  runtime static RAM by 1024 bytes without changing service capacity; hardware
  resource diagnostics report `runtime_static_bytes=16608`.
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

### 5. Remove Obsolete Rust Firmware

- Delete `firmware/squid-firmware` after the Zephyr command surface, storage,
  lifecycle, and hardware tests cover the current required behavior.
- Remove old Rust firmware build/flash/test scripts once their Zephyr
  replacements exist.
- Keep only concise obsolete-reference notes when they help explain removed
  behavior; do not carry old APIs, protocols, storage formats, or compatibility
  paths.
