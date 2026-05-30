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
- Replace Zephyr protocol ztest static SQBC byte arrays with generated fixtures
  built from checked-in `.squid` sources, so builtin ID regrouping cannot leave
  protocol tests executing stale bytecode.
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
  caches. Current C3 build map evidence sizes the resident runtime object at
  14,272 bytes after moving Wi-Fi scan result backing into runtime transfer
  scratch, adding a diagnostic transfer-owner marker for scratch/completion/
  Wi-Fi-scan overlap checks, capping foreground runtime timers to two slots,
  capping retained VM output history at twelve lines for lifecycle diagnostics,
  retained VM trace history at four lines,
  narrowing output and drawlog diagnostic line storage, bounding transient VM
  result records to 26 fields, trimming the ESP32-C3 VM context reserve to
  10,400 bytes, reducing app-id slots to 40 bytes, and
  lowering the SQBC code/read transfer window to 768 bytes. The runtime layout
  keeps small flags out of 32-bit alignment gaps and stores fixed-array counts
  as bytes where the backing arrays are capped below 255. Runtime device
  config drafts now hold five records with 48-byte string values, enough for the
  five-record GPIO button binding shape and current package `.sqdevice` resource
  paths without retaining unused slots. Runtime event-name slots are now
  24 bytes, enough for the current measured examples and tests such as
  `timer.breathe.marker` without retaining the previous 32-byte slots. The
  resident installed-app VM launch storage now uses 64-byte SQBC path storage
  and 60-byte state path storage for the fixed installed-app path shapes instead
  of two general 128-byte app-store path buffers. The resident app registry now
  holds eight installed-app entries with 40-byte app-id storage slots for
  current measured app workloads, and the serial
  receive frame budget is 256 bytes with host upload chunking derived from that
  limit, protocol transfer sessions use 80-byte staging path slots and 80-byte
  resource path slots, and resource diagnostics encode directly into the
  826-byte response buffer without a resident metric staging array. Runtime
  physical input state is bounded to two GPIO button slots for the confirmed
  BOOT/GPIO9 path plus one targeted diagnostic slot, foreground runtime timers
  are bounded to two slots for current one-shot/repeating workloads, and active
  device bindings are bounded to three entries for the current indicator,
  display/input, and targeted diagnostic binding workloads. Temp-run
  state now uses the file-backed VM storage backend with a cleared temp state
  path instead of a resident saved-state-capacity RAM buffer, and Zephyr's
  deferred logger buffer and process-thread stack are explicitly bounded at 512
  bytes each. LittleFS open-file slots are bounded at two while directory slots
  remain at the Zephyr default for recursive format/delete walks. The
  protocol/main stack is now 8192 bytes after installed-app lifecycle launch
  coverage exposed a protocol/main-side fatal crash at the earlier 3264-byte
  budget, and the VM worker stack is now 22016 bytes.
  The latest target build reports 184,576 bytes
  of linker DRAM use and 184,560 bytes through the RAM audit; next
  reductions should re-attribute trigger metadata registration, installed-app
  launch, and protocol/main stack use before lowering the 8192-byte protocol
  budget again. The stack harness now fails with captured
  resources if protocol/main unused stack drops below 768 bytes or VM worker
  unused stack drops below 384 bytes. After that, investigate full-suite
  worker-stack high-water headroom after raising the worker stack to 22016 bytes and
  inspect any remaining accidental static buffers. For host-side attribution
  before physical confirmation, build with `SQUID_ZEPHYR_STACK_USAGE=1` and run
  `scripts/c3-supermini-stack-usage-report.sh` to sort generated Zephyr app C
  `.su` stack-usage records. The report includes source-known cumulative call
  paths; check those before splitting more app-store or protocol helpers
  because per-function `.su` reductions can increase real stack high-water use
  when a larger callee remains active under the caller. Current host
  attribution has reduced
  `sq_app_store_scan_registry` from 576 bytes to 224 bytes by reusing its path
  scratch buffer after opening the app directory, reusing its directory entry
  for `main.sqbc` stats, and narrowing the shared app-file path scratch to
  the fixed `/apps/<app>/main.sqbc` shape. The Zephyr filesystem filename
  buffer is now capped at 80 bytes to match the protocol resource-path cap
  instead of keeping 128-byte `fs_dirent` name slots. It has also reduced
  `sq_app_store_install_resource` plus `sq_app_store_commit_staged_resource`
  from 432 bytes each to 176 bytes each by reusing path scratch and validating
  the app's `main.sqbc` with an open/close check instead of a directory-entry
  stat. Their parent-directory creation now reuses the caller-owned path
  scratch and calls `fs_mkdir` directly, accepting `-EEXIST` instead of probing
  existing parents with `fs_stat`, reducing `ensure_resource_parent_dirs` from
  176 bytes to 48 bytes and the source-known `commit_resource_install` path
  from 496 bytes to 304 bytes. The current cumulative
  `sq_app_store_commit_staged_resource` path is 272 bytes via
  `validate_app_main_sqbc` and `format_app_path`. Direct app install and
  staged-install commit paths now use the fixed
  app-file path scratch and emit 96-byte C stack estimates, down from 288 and
  304 bytes respectively; staged-install begin now emits 112 bytes. Package
  `.sqdevice` loads now format resource paths directly from validated bytes,
  reducing `sq_vm_runtime_device_config_load_resource` from 304 bytes to 176
  bytes. Recursive app-store format/delete walks now reuse the caller-owned
  path buffer instead of allocating a full child path per recursion, reducing
  `delete_files_under` from 320 bytes to 160 bytes. The storage-format path now
  reuses its format path scratch when recreating top-level app-store
  directories, reducing the source-known `storage_format` path from 448 bytes
  to 352 bytes and the top source-known main/protocol path from 656 bytes to
  608 bytes. Direct app install and staged-install begin now reuse their
  existing app-file/app-directory scratch when preparing top-level app-store
  directories, reducing the source-known direct app install path from 352 bytes
  to 224 bytes, staged-install begin from 368 bytes to 240 bytes, and the top
  source-known main/protocol path from 608 bytes to 576 bytes. Resource install
  and staged-resource commit now validate the app's `main.sqbc` with a narrow
  app-file-path helper, reducing each function from 176 bytes to 160 bytes,
  the source-known `commit_resource_install` path from 368 bytes to 352 bytes,
  and the top source-known main/protocol path from 576 bytes to 560 bytes.
  Protocol dispatch now decodes only the opcode and sequence into its live
  request header because opcode handlers parse payloads from the original
  request bytes, reducing `sq_device_protocol_handle_frame` from 96 bytes to
  80 bytes and the top source-known main/protocol path from 560 bytes to 544
  bytes; after the resource parent directory change, the top source-known
  main/protocol path remains 544 bytes and is dominated by
  `storage_format -> sq_app_store_format_filesystem -> delete_files_under`.
  ESP32-C3 target storage format now unmounts the target LittleFS app-store
  partition, erases the flash area, remounts it so LittleFS formats the blank
  partition, and recreates the top-level app-store directories. The recursive
  delete walk remains as the non-target fallback, but the target
  `storage_format` path now emits 272 bytes and the top source-known
  main/protocol path is 528 bytes through transfer-begin filesystem
  preparation, with linker DRAM still at 185,024 bytes and RAM audit DRAM at
  185,008 bytes. Transfer-begin filesystem preparation now reuses the
  caller-owned staging path scratch for temp runs and staged resources,
  reducing `sq_app_store_begin_temp_run` and
  `sq_app_store_begin_staged_resource` source-known paths from 304 bytes to
  176 bytes, `begin_resource_install` to 208 bytes, and the staged
  `begin_install` path to 272 bytes. Resource install and staged-resource
  commit validation now pass the caller-owned path scratch into
  `validate_app_main_sqbc_with_path`, whose emitted C stack estimate is
  32 bytes; `commit_resource_install` is now 240 bytes and
  `sq_app_store_commit_staged_resource` is now 208 bytes. Linker DRAM remains
  185,024 bytes and RAM audit DRAM remains 185,008 bytes. The top source-known
  main/protocol path is now 496 bytes through
  `launch_app -> start_installed_app -> sq_vm_runtime_start ->
  sq_vm_runtime_start_event -> sq_vm_runtime_init`; investigate that launch
  path next. Installed-app launch now passes event bytes and length directly
  into `sq_vm_runtime_start_event`, removing the string wrapper from the launch
  path. The cumulative launch path is now 272 bytes and `start_installed_app`
  is now 192 bytes. Linker DRAM remains 185,024 bytes and RAM audit DRAM
  remains 185,008 bytes. The top source-known main/protocol path is now
  480 bytes through
  `dispatch_key -> dispatch_event_from_parts -> sq_vm_runtime_start_event ->
  sq_vm_runtime_init`; investigate the key/event dispatch path next. Key
  dispatch now decodes key events into the runtime-owned event scratch instead
  of retaining a separate `SQ_VM_RUNTIME_EVENT_LEN` stack buffer, reducing
  `dispatch_key` from 96 bytes to 80 bytes and its cumulative path to
  272 bytes. The top source-known main/protocol path is now 464 bytes through
  `begin_install -> sq_app_store_begin_staged_install ->
  prepare_filesystem_with_path -> ensure_directory`; investigate the install
  begin path next. Staged install begin now reuses the protocol transfer
  session's 80-byte `staging_path` buffer for filesystem preparation and
  app-directory creation before formatting the final temp `main.sqbc.tmp`
  path. That reduces `sq_app_store_begin_staged_install` from 112 bytes to
  48 bytes and the cumulative `begin_install` path from 272 bytes to 208
  bytes. The top source-known main/protocol path remains 464 bytes, now through
  `commit_install -> sq_app_store_scan_registry -> join_path2`; investigate
  the registry scan path next. Registry scanning now has a caller-scratch entry
  point. The public `sq_app_store_scan_registry` wrapper still owns a narrow
  64-byte path buffer, while protocol app-install commit reuses the install
  session's 80-byte `staging_path` buffer after the staged file is renamed.
  That reduces the protocol `commit_install` path from 272 bytes to 224 bytes.
  Installed-app launch now stores the file-backed VM storage backend directly
  in the runtime-owned job backend instead of keeping a protocol-stack backend
  temporary. That reduces `start_installed_app` from 96 bytes to 64 bytes and
  the cumulative launch path from 272 bytes to 240 bytes. The top source-known
  main/protocol path remains 464 bytes, now through
  `reset_runtime -> clear_runtime_context -> sq_vm_runtime_reset ->
  sq_vm_runtime_apply_target_default_indicator_binding ->
  sq_vm_runtime_device_config_rebind -> runtime_device_config_string_equals ->
  runtime_device_config_find`. Target default indicator binding still
  materializes the generated target default into the in-memory SQDC draft, but
  now applies that generated default directly instead of routing it through
  the general dynamic `sq_vm_runtime_device_config_rebind` path. That reduces
  the cumulative `reset_runtime` path from 272 bytes to 176 bytes and removes
  reset/default-indicator rebinding from the top source-known main/protocol
  path. The top source-known main/protocol path remains 464 bytes through
  `dispatch_event_request -> dispatch_event_from_parts ->
  sq_vm_runtime_start_event -> sq_vm_runtime_init`. Event dispatch now keeps
  parsed app IDs as borrowed byte slices through app storage path formatting
  instead of copying them into a protocol-stack app-id buffer. That reduces
  `dispatch_event_from_parts` from 96 bytes to 80 bytes and the top
  source-known main/protocol path from 464 bytes to 448 bytes. Event-dispatch
  protocol requests now handle their parsed app and event slices directly
  instead of adding the shared key-dispatch helper frame. That leaves key
  dispatch on `dispatch_event_from_parts`, reduces the event-dispatch
  cumulative path to 192 bytes, and moves the top source-known main/protocol
  path to 432 bytes through
  `commit_resource_install -> sq_app_store_commit_staged_resource ->
  validate_app_main_sqbc_with_path -> format_app_path`. Protocol resource
  commit now passes the caller-owned response buffer as path scratch to
  `sq_app_store_commit_staged_resource_with_path`, reducing
  `commit_resource_install` from 240 bytes to 112 bytes and moving the top
  source-known main/protocol path to 432 bytes through
  `commit_temp_run -> sq_vm_runtime_start -> sq_vm_runtime_start_event ->
  sq_vm_runtime_init`. Temp-run commit now starts `app.start` through
  `sq_vm_runtime_start_event` directly, reducing its cumulative path from
  240 bytes to 208 bytes and moving the top source-known main/protocol path to
  432 bytes through `launch_app -> start_installed_app ->
  sq_vm_runtime_start_event -> sq_vm_runtime_init`. App launch now passes the
  parsed app-id byte slice into a byte-slice installed-start helper instead of
  materializing a NUL-terminated `SQ_APP_STORE_APP_ID_MAX` protocol-stack
  buffer, reducing `launch_app` from 80 bytes to 48 bytes, its cumulative path
  from 240 bytes to 224 bytes, and the top source-known main/protocol path
  from 432 bytes to 416 bytes through
  `commit_install -> sq_app_store_scan_registry_with_path -> join_path2`.
  Install commit now updates or inserts the committed app's mutable registry
  entry directly with `sq_app_store_update_registry_entry_with_path` instead of
  rescanning the full app directory, reducing the cumulative `commit_install`
  path from 224 bytes to 208 bytes. The top source-known main/protocol path
  remains 416 bytes through `launch_app -> start_installed_app_bytes ->
  sq_vm_runtime_start_event -> sq_vm_runtime_init`. Foreground app launch now
  uses a byte-slice helper dedicated to setting the current app instead of
  routing through the generic `set_current` installed-app start path. The
  launch chain drops out of the top source-known paths, the remaining
  `launch_app` path is 112 bytes through `ok_response`, and the top
  source-known main/protocol path is 400 bytes through
  `begin_install -> sq_app_store_begin_temp_run -> prepare_filesystem_with_path -> ensure_directory`;
  transfer-begin filesystem preparation now creates only the needed target
  directory for temp runs, staged resources, and staged installs. Temp/resource
  begin paths now emit 80-byte source-known app-store paths, staged install
  begin no longer carries full filesystem preparation, and registry entry
  updates now open `main.sqbc` and use `fs_seek`/`fs_tell` for file size
  instead of carrying Zephyr's large `fs_dirent` stat buffer. That reduces
  `sq_app_store_update_registry_entry_with_path` from 144 bytes to 80 bytes
  and the source-known `commit_install` path to 160 bytes. Temp-run commit now
  stores its file-backed backend in the runtime-owned job backend, reducing
  `commit_temp_run` from 112 bytes to 80 bytes and its source-known cumulative
  path from 208 bytes to 176 bytes. The top source-known main/protocol path is
  now 384 bytes through `errors_response -> repeated_runtime_lines_response`.
  Wi-Fi scan result backing now reuses runtime transfer scratch because the
  Rust FFI copies scan results out of the C callback before returning to the
  VM. The diagnostic transfer-owner marker then guards scratch/completion/
  Wi-Fi-scan overlap and leaves `runtime.3` at 14,272 bytes, with the current
  build at 184,576 bytes linker DRAM / 184,560 bytes RAM-audit DRAM. Remaining
  resident RAM reductions should come from workload-attributed worker stack,
  system heap, or runtime-object cuts rather than stack-only path cleanups. VM
  dispatch now uses a
  static callback table plus an explicit `user_data` pointer across the FFI
  boundary, reducing `sq_vm_runtime_dispatch` from 432 bytes to 80 bytes
  without adding resident runtime RAM. Protocol frame dispatch now keeps
  opcode-specific request parsing/formatting out of the top-level switch,
  reducing `sq_device_protocol_handle_frame` from 352 bytes to 96 bytes in
  the emitted C stack report. Protocol transfer begin and commit validation
  now passes a null action output when the C handler only needs session
  validation, reducing `begin_install`, `begin_resource_install`,
  `commit_install`, and `commit_resource_install` from 96 bytes each to 32
  bytes each, and `commit_temp_run` from 144 bytes to 112 bytes; chunk handlers
  still keep a decoded action record for offsets and payload byte slices.
  Lifecycle diagnostics now encode armed timers
  directly from the runtime timer array instead of staging copied timer records
  on the C stack, reducing `lifecycle_response` from 224 bytes to 96 bytes.
  VM worker dispatch setup now keeps app-start binding preparation out of the
  worker callback frame, reducing `runtime_work_handler` from 224 bytes to 16
  bytes for steady event dispatch. App-start setup is split into separately
  attributed phases: `sq_vm_runtime_prepare_app_start` is now 16 bytes,
  saved-device-config setup is 80 bytes, and app device-binding setup is
  128 bytes, instead of retaining the combined 224-byte prepare frame.
  The fixed `/system/device-config.sqdc` path now uses a 40-byte path slot and
  direct formatting, reducing `sq_app_store_device_config_path` from 144 bytes
  to 16 bytes and `sq_vm_runtime_device_config_save` from 160 bytes to 80 bytes.
  File-backed state and device-config reads now detect oversized files with a
  one-byte overflow read instead of a `struct fs_dirent` size probe, reducing
  `fs_storage_load_state` from 192 bytes to 48 bytes and
  `runtime_device_config_read_file` from 192 bytes to 32 bytes.
  The Zephyr VM context reserve now tracks the measured 32-bit FFI context size
  of 10,392 bytes with a 10,400-byte C reserve, reducing the static runtime
  block from 15,232 bytes to 14,752 bytes.
  Runtime field ordering and byte-sized fixed-array counters now reduce the
  static runtime block further to 14,720 bytes.
  Installed-app VM launch storage path buffers now reduce `launch_storage`
  from 276 bytes to 144 bytes.
  The resident protocol response buffer now tracks the calculated current
  resources-response ceiling, reducing `response.0` from 848 bytes to 826 bytes.
  Protocol polling now reuses runtime app-id/event scratch for lifecycle and
  armed timer transitions, app-arm trigger discovery is split out of the steady
  poll frame, and trigger registration uses dedicated resident trigger storage
  so metadata reads cannot overwrite the active foreground app's file-backed
  launch storage backend. Protocol event dispatch now
  passes event bytes directly into `sq_vm_runtime_start_event` instead of
  staging a NUL-terminated event buffer, reducing `dispatch_event_from_parts`
  from 112 bytes to 96 bytes. Installed-app foreground handoff now reuses the
  runtime-owned pending-launch app-id slot as temporary rollback storage,
  reducing `start_installed_app` from 128 bytes to 96 bytes.
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
- Design lazy or reclaimable runtime string materialization for service-result
  records and lists. The current contract intentionally exposes a bounded
  12-slot per-event temporary string pool, and tests cover exact-budget and
  over-budget behavior for display info, Wi-Fi scan, app registry, file line
  reads, and repeated string concatenation. Remaining improvement work is to
  avoid consuming slots for service-result string fields that the app never
  reads, without unbounded allocation or hidden lifetime hazards.
- Design a cursor-style Wi-Fi scan API so targets can expose more scan results
  without materializing every AP record and string into one VM event. Compare
  options such as `wifi.scan()` returning a snapshot handle with
  `wifi.scan.get(scan, index)`, paged scan reads, or an iterator-like cursor,
  and keep SSID/BSSID/auth strings backed by host/runtime storage until the app
  asks for a specific network.
