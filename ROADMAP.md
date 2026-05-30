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
  `file.*` APIs beyond the current file pick/read family. Defer
  `binbook.*` firmware/compiler/FFI work until the e-paper display is available
  and the BinBook spec has settled enough to avoid optimizing around rough
  draft behavior. Add each remaining API only as a real compiler/SQBC/VM/Zephyr
  slice with honest unsupported behavior until then.
- Implement an SSD1677/GDEQ0426T82 SquidScript display backend when the display
  breakout is available. Evaluate `ssd1677-driver` first, compare `ssd1677` if
  needed, and adopt a dependency only after proving bounded strip/window
  writes, caller-owned buffers, and bounded/nonblocking BUSY handling for
  constrained firmware RAM.
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
- Extend planned-sleep wake sources beyond the current ESP32-C3 timer-wake
  slice. Investigate safe GPIO wake for physical inputs without using BOOT/GPIO9
  as the default wake source, and keep wake trigger metadata derived from
  installed app trigger declarations rather than persisted VM state.
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
  caches. Current target configuration keeps the protocol/main stack at
  8192 bytes and the VM worker stack at 22016 bytes after installed-app
  lifecycle launch and trigger metadata registration exposed stack exhaustion at
  lower budgets. The stack harness fails with captured resources if
  protocol/main unused stack drops below 768 bytes or VM worker unused stack
  drops below 384 bytes. For host-side attribution before physical
  confirmation, build with `SQUID_ZEPHYR_STACK_USAGE=1` and run
  `scripts/c3-supermini-stack-usage-report.sh` to sort generated Zephyr app C
  `.su` stack-usage records. The report includes source-known cumulative call
  paths; check those before splitting more app-store or protocol helpers
  because per-function `.su` reductions can increase real stack high-water use
  when a larger callee remains active under the caller.
  Current ESP32-C3 build evidence after compact substring-capable VM string
  interning reports 198,320 bytes of linker DRAM, 198,304 bytes through
  `zephyr-ram-audit`, and a 15,128-byte `runtime.4` static runtime symbol.

  Current stack report evidence shows that the previous
  `commit_install -> sq_app_store_scan_registry_with_path -> join_path2`
  target is no longer current. `commit_install` now updates the mutable app
  registry entry directly through `sq_app_store_update_registry_entry_with_path`
  after staged-file rename, reusing the install session's staging-path scratch
  and avoiding a full directory scan. The emitted `commit_install` frame is
  48 bytes, `sq_app_store_commit_staged_install` is 96 bytes, and
  `sq_app_store_update_registry_entry_with_path` uses `fs_open` plus
  `fs_seek`/`fs_tell` instead of a `struct fs_dirent` stat buffer. The remaining
  registry scan path is the public/admin
  `sq_app_store_scan_registry -> sq_app_store_scan_registry_with_path ->
  join_path2` path at 256 cumulative bytes. Its live storage is the public
  wrapper's 64-byte app-file path scratch plus the scan helper's
  `struct fs_dir_t`, `struct fs_dirent`, and small scalars. It is not on the
  install-commit protocol path.

  Current source-known protocol/main stack targets are planned-resume paths:
  `main -> sq_device_protocol_restore_planned_resume -> register_app_triggers ->
  register_app_trigger_timer -> sq_vm_runtime_register_armed_timer` at
  864 cumulative bytes, and `sq_device_protocol_poll ->
  write_planned_resume_file -> sq_device_protocol_encode_planned_resume ->
  append_fixed_app_id` at 656 cumulative bytes. The restore frame carries a
  planned-resume record, encoded record bytes, a 48-byte planned-resume path,
  an `fs_file_t`, and scalars before re-registering armed app triggers. The
  write frame carries a planned-resume record, encoded record bytes, separate
  48-byte temp and final paths needed for the temp-write-plus-rename protocol,
  an `fs_file_t`, and scalars. Next RAM work should investigate whether those
  planned-resume record/encoded-byte/path/file temporaries can use
  caller-owned, runtime-owned, or streaming storage without weakening atomic
  checkpoint behavior, app-store correctness, or wake restore semantics.
