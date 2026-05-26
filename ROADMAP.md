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
- Complete a broader ESP32-C3 RAM improvement investigation. Build a measured
  map of stack, heap, static, filesystem, logging, protocol/session, VM runtime,
  and service-specific memory use across boot, app install, app launch, input
  dispatch, storage, and network workloads; explain peaks and outliers, then
  split reductions into concrete implementation tasks.
- Improve network heap attribution before expanding Wi-Fi scope. Current AP
  start/stop hardware coverage drives `ram_heap_max_allocated_bytes` close to
  the 36 KiB system heap budget; add clearer per-workload heap reset or
  attribution before TCP, AP client throughput, BLE coexistence, or larger
  network workloads.
- Bound every hardware-test serial command, not only the outer polling loop.
  During GPIO9 input-button testing, `scripts/c3-supermini-test-input-button.sh`
  launched successfully but hung inside a `device output` command before its
  `wait_for_contains` retry loop could time out. Move hardware scripts to a
  shared helper that wraps each `squidc device/app` command with a command-level
  timeout and captures enough context to diagnose protocol stalls without
  wedging the whole script.
- Reduce main/protocol stack pressure from top-level device binding planning.
  GPIO-button testing showed device binding metadata planning can use almost
  10 KiB of main stack on ESP32-C3, so the current firmware raises
  `CONFIG_MAIN_STACK_SIZE` to 12288. Move that work to a slimmer parser path,
  caller-owned scratch, or the VM worker path so the main serial loop can return
  to a smaller measured budget.
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
