# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

## Current Track: Zephyr-Only Firmware Migration

Goal: make Zephyr the only real-firmware runtime while keeping Rust authoritative
for compiler, SQBC tooling, and VM semantics.

### 1. Expand Zephyr VM Hosting ABI

- Keep the Zephyr VM ABI aligned with implemented SQBC builtins. Current
  builtins `1..45` have Rust VM host callbacks plus Zephyr FFI and runtime
  connections for state, app lifecycle, display draw-log, GPIO, indicator,
  timers, Wi-Fi, app inspection, system diagnostics, and device configuration
  result records. Future service work should promote a spec/API slice through
  compiler lowering, SQBC builtin IDs, VM host callbacks, FFI, Zephyr runtime
  wiring, docs, and tests together.
- Expand FFI equivalence tests and Zephyr ztests for remaining edge cases in
  storage, state, timers, display, GPIO, Wi-Fi service records, lifecycle
  callbacks, and VM error conversion.
  `system.memory()` and `system.storage("apps")` now have Zephyr FFI host
  callbacks and hardware coverage; keep future service additions on the same
  caller-owned-buffer pattern. Explicit Zephyr VM FFI status-to-errno mapping
  and `device errors` status labels are implemented.

### 2. Port Runtime Services To Zephyr

- Complete the physical SQDEVICE/SQDC backend for current device configuration
  APIs. `device.config.load("package:...")` now reads installed foreground app
  package resources into a bounded runtime draft, and `device.config.set(...)`
  edits that draft through the no-alloc Rust FFI core. `device.config.rebind`
  now validates and activates `indicator.default` GPIO bindings, and
  `device.config.save("flash")` persists binary SQDC to firmware-owned storage.
  Saved global SQDC defaults are loaded during app-start binding initialization
  before app-local `device {}` bindings. Remaining work is to generalize
  binding validation/application beyond the current indicator path.
- Finish moving `service.indicator.*` ownership to the resolved logical
  `indicator.default` binding. The Zephyr runtime now tracks an active
  indicator binding and routes indicator output through it, including package
  SQDEVICE `device.config.rebind(...)` and installed app top-level
  `device { indicator { use ... } }` activation before `app.start`. Target
  default indicator bindings now initialize through the same SQDC draft/rebind
  path instead of direct runtime field assignment. The current ESP32-C3 Super
  Mini behavior should remain GPIO8 LEDC PWM by default. Remaining work is to
  source those defaults from target metadata instead of Zephyr devicetree
  aliases alone.
- Decide service priority and target support for currently spec-recognized but
  not SQBC-backed APIs: `httpServer.*`, `bleTransfer.*`, and any remaining
  `content.*` APIs beyond the current file pick/read family. Defer
  `binbook.*` firmware/compiler/FFI work until the e-paper display is available
  and the BinBook spec has settled enough to avoid optimizing around rough
  draft behavior.
  `content.pickFile(extension)`, `content.readText(path)`, and
  `content.readLines(path, maxLines)` now have compiler/SQBC lowering plus Rust
  VM, FFI, Zephyr callback, ztest, and hardware-script coverage that returns
  honest unsupported result records until real external content support exists.
  Add each remaining API only as a real compiler/SQBC/VM/Zephyr slice with
  honest unsupported behavior until then.
- Design app-entry versus import-only source semantics before adding real
  include/import expansion. Only an app entry file should become an app;
  include/import files should be reusable declarations and should not synthesize
  screens by themselves. Use that design pass to settle related module
  questions such as symbol namespacing, declaration override rules,
  package/import versioning, duplicate declarations across files, and what
  app-lifecycle declarations are legal in import-only files.
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
- Extend top-level `device {}` bindings beyond the current
  `indicator.default` implementation. The compiler, SQBC metadata, and Zephyr
  runtime now support packaged `.sqdevice` resources and simple inline GPIO
  resources such as `indicator { use "gpio:GPIO10" }` for one-pin external LED
  cases. Remaining work is to support multiple `use` entries for one logical
  indicator when the app intentionally wants `service.indicator.write(...)` to
  drive more than one physical output, validate inline GPIO bindings against
  target metadata, and generalize the normalized binding model beyond
  `indicator.default`.
- Reduce ESP32-C3 Zephyr RAM after service parity. Identify concrete reductions
  for the largest static allocations, especially VM runtime storage, work
  stacks, response/session buffers, logging, LittleFS pools, and file caches.
  Use `device resources` worker-stack and protocol-stack high-water diagnostics
  before lowering stack budgets; recent hardware measurements were
  `protocol_thread_stack_used_bytes=4256` of 8192 and
  `vm_worker_stack_used_bytes=22976` of 24576. The current Wi-Fi-enabled build
  is under the RAM guard at `dram0_0_seg=212480` linker bytes, so this is not a
  feature-parity blocker.
- Audit remaining firmware, FFI, protocol, and hardware-helper fixed buffers;
  replace accidental stack or harness buffers with caller-owned, borrowed,
  streaming, file-backed, or VM-owned storage where practical.
