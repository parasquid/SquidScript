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
  caches. Current target configuration uses a 5,120-byte protocol/main stack
  and a 17,408-byte VM worker stack. The stack harness fails with captured
  resources if
  protocol/main unused stack drops below 768 bytes or VM worker unused stack
  drops below 384 bytes. For host-side attribution before physical
  confirmation, build with `SQUID_ZEPHYR_STACK_USAGE=1` and run
  `scripts/c3-supermini-stack-usage-report.sh` to sort generated Zephyr app C
  `.su` stack-usage records. The report includes source-known cumulative call
  paths; check those before splitting more app-store or protocol helpers
  because per-function `.su` reductions can increase real stack high-water use
  when a larger callee remains active under the caller.
  Current ESP32-C3 build evidence with the real Wi-Fi scan backend enabled
  reports 195,632 bytes of linker DRAM, 195,604 bytes through
  `zephyr-ram-audit`, and a 12,264-byte `runtime.4` static runtime symbol. The
  static-buffer report classifies the top ESP32-C3 symbols as approximately
  89 KiB platform-owned, 32 KiB SquidScript-owned, and 8 KiB unknown small
  symbols. Recent SquidScript-owned reductions lowered the runtime output
  history from eight to six retained lines and the VM operand stack from 32 to
  16 values, then lowered the resident protocol response buffer from 916 to 824
  bytes by encoding resource metric values as U32 TLVs, then capped the linked
  Rust VM context reservation at 10,304 bytes after the then-current RISC-V
  `sqvm_context_size()` measured 10,284 bytes, then reduced the VM worker stack
  from 19,456 to 18,432 bytes using the existing confirmed GPIO9 press row and
  current same-build non-scan stack evidence, then lowered the configured Zephyr
  system heap from 51,200 to 45,056 bytes based on reset-bounded heap high-water
  rows, then lowered the native network packet pools from 10/10 packets and
  24/24 buffers to 6/6 packets and 16/16 buffers for the current
  low-throughput Wi-Fi scope, then capped service-result runtime records from
  12 to 8 and lowered the linked Rust VM context reservation to 8,624 bytes
  after the RISC-V `sqvm_context_size()` measured 8,604 bytes, then reduced
  the VM worker stack from 18,432 to 17,408 bytes using same-build broad
  hardware evidence. Together these save at least 13,440 bytes of linker DRAM
  and 13,440 bytes of RAM-audit DRAM
  while preserving current host, Zephyr, real Wi-Fi scan/list, and non-scan
  hardware coverage. Classify any large unknown future symbols before using
  group totals for reduction
  decisions.
  Current same-build non-scan hardware coverage on the flashed target passed
  app state, foreground memory, app lifecycle, app registry, display drawlog,
  system resources, indicator state, device binding, inline GPIO binding,
  inline GPIO10 binding, unsupported inline GPIO rejection, device config, file
  pick, stack usage, Wi-Fi status, Wi-Fi AP, and final blinky checks. Use
  `scripts/c3-supermini-test-hardware-non-scan.sh` to repeat that same-build
  RAM-confidence path; `--skip-physical-input` is allowed only for unattended
  stack/RAM coverage and does not validate the physical GPIO9 press row.
  Logical input dispatch stack coverage can use host-injected
  `device key SELECT` events; the physical GPIO9 tests validate the
  electrical/binding path that queues the same logical event. A completed
  physical GPIO9 input-stack run observed `after-press-observed` with
  `input_button_state=257`, proving one configured input and one currently
  pressed input; app output changed from `output=count 0` to `output=count 1`.
  The press row kept protocol/main stack flat at 2,476 bytes and VM worker stack
  at 17,136 bytes with 2,320 bytes free. The current targeted RAM workload
  measured protocol/main stack at 2,292 bytes used and VM worker stack peak at
  16,128 bytes used.
  The Zephyr system heap is currently capped at 45,056 bytes, below the ESP32
  Wi-Fi driver's 51,200-byte `ESP_WIFI_HEAP_SYSTEM` minimum through
  `CONFIG_HEAP_MEM_POOL_IGNORE_MIN`, because current reset-bounded workloads
  provide a tighter app-specific ceiling. Reset-bounded heap workload rows
  measured Wi-Fi AP start at `heap_max_alloc_bytes=36432` and Wi-Fi AP stop at
  `heap_max_alloc_bytes=36460`, leaving at least 8,596 bytes below the
  configured heap ceiling. The GPIO9 input stack isolation path
  refreshed current-format resource rows through `after-press-timeout`; without a
  physical press, `input_button_state=1` proves one configured input and zero
  currently pressed inputs during that run, not dispatch. The broader
  same-build non-scan stack checkpoint measured protocol/main stack at
  4,048 bytes used with 1,072 bytes free and VM worker stack at
  16,128 bytes used with 1,280 bytes free. Current
  `device resources` includes
  `heap_largest_free_supported` and `heap_largest_free_bytes`; ESP32-C3 reports
  `0/0` because the public Zephyr heap stats available in this build do not
  expose a safe non-mutating largest-free-block query. Real ESP32-C3 Zephyr
  Wi-Fi scan/list coverage now passes through the driver scan callback with
  bounded redacted AP rows; keep future Wi-Fi scan work focused on result
  pagination/cursor APIs and broader service-state modeling rather than the old
  unsupported scan path.

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

  Planned-resume checkpoint and restore use protocol-owned scratch for the
  planned-resume record, encoded bytes, file paths, and file handle instead of
  carrying those temporaries on the protocol/main stack. Keep validating the
  current protocol/main and worker stack budgets with the hardware suite before
  attempting another stack reduction. Current stack-report evidence after that
  migration shows `sq_device_protocol_restore_planned_resume ->
  register_app_triggers -> register_app_trigger_timer ->
  sq_vm_runtime_register_armed_timer` at 240 cumulative bytes and
  `sq_device_protocol_poll -> register_app_triggers ->
  register_app_trigger_timer -> sq_vm_runtime_register_armed_timer` at
  272 cumulative bytes.
