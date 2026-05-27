# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

## Current Track: Canonical Zephyr Firmware

Goal: keep Zephyr as the canonical firmware architecture while Rust remains
authoritative for compiler, SQBC tooling, and VM semantics.

### 1. Extend Canonical Zephyr Runtime Services

- Decide service priority and target support for currently spec-recognized but
  not SQBC-backed APIs: `httpServer.*`, `bleTransfer.*`, and any remaining
  `content.*` APIs beyond the current file pick/read family. Defer
  `binbook.*` firmware/compiler/FFI work until the e-paper display is available
  and the BinBook spec has settled enough to avoid optimizing around rough
  draft behavior. Add each remaining API only as a real compiler/SQBC/VM/Zephyr
  slice with honest unsupported behavior until then.
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
- Design and implement richer logical input events for press and release
  phases, long press, double tap, and chords. Specify naming, target policy,
  precedence, debounce/timing windows, and whether recognized long/chord/double
  gestures suppress component short press/release events. Include a way for app
  or device input configuration to set long-press duration thresholds, likely
  through `device { input ... }` binding metadata or a related input config
  block, while preserving target defaults and target-owned system actions such
  as long `POWER` sleep.
- Add a SquidScript GPIO input configuration affordance for raw hardware
  diagnostics and target-specific local inputs. It should let code or device
  binding metadata request input bias such as pull-up, pull-down, or floating
  where the target supports it, while keeping portable service APIs separate
  from board-specific GPIO names. Use it to make ESP32-C3 Super Mini diagnostic
  scans less noisy without changing the confirmed GPIO9 BOOT binding from
  active-low pull-up behavior. Use Espruino's split between pin mode
  (`input_pullup`/`input_pulldown`) and watches (`edge: rising/falling/both`,
  `debounce`) as design inspiration, but adapt it to SquidScript's explicit
  device/input binding model instead of adding global auto-mode side effects.
- Add a generic PWM-capable LED-like device output model beyond
  `service.indicator`, so future target-described GPIO/PWM endpoints can expose
  smooth brightness control without board-specific app code.
- Support multiple `use` entries for one logical indicator when an app
  intentionally wants `service.indicator.write(...)` to drive more than one
  physical output.
- Reduce ESP32-C3 Zephyr RAM as canonical firmware hardening. Identify concrete
  reductions for the largest static allocations, especially VM runtime storage,
  work stacks, response/session buffers, logging, LittleFS pools, and file
  caches.
- Add a firmware lockup triage pass for ESP32-C3 hardware work. When flashing
  succeeds but serial commands stall, app launch hangs, or input dispatch stops
  responding, check stack exhaustion early with `device resources`, compare
  protocol/main and VM worker stack used/unused values, and inspect recent FFI,
  metadata parsing, storage, and service paths for hidden stack temporaries
  before treating GPIO, flashing, or serial as the primary failure.
- Complete physical GPIO9 input stack attribution and budget reduction.
  `scripts/c3-supermini-measure-input-stack-isolation.sh` now records a
  fresh-boot physical input path baseline through format, install, launch, and
  observed BOOT/GPIO9 press. Current launch evidence shows protocol/main stack
  flat at 2476 bytes and VM worker stack reduced from 17296 to 17056 bytes by
  narrowing FFI app process/armed stack scratch to the firmware's two-entry
  limit. The launch and timeout rows now include `input_button_state`, where the
  low byte is configured input count and the next byte is currently pressed
  count, so timeout runs can prove whether the physical binding was installed
  and whether the firmware saw the line pressed. Current hardware evidence has
  `input_button_state=1` at launch and after release, meaning the GPIO9 binding
  is installed and the line now reads released after devicetree pull-up
  configuration. The input isolation script now asks for a held press and
  separates electrical press timeout from dispatch/output timeout. The raw GPIO9
  probe confirms `hardware.gpio.read("GPIO9")` sees released as `true` and held
  as `false` with the same pull-up configuration, separating physical pin
  visibility from event dispatch. A completed physical GPIO9 input-stack run
  observed `after-press-observed` with `input_button_state=257`, proving one
  configured input and one currently pressed input; app output changed from
  `output=count 0` to `output=count 1`. The press row kept protocol/main stack
  flat at 2476 bytes and VM worker stack at 17136 bytes with 2320 bytes free in
  the current skip-flash run. The broader BOOT-button pin scan samples GPIO0
  through GPIO10 and now requires repeated stable changed samples before
  accepting a result because a pen-held tiny button can slip and floating or
  unconfigured pins can produce one-off changed samples. For the ESP32-C3 Super
  Mini reference board, treat GPIO9 as the confirmed physical input path; do
  not treat GPIO3, GPIO4, GPIO7, GPIO10, or GPIO5 scan changes as real buttons
  without targeted confirmation. Next, explain any press-time peak, then
  validate whether the 8 KiB protocol/main stack or 19 KiB worker stack can be
  reduced after full-suite coverage.
- Improve network heap attribution before expanding Wi-Fi scope. Current AP
  start/stop hardware coverage drives `ram_heap_max_allocated_bytes` close to
  the 36 KiB system heap budget; add clearer per-workload heap reset or
  attribution before TCP, AP client throughput, BLE coexistence, or larger
  network workloads.
- Convert blocking Wi-Fi VM callbacks to nonblocking runtime progress. Current
  Zephyr `wifi.connect`, `wifi.disconnect`, and `wifi.scan` callbacks wait on
  semaphores for up to 15s, 5s, and 8s respectively. They run in the VM worker
  rather than the serial main loop, but they still block app/runtime progress
  and conflict with the firmware rule that services should use short steps,
  async progress, or target scheduler integration.
- Convert remaining synchronous firmware storage and app-store maintenance
  paths to bounded progress where they can run during interactive use. The
  event-driven audit found Zephyr storage requests are resumable at the
  VM/FFI boundary but are completed with synchronous filesystem calls in the
  firmware backend, while registry scans, recursive deletes, and storage format
  loops can run as blocking administrative operations. Keep startup/admin-only
  work explicitly scoped, and move any user-visible or runtime-reachable work
  toward chunked, callback-driven, or scheduler-integrated progress.
- Make runtime string allocation fail instead of silently overwriting or
  truncating. `RuntimeStrings` currently uses a fixed ring of slots and
  silently truncates writes at `MAX_RUNTIME_STRING_BYTES`; this is predictable
  but can corrupt still-live `Value::RuntimeString` references or hide data
  loss. Add explicit overflow/truncation errors while preserving bounded,
  heap-free firmware behavior.
- Audit remaining firmware, FFI, protocol, and hardware-helper fixed buffers;
  replace accidental stack or harness buffers with caller-owned, borrowed,
  streaming, file-backed, or VM-owned storage where practical.
