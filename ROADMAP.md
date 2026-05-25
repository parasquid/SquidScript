# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

## Current Track: Zephyr-Only Firmware Migration

Goal: make Zephyr the only real-firmware runtime while keeping Rust authoritative
for compiler, SQBC tooling, and VM semantics.

### 1. Keep Future Zephyr VM Hosting ABI Additions Covered

- Keep the Zephyr VM ABI aligned with implemented SQBC builtins. Current
  builtins `1..45` have Rust VM host callbacks plus Zephyr FFI and runtime
  connections for state, app lifecycle, display draw-log, GPIO, indicator,
  timers, Wi-Fi, app inspection, system diagnostics, and device configuration
  result records. Future service work should promote a spec/API slice through
  compiler lowering, SQBC builtin IDs, VM host callbacks, FFI, Zephyr runtime
  wiring, docs, and tests together.
- The currently implemented Zephyr VM host callbacks have Rust FFI equivalence
  tests and Zephyr ztests for their success, boundary, unsupported, and
  error/status behavior where those states apply. Keep
  `docs/zephyr_vm_host_abi_coverage.md` current when future callbacks are
  added, and keep future service additions on the same caller-owned-buffer
  pattern used by `system.memory()` and `system.storage("apps")`.

### 2. Port Runtime Services To Zephyr

- Complete the physical SQDEVICE/SQDC backend for current device configuration
  APIs. `device.config.load("package:...")` now reads installed foreground app
  package resources into a bounded runtime draft, and `device.config.set(...)`
  edits that draft through the no-alloc Rust FFI core. `device.config.rebind`
  now validates and activates `indicator.default` GPIO bindings plus package
  display bindings such as `display.status`, and `device.config.save("flash")`
  persists binary SQDC to firmware-owned storage. Saved global SQDC defaults
  are loaded during app-start binding initialization before app-local
  `device {}` bindings. Top-level app `device {}` binding classification,
  package display binding planning, and inline GPIO SQDC normalization now live
  in Rust FFI; keep future binding planners on that side of the boundary.
  Remaining work is to generalize binding validation/application for additional
  services beyond the current indicator and display paths.
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
- Add a generic PWM-capable LED-like device output model beyond
  `service.indicator`, so future target-described GPIO/PWM endpoints can expose
  smooth brightness control without board-specific app code.
- Extend top-level `device {}` bindings beyond the current
  `indicator.default` implementation. The compiler, SQBC metadata, and Zephyr
  runtime now support packaged `.sqdevice` resources and simple inline GPIO
  resources such as `indicator { use "gpio:GPIO8" }` for one-pin LED cases
  when the selected target metadata marks that pin GPIO-capable. Zephyr also
  applies packaged display bindings such as
  `display "status" { use "device/status-display.sqdevice" }` into the runtime
  active-binding table before app start. Remaining work is to support multiple
  `use` entries for one logical indicator when the app intentionally wants
  `service.indicator.write(...)` to drive more than one physical output, and
  extend the normalized binding model to additional services beyond indicator
  and display.
- Reduce ESP32-C3 Zephyr RAM after service parity. Identify concrete reductions
  for the largest static allocations, especially VM runtime storage, work
  stacks, response/session buffers, logging, LittleFS pools, and file caches.
  Use `device resources` worker-stack and protocol-stack high-water diagnostics
  before lowering stack budgets. A targeted inline device-binding launch check
  measured `protocol_thread_stack_used_bytes=7604` of 8192 and
  `vm_worker_stack_used_bytes=17056` of 24576 after moving launch scratch
  storage out of the protocol stack. The latest full ESP32-C3 suite measured
  `vm_worker_stack_used_bytes=24448` of 24576, and display isolation showed
  that `screen.open(...)` into a screen with only `service.display.clear`
  reaches `vm_worker_stack_used_bytes=24020`, so reduce the nested screen-render
  interpreter path before lowering that budget. The current Wi-Fi-enabled build
  is under the RAM guard at `dram0_0_seg=206360` linker bytes with the
  hardware-verified 40960-byte Zephyr system heap, so remaining RAM reduction
  is post-parity optimization rather than a feature-parity blocker.
- Audit remaining firmware, FFI, protocol, and hardware-helper fixed buffers;
  replace accidental stack or harness buffers with caller-owned, borrowed,
  streaming, file-backed, or VM-owned storage where practical.