- Add a firmware lockup triage pass for ESP32-C3 hardware work. When flashing
  succeeds but serial commands stall, app launch hangs, or input dispatch stops
  responding, check stack exhaustion early with `device resources`, compare
  protocol/main and VM worker stack used/unused values, and inspect recent FFI,
  metadata parsing, storage, and service paths for hidden stack temporaries
  before treating GPIO, flashing, or serial as the primary failure. Hardware
  scripts now use the shared bounded command helper, which prints captured
  command output when a command fails or times out.
- Convert blocking Wi-Fi VM callbacks to nonblocking runtime progress. Current
  Zephyr `wifi.connect`, `wifi.disconnect`, and `wifi.scan` callbacks wait on
  semaphores for up to 15s, 5s, and 8s respectively. They run in the VM worker
  rather than the serial main loop, but they still block app/runtime progress
  and conflict with the firmware rule that services should use short steps,
  async progress, or target scheduler integration.
- Continue SquidScript-owned static DRAM reductions using the classified
  static-buffer report. Current same-build evidence does not justify lowering
  the 17,408-byte VM worker stack further: the broad non-scan suite leaves
  1,280 bytes free, while the stack harness requires at least 384 bytes and the
  older physical GPIO9 input isolation row peaked at 17,136 bytes on the then
  current image. Lower it only after a same-build input-button or equivalent
  logical-input fixture proves the physical/input app path stays below the new
  candidate budget. Keep the 824-byte protocol response buffer until resources
  output is redesigned again, because it is sized to the current largest
  response. Treat `runtime.4` quota cuts as test-first changes: reduce VM
  records, record fields, dynamic string slots, trace/output/drawlog slots, or
  lifecycle/input arrays only when compiler/runtime fixtures and hardware apps
  show the smaller quota still covers current behavior. Replacing resident
  Wi-Fi scan result storage with a cursor-backed API remains a separate design
  item if Wi-Fi scan pagination moves forward. Keep Zephyr kernel stacks,
  system heap, network packet pools, Wi-Fi driver storage, and other platform
  symbols separate unless platform RAM policy is explicitly in scope.
- Add a safe largest-free-block heap probe or mitigation path for ESP32-C3
  Zephyr RAM work. `device resources` now reports allocation high-water data and
  largest-free-block support/value fields, and workload scripts can reset the
  allocation high-water mark at measurement boundaries. The current public
  Zephyr heap stats API only returns free, allocated, and max-allocated bytes;
  `sys_heap_print_info()` prints bucket details but does not return bounded
  numeric telemetry, and heap listeners report allocation/free events rather
  than the current largest free block. Continue with a target-safe probe,
  subsystem allocation-failure attribution, or heap design mitigation such as
  fixed arenas, caller-owned buffers, bounded scratch, slabs/pools for
  unavoidable dynamic allocations, and startup-owned long-lived allocations
  instead of mixed-lifetime heap usage.
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
  and keep SSID/auth strings backed by host/runtime storage until the app asks
  for a specific network. Preserve the current rule that raw BSSID/MAC values
  are not exposed to SquidScript.