- Add a firmware lockup triage pass for ESP32-C3 hardware work. When flashing
  succeeds but serial commands stall, app launch hangs, or input dispatch stops
  responding, check stack exhaustion early with `device resources`, compare
  protocol/main and VM worker stack used/unused values, and inspect recent FFI,
  metadata parsing, storage, and service paths for hidden stack temporaries
  before treating GPIO, flashing, or serial as the primary failure. Hardware
  scripts now use the shared bounded command helper, which prints captured
  command output when a command fails or times out.
- Investigate the ESP32-C3 app-registry hardware check returning an empty host
  `app list` after `app install` reports success. This reproduces on clean
  firmware with no `device errors` output after restoring the app-store
  directory check to `fs_stat`, so do not use
  `scripts/c3-supermini-test-app-registry-api.sh` as evidence for stack changes
  until the registry/listing path is diagnosed.
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
  without targeted confirmation. The protocol/main stack budget was
  reduced from 8 KiB to 3264 bytes based on repeated 2476-byte measured peaks,
  then restored to 8 KiB after installed-app lifecycle launch and trigger
  metadata registration exposed a protocol/main-side fatal crash at the smaller
  budget. The worker stack was reduced from
  19 KiB to 18016 bytes based on saved workload peaks, then raised to 22016
  bytes after installed-app lifecycle launch testing exposed a fatal crash
  consistent with worker-stack exhaustion. Next, validate the 8192-byte
  protocol/main stack and 22016-byte worker stack with the hardware suite, then
  re-attribute trigger metadata and app-launch paths before attempting another
  protocol/main stack reduction.
- Improve network heap attribution before expanding Wi-Fi scope. Current AP
  start/stop hardware coverage drives `heap_max_alloc_bytes` close to
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
- Audit remaining firmware, FFI, protocol, and hardware-helper fixed buffers;
  replace accidental stack or harness buffers with caller-owned, borrowed,
  streaming, file-backed, or VM-owned storage where practical.
- Add heap fragmentation diagnostics and mitigation for ESP32-C3 Zephyr RAM
  work. `system.memory()` and/or `device resources` should distinguish total
  free heap from allocator fragmentation by reporting the largest allocatable
  block when Zephyr exposes it, allocation high-water data, and subsystem
  allocation failures where practical. Use the data to keep SquidScript runtime
  paths on fixed arenas, caller-owned buffers, bounded scratch, slabs/pools for
  unavoidable dynamic allocations, and startup-owned long-lived allocations
  instead of mixed-lifetime heap usage. This should help explain failures where
  free heap appears sufficient but a larger contiguous allocation cannot be
  satisfied.
- Refactor implicit runtime state-machine concepts into explicit, documented,
  testable abstractions where the transition model is already meaningful.
  Treat the app lifecycle as the first candidate, followed by device input
  press/release/debounce/gesture recognition, planned-sleep
  prepare/ready/restore coordination, protocol transfer sessions for
  install/temp/resource uploads, scoped scratch-buffer ownership, and reusable
  timed output patterns for indicator blink/breathe behavior. For each
  abstraction, document the stable states, events, failure handling, ownership
  boundaries, and cross-platform contract versus target-specific wiring; add
  Mermaid state or sequence diagrams where they make transitions clearer.
  Keep Wi-Fi scan/connect/AP lifecycle as a separate future service-state
  machine item unless Wi-Fi work is explicitly in scope, and leave simple
  trace/output/drawlog buffers as bounded queues rather than overfitting them
  into state machines.
- Design a cursor-style Wi-Fi scan API so targets can expose more scan results
  without materializing every AP record and string into one VM event. Compare
  options such as `wifi.scan()` returning a snapshot handle with
  `wifi.scan.get(scan, index)`, paged scan reads, or an iterator-like cursor,
  and keep SSID/BSSID/auth strings backed by host/runtime storage until the app
  asks for a specific network.
