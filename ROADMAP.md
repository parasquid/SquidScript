# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

## Current Track: Canonical Zephyr Firmware

Goal: keep Zephyr as the canonical firmware architecture while Rust remains
authoritative for compiler, SQBC tooling, and VM semantics.

### 1. Extend Canonical Zephyr Runtime Services

- Complete the physical SQDEVICE/SQDC backend for current device configuration
  APIs. `device.config.load("package:...")` now reads installed foreground app
  package resources into a bounded runtime draft, and `device.config.set(...)`
  edits that draft through the no-alloc Rust FFI core. `device.config.rebind`
  now validates and activates `indicator.default` GPIO bindings plus package
  display bindings such as `display.status`, and `device.config.save("flash")`
  persists binary SQDC to firmware-owned storage. Saved global SQDC defaults
  are loaded during app-start binding initialization before app-local
  `device {}` bindings. Top-level app `device {}` binding classification,
  package display binding planning, inline GPIO indicator SQDC normalization,
  and inline GPIO-button input metadata planning now live in Rust FFI; keep
  future binding planners on that side of the boundary. Remaining work is to
  activate GPIO-button input bindings in Zephyr and prove `key.SELECT` dispatch
  from the ESP32-C3 BOOT/GPIO9 button when physical button testing is possible.
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
- Reduce ESP32-C3 Zephyr RAM as canonical firmware hardening. Identify concrete
  reductions for the largest static allocations, especially VM runtime storage, work
  stacks, response/session buffers, logging, LittleFS pools, and file caches.
  Use `device resources` worker-stack and protocol-stack high-water diagnostics
  before lowering stack budgets. A targeted inline device-binding launch check
  measured `protocol_thread_stack_used_bytes=7604` of 8192 and
  `vm_worker_stack_used_bytes=17056` before the worker stack moved to 20480,
  after launch scratch storage moved out of the protocol stack. After flattening the resumable
  screen-render interpreter path, the headless draw-log isolation app using
  `screen.open(...)` into a screen with only `service.display.clear("gray0")`
  measured `vm_worker_stack_used_bytes=17056`, down from the previous
  24020-byte display-only spike. The latest full ESP32-C3 suite measured
  `vm_worker_stack_used_bytes=17620` before the worker stack moved to 20480.
  The current Wi-Fi-enabled build is under the RAM guard at `dram0_0_seg=198744` audit bytes with
  `runtime_static_bytes=18704` and the hardware-verified 36864-byte Zephyr
  system heap, so remaining RAM reduction is hardening work rather than a
  service blocker.
- Audit remaining firmware, FFI, protocol, and hardware-helper fixed buffers;
  replace accidental stack or harness buffers with caller-owned, borrowed,
  streaming, file-backed, or VM-owned storage where practical.
