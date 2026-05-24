# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

## Current Track: Zephyr-Only Firmware Migration

Goal: make Zephyr the only real-firmware runtime while keeping Rust authoritative
for compiler, SQBC tooling, and VM semantics.

### 1. Complete Zephyr Toolchain Bring-Up

- When physical board observation is available, re-run the Zephyr
  `breathe-supermini` visible LED check and verify whether the current
  LEDC PWM polarity is correct before changing the overlay. This is not a
  current blocker; the physical observation can wait until the user is back at
  the board.

### 2. Expand Zephyr VM Hosting ABI

- Keep the Zephyr VM ABI aligned with implemented SQBC builtins. Current
  builtins `1..21` and `27..37` have Rust VM host callbacks plus Zephyr FFI and
  runtime connections for state, app lifecycle, display draw-log, GPIO,
  indicator, timers, Wi-Fi, and system diagnostics. Future service work should
  promote a spec/API slice through compiler lowering, SQBC builtin IDs, VM host
  callbacks, FFI, Zephyr runtime wiring, docs, and tests together.
- Expand FFI equivalence tests and Zephyr ztests for remaining edge cases in
  storage, state, timers, display, GPIO, Wi-Fi service records, lifecycle
  callbacks, and VM error conversion.
  `system.memory()` and `system.storage("apps")` now have Zephyr FFI host
  callbacks and hardware coverage; keep future service additions on the same
  caller-owned-buffer pattern. Explicit Zephyr VM FFI status-to-errno mapping
  and `device errors` status labels are implemented.

### 3. Port Runtime Services To Zephyr

- Extend app-facing lifecycle inspection beyond the implemented low-RAM
  `app.registry()` and `app.registry.get(apps, index)` installed-app listing.
  Reuse the existing foreground stack and armed timer state; define the
  portable SquidScript contract first, then add compiler/SQBC/VM/FFI/Zephyr
  support and hardware coverage for inspecting foreground and armed-app state.
- Promote planned display APIs through the real runtime stack:
  `service.display.select`, `service.display.image`, and
  `service.display.draw`. Keep resource/drawable ownership explicit and avoid
  inventing simulator-only syntax.
- Promote planned device configuration APIs through the real runtime stack:
  `device.config.load`, `device.config.set`, `device.config.rebind`, and
  `device.config.save`. Align these with `.sqdevice` package resources and the
  future top-level `device {}` binding model.
- Decide service priority and target support for currently spec-recognized but
  not SQBC-backed APIs: `httpServer.*`, `bleTransfer.*`, `content.*`, and
  `binbook.*`. Add each only as a real compiler/SQBC/VM/Zephyr slice with
  honest unsupported behavior until then.
- Design app-entry versus import-only source semantics before adding no-screen
  app sugar. The likely direction is: only an app entry file can become an app,
  include/import files are reusable declarations and never synthesize screens by
  themselves, and after whole-app include expansion an app with no explicit
  `screen(...)` declarations gets compiler-synthesized `screen("main") {}`.
  Keep rejecting unknown `screen.open(...)` targets, document the sugar, and add
  compiler/SQBC tests proving no-screen apps compile to one empty `main` screen.
  Use the same design pass to settle related module questions such as symbol
  namespacing, declaration override rules, package/import versioning, duplicate
  declarations across files, and what app-lifecycle declarations are legal in
  import-only files.
- Add external Wi-Fi AP client association/DHCP lease proof through
  Zephyr-native subsystems.
- Decide whether the ESP32-C3 Super Mini reference target should expose
  `bleTransfer.*`; if yes, implement and verify it through Zephyr BLE instead
  of relying on MCU radio metadata alone.
- Extend the `app.triggers` model beyond current timer metadata declarations to
  future logical button/input triggers while keeping `event.on(...)` as the
  handler for the activation event that fires later.
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
- Reduce ESP32-C3 Zephyr RAM after service parity. Identify concrete reductions
  for the largest static allocations, especially VM runtime storage, work
  stacks, response/session buffers, logging, LittleFS pools, and file caches.
  Use `device resources` worker-stack and protocol-stack high-water diagnostics
  before lowering stack budgets; recent hardware measurements were
  `protocol_thread_stack_used_bytes=4256` of 8192 and
  `vm_worker_stack_used_bytes=16000` of 24576. The current Wi-Fi-enabled build
  is under the RAM guard at `dram0_0_seg=212704` linker bytes, so this is not a
  feature-parity blocker.
- Audit remaining firmware, FFI, protocol, and hardware-helper fixed buffers;
  replace accidental stack or harness buffers with caller-owned, borrowed,
  streaming, file-backed, or VM-owned storage where practical.

### 4. Remove Obsolete Rust Firmware

- Delete `firmware/squid-firmware` after the Zephyr command surface, storage,
  lifecycle, and hardware tests cover the current required behavior.
- Remove old Rust firmware build/flash/test scripts once their Zephyr
  replacements exist.
- Keep only concise obsolete-reference notes when they help explain removed
  behavior; do not carry old APIs, protocols, storage formats, or compatibility
  paths.
