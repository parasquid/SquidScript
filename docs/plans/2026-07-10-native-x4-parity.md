# Native X4 Firmware Parity Implementation Plan

> This plan is written for an agent starting with no conversation context.
> Read the repository `AGENTS.md`, then `AGENTS.local.md`, then the design spec
> at `docs/specs/2026-07-10-native-x4-parity-design.md` before executing any
> task. Do not use subagents. Update `.current_agent_work` and the active todo
> tracker before investigation, edits, tests, or hardware access.

**Goal:** Complete native XTEINK X4 product behavior required to remove Zephyr
without porting the XIAO or ESP32-C3 Super Mini firmware targets.

**Architecture:** Native Rust firmware owns the complete X4 product. Installed
apps, state, lifecycle checkpoints, and OTA metadata live in internal flash;
books and general content remain on SD. Physical inputs feed a bounded generic
event router. Armed timer/input triggers launch fresh foreground app sessions.
SquidScript apps own all gesture behavior. Serial OTA writes the inactive stock
bootloader slot and relies on health confirmation plus rollback.

**Primary design source:**
`docs/specs/2026-07-10-native-x4-parity-design.md`.

**Current implementation baseline:**

- Branch `native-firmware-x4` contains unified HTTP/BLE upload through commit
  `a2e35e3` and later work.
- Native X4 currently passes compiler, VM, firmware-core, X4, protocol, CLI,
  BLE transfer, resumable HTTP transfer, BinBook render, and live display gates.
- Native runtime implements display, RAM-backed installed-app execution/state,
  foreground timers, file/BinBook services, Wi-Fi, and unified upload.
- It does not yet implement persistent multi-app registry/lifecycle, armed
  input triggers, physical X4 buttons, planned deep sleep, serial OTA, or
  reliable BLE terminal status.
- Runtime `device.config.*` is still public but is not part of the intended X4
  product and must be removed directly.
- `ROADMAP.md` has an approved uncommitted correction to its native parity
  entry at the time this plan was written. Preserve it.
- Zephyr is being replaced. Do not spend time keeping Zephyr code, wrappers,
  tests, generated files, or docs working during these slices.

## Global Execution Rules

- Use TDD for compiler, VM, runtime, protocol, storage, input, lifecycle, and
  OTA behavior: add the narrow failing test before implementation.
- Commit after each independently verified slice. Never batch two hardware
  changes before measuring the first.
- Run hardware-owning commands sequentially. `/dev/ttyACM0` is normally the
  X4 and `/dev/ttyACM1` may be a second ESP32-C3 peer, but probe rather than
  assuming.
- Redact SSIDs, credentials, MAC addresses, and local IP details in chat and
  commits. Generated test credentials must remain untracked.
- Preserve user working-tree changes. Do not restore, reset, stash, or erase
  them.
- Keep debug timing/routing logs enabled through `debug_assertions`; release
  builds compile them out.
- Update related current-state docs in each slice. Do not leave documentation
  cleanup until the final Zephyr deletion.
- Remove completed roadmap entries as their work lands. Propose any new
  roadmap wording to the user before adding it unless this plan already records
  explicit approval.

## Standard Verification Commands

Run the smallest owning tests during TDD, then the relevant bundle before each
commit:

```bash
RUSTUP_TOOLCHAIN=stable cargo test -p squidc-core
RUSTUP_TOOLCHAIN=stable cargo test -p squidvm-core
RUSTUP_TOOLCHAIN=stable cargo test -p squid-device-protocol
RUSTUP_TOOLCHAIN=stable cargo test -p squidscript-fw-core
RUSTUP_TOOLCHAIN=stable cargo test -p squidscript-fw-x4 --features x4-binbook
RUSTUP_TOOLCHAIN=stable cargo test -p squidc --bin squidc
RUSTUP_TOOLCHAIN=stable cargo run -p squidc -- target build --target xteink-x4
```

The repo-wide formatter currently may expose unrelated committed formatting
drift. Run focused format checks and `git diff --check`; do not rewrite
unrelated files merely to silence existing drift.

---

## Task 1: Freeze Baseline And Recovery Evidence

**Purpose:** Establish a recoverable hardware and test baseline before changing
the partition table or storage ownership.

- [ ] Update `.current_agent_work` with the exact task, current commit, attached
  ports, and next recovery step.
- [ ] Probe serial ports, Bluetooth controller, Wi-Fi interface, webcam, flash
  size, and X4 identity without publishing identifiers.
- [ ] Run the standard native verification commands and record pass/fail counts
  in `/tmp/native-x4-parity-baseline.md`.
- [ ] Run existing X4 serial, BLE, HTTP, reader, grid-cursor, and SD gates.
- [ ] Capture fresh `device resources`, `device lifecycle`, and `device errors`
  from an idle reset.
- [ ] Capture a fresh webcam frame and verify the current panel state.
- [ ] Back up the complete 16 MiB X4 flash with
  `scripts/x4-firmware-backup.sh` or `espflash read-flash`; store it only under
  the gitignored hardware-test directory.
- [ ] Read and save the current bootloader and partition table separately.
- [ ] Prove the backup/recovery instructions by generating the exact commands
  needed to restore bootloader, partition table, and application. Do not erase
  the live board in this task.
- [ ] Confirm the current table is the factory layout and the chip reports
  16 MiB flash.

**Acceptance:** Existing native tests and hardware gates pass, a complete
untracked flash backup exists, and recovery commands are documented in the
handoff.

**Commit:** No tracked commit unless recovery documentation needs a current,
non-local repository update.

---

## Task 2: Lock Contracts, Target Schema, And Runtime Caps

**Owning layers:** language/compiler, SQBC metadata, target schema, docs.

### 2A. Remove runtime device configuration

- [x] Add compiler negative/unknown-API tests showing
  `device.config.load/set/rebind/save` follow the ordinary unsupported symbol
  path.
- [x] Remove their AST/IR lowering, builtin IDs, VM host methods, result records,
  fixtures, examples, and current documentation.
- [x] Remove generated ABI entries when they are derived from the manifest.
  Do not repair the now-obsolete Zephyr implementation.
- [x] Do not retain aliases, migration diagnostics, old-name regression tests,
  or compatibility readers.

### 2B. Add `service.input.on`

- [x] Add compiler tests for:
  - valid use only inside `app.triggers`;
  - a required static string;
  - `key.<logical>`, `key.<logical>.longTap`, and
    `key.<logical>.doubleTap` shapes;
  - the portable event-name byte limit;
  - a required matching `event.on` handler;
  - duplicate declaration rejection;
  - rejection in normal handlers, screens, functions, and top-level code.
- [x] Add a typed input-trigger record to IR and the SQBC trigger section.
  Replace directly; do not add an SQBC version or compatibility path.
- [x] Extend `ProgramIndex` reader tests so firmware can enumerate timer and
  input trigger records without loading the full app.
- [x] Keep timer trigger records unchanged except where a generic trigger-kind
  field removes duplication cleanly.

### 2C. Make target input policy authoritative

- [x] Update target schema/reference documentation for independent
  `gestureTiming.longTapMs` and `gestureTiming.doubleTapWindowMs` fields.
- [x] Add per-button `gestures` validation accepting `longTap` and
  `doubleTap` only.
- [x] Set X4 values to 350 ms and enable both gestures only on POWER.
- [x] Remove the target-owned POWER sleep action and POWER+DOWN chord. Firmware
  must not assign actions to gesture events.
- [x] Generate Rust constants from target JSON for logical names, GPIO/ADC
  mappings, thresholds, debounce, gestures, and timing.

### 2D. Move parity caps out of Zephyr ownership

- [x] Define native/platform-neutral sources of truth for:
  - 8 installed apps;
  - 2 process-stack entries;
  - 2 armed timer slots;
  - 8 armed input registrations;
  - 8 pending input/timer events;
  - existing event-name, app-id, state, and protocol limits.
- [x] Update `docs/runtime_limits.md` to identify the new source rather than C
  macros.
- [x] Generate or import constants rather than hand-copying values between
  compiler, protocol, and firmware.

**Acceptance:** Compiler and VM tests prove the new trigger metadata and direct
device-config removal; target validation generates the exact X4 Rust constants;
all native builds pass.

**Commit:** `feat: add declarative armed input triggers`

---

## Task 3: Add The OTA-Compatible Partition Table

**Owning layers:** target metadata, target build/flash tooling, bootloader
configuration.

- [x] Add a tracked ESP-IDF CSV with the exact design-spec geometry:

  ```text
  nvs,data,nvs,0x9000,0x5000
  otadata,data,ota,0xe000,0x2000
  app0,app,ota_0,0x10000,0x280000
  app1,app,ota_1,0x290000,0x280000
  squidscript,data,littlefs,0x510000,0xae0000
  coredump,data,coredump,0xff0000,0x10000
  ```

- [x] Add table validation tests for alignment, overlap, total 16 MiB bounds,
  equal OTA slots, app image size, and required labels/subtypes.
- [x] Reference the table from X4 target metadata and make `target inspect`
  report the resolved table and OTA image path.
- [x] Make `target flash` pass the table to `espflash` without exposing a
  backend selector.
- [x] Generate a raw OTA-compatible application image from the ELF during
  `target build` and fail if it exceeds `0x280000`.
- [x] Keep bootloader, table, ELF, and raw image artifacts distinct in printed
  plans and JSON output.
- [x] Flash the table and current native image only after the Task 1 backup is
  confirmed.
- [x] Verify boot, serial protocol, display, SD, Wi-Fi, and BLE before
  continuing. Do not start LittleFS work if repartitioned boot is unstable.

**Acceptance:** X4 boots native firmware from `app0`; readback of the live
partition table exactly matches the tracked CSV; existing hardware gates still
pass.

**Commit:** `feat: add native x4 ota partition layout`

---

## Task 4: Prove The Internal Flash Storage Stack

**Owning layer:** X4 hardware storage adapter. This is a mandatory throwaway
spike before app-store design is committed.

- [x] Add `esp-storage 0.9` and `littlefs2 0.8` behind a focused X4 feature.
- [x] Implement only a temporary partition-bounded block adapter and a serial
  diagnostic command/harness.
- [x] Prove on the real X4:
  - invalid/blank filesystem detection;
  - explicit format;
  - mount and remount;
  - create/write/flush/read;
  - atomic rename;
  - deletion;
  - capacity/no-space reporting;
  - recovery after reset between temporary write and rename;
  - writes cannot cross the `squidscript` partition.
- [x] Measure static RAM, heap, stack high water, erase latency, write latency,
  and serial responsiveness.
- [x] Delete the diagnostic command and spike-only code after recording the
  result in `/tmp` and `.current_agent_work`.

**Blocking rule:** If the stack cannot pass without an unbounded allocator,
cross-partition risk, or unacceptable main-loop blocking, stop and present the
evidence. Do not invent a custom filesystem or silently switch libraries.

**Acceptance:** The retained production adapter has focused host tests and the
hardware spike proves the selected stack. Existing SD/display behavior remains
unchanged.

**Commit:** `feat: add bounded native x4 flash filesystem`

---

## Task 5: Implement The Persistent Native App Store

**Owning layers:** portable firmware-core app-store interface and X4 LittleFS
adapter.

- [x] Add a storage interface supporting bounded directory scan, byte-range
  read, temporary write, flush, rename, delete, state records, and capacity
  reporting.
- [x] Implement the logical paths from the design spec. Keep physical paths
  out of compiler core and app-visible results.
- [x] Rebuild an eight-entry resident registry from valid app directories at
  boot. Reject overflow visibly rather than truncating.
- [x] Stream SQBC reads from flash through the existing reader/chunk boundary;
  do not retain an app-sized bytecode array.
- [x] Implement atomic app installation:
  - begin a temporary file;
  - enforce app-id and size caps;
  - accept durable sequential chunks;
  - verify byte count, CRC/SHA as selected by the existing install protocol,
    app-id metadata, and SQBC structure;
  - publish resources and `main.sqbc` atomically;
  - leave the previous installed app intact on any failure.
- [x] Persist package resources below the app directory and prove an installed
  app can read one after cold boot.
- [x] Persist per-app state through atomic temporary-record replacement.
- [x] Keep `RUN.TEMP` and its state RAM-backed. Reset/replacement must reclaim
  it without flash writes.
- [x] Compile and embed a native fallback SquidScript app. Boot installed
  `main` when valid; otherwise run fallback `main` without publishing it in the
  registry.
- [x] Implement `system.storage("apps")` from real partition metrics and
  `system.memory()` from native RAM/heap metrics.
- [x] Make `device storage-format` erase only SquidScript filesystem content,
  remount it, clear registry/state/lifecycle data, and preserve OTA slots and SD
  books.

**Automated tests:** blank store, format, valid/invalid scans, eight/nine app
bounds, atomic replacement failure, resource publication, state persistence,
fallback selection, temp-run no-write sentinel, and format isolation.

**Hardware gate:** install at least three apps and a package resource, cold
reset, list them, launch each, verify state across resets, corrupt one temp
install, prove the prior app still runs, and verify SD books are untouched by
app-store format.

**Acceptance:** No installed-app execution depends on the RAM installed slot or
Zephyr storage. Cold boot and explicit state persistence work from internal
flash.

**Commit:** `feat: add persistent native x4 app store`

---

## Task 5A: Add Internal Content Fallback And Long ASCII Names

**Owning layers:** native file-storage abstraction, X4 LittleFS/FAT adapters,
device protocol, host upload validation, and BinBook enumeration.

- [x] Define a 121-byte ASCII content-name limit from the portable 128-byte
  path budget and logical `books/` prefix.
- [x] Raise serial, BLE, runtime, FAT, and BinBook enumeration buffers that
  previously truncated valid names at 64 bytes.
- [x] Share one mounted LittleFS owner between app storage and internal content
  handles; do not create independent mounts over the flash partition.
- [x] Add an SD-first logical content router with internal fallback on missing
  SD, per-upload volume pinning, read fallback, merged enumeration, and
  duplicate-name precedence.
- [x] Add host tests for 121-byte success, 122-byte and non-ASCII rejection,
  FAT round-trip, shared LittleFS app/content coexistence, missing-SD upload,
  and SD-preferred upload.
- [ ] On hardware without SD, upload, check, list, open, and delete a valid
  long-named BinBook; cold reset and prove it persists.
- [ ] On hardware with SD, prove new uploads use SD, internal-only content stays
  readable, duplicate names resolve to SD, and formatting internal storage
  preserves SD content.

**Acceptance:** The same logical references and long ASCII names work on both
volumes. Missing SD does not prevent BinBook upload or reading, and no physical
filesystem path or backend selector reaches SquidScript.

**Commit:** `feat: add native internal content fallback`

---

## Task 6: Implement Full Foreground And Armed Lifecycle

**Owning layer:** native firmware-core lifecycle state machine using the app
store interface.

- [x] Port behavior, not C structure, from
  `docs/app_lifecycle_state_machine.md` into a Rust state machine.
- [x] Implement host callbacks for registry list/get, process stack, armed
  stack/get, arm, disarm, launch, and start reason.
- [x] Implement a two-entry return stack with explicit overflow errors.
- [x] Dispatch `app.exit`, then start launch/return targets fresh with
  `launch`/`return` reasons.
- [x] Read trigger metadata from installed SQBC through a separate bounded
  reader so arming cannot disturb the active foreground reader.
- [x] Register up to two armed timers and eight armed logical-input triggers.
- [x] Enforce one armed owner per input event. A duplicate arm fails, retains
  the old owner, records a device diagnostic, and emits a debug-build routing
  log.
- [x] Add an eight-entry pending event queue shared by timer/input producers.
  Use drop-newest overflow, retain an overflow diagnostic, and preserve queued
  order.
- [x] Drain only while lifecycle and VM dispatch are ready. Never invoke a
  second VM job reentrantly.
- [x] Route matched armed input/timer events by pushing the current app,
  launching the owner fresh, and dispatching the exact declared event. Do not
  also send it to the previous foreground app.
- [x] Route unmatched input/timer events to the current foreground session.
- [x] Update lifecycle/resources protocol output with real active, return,
  armed, queue, and phase state.

**Automated tests:** launch without/with foreground, exit return, missing app,
stack overflow, arm/disarm, duplicate ownership, armed timer due, armed input
match, unmatched foreground input, busy-VM queueing, overflow, app replacement,
and reset cleanup.

**Hardware gate:** install a root app, reader app, armed timer app, and armed
input app. Verify launch/return, registry APIs, timer launch, serial-injected
input launch, state continuity, lifecycle diagnostics, and clean reset.

**Acceptance:** All app registry/lifecycle language APIs work on native X4 and
no lifecycle operation depends on Zephyr.

**Commit:** `feat: add native app registry and armed lifecycle`

---

## Task 7: Implement Physical X4 Input And Gesture Classification

**Owning layers:** pure firmware-core classifier, X4 ESP-HAL sampling task,
lifecycle event queue.

- [x] Add pure tests for ADC range boundaries, no-button regions, active-low
  POWER, debounce, press/release, long threshold, pending short timeout,
  double second-press recognition, and gesture-disabled buttons.
- [x] Implement a nonblocking Embassy input task:
  - use ADC1 one-shot reads for GPIO1 and GPIO2;
  - use active-low pull-up input for GPIO3;
  - sample at a bounded interval compatible with target debounce;
  - perform no display, storage, VM, or serial work while owning ADC/GPIO;
  - enqueue logical transitions through the shared runtime boundary.
- [x] Ordinary ADC buttons emit UP, DOWN, LEFT, RIGHT, SELECT, and BACK on
  debounced release.
- [x] POWER implements the exact target-configured 350/350 ms semantics from
  the design spec.
- [x] Emit no automatic sleep or force-refresh action.
- [x] Add debug logs for raw redacted ADC bucket, debounced logical state,
  classification, and final route. Do not log environment identifiers.
- [x] Update `tests/hardware/xteink-x4/key-detector` to assert ordinary,
  `.longTap`, and `.doubleTap` events.
- [x] Add armed input example apps using `service.input.on`:
  - double tap launches a helper that exits, causing the previous app to start
    and redraw normally;
  - long tap launches a helper that requests planned sleep after Task 8 lands.

**Physical hardware gate:** With the real X4, press all six ADC keys and POWER,
hold POWER beyond 350 ms, perform a POWER double tap, and deliberately wait out
a single POWER tap. Verify exact event counts, suppression, ordering, armed
launch behavior, lifecycle logs, and no serial starvation. Capture fresh panel
evidence for the launch/return redraw example.

**Acceptance:** Physical X4 input, not serial injection, drives portable
foreground and armed events with target-owned timing.

**Commit:** `feat: add native x4 physical input gestures`

---

## Task 8: Implement Planned Sleep, Timer Wake, And POWER Wake

**Owning layers:** native lifecycle request, app-store checkpoint, X4 RTC power
driver.

- [x] Add a `NativePowerBackend` boundary so the VM records a pending sleep
  request and the outer firmware task owns hardware sleep entry.
- [x] Implement `service.power.sleep` validation and reject temp foreground
  apps.
- [x] After the requesting event completes, dispatch `power.sleep`; abort sleep
  if the handler or checkpoint fails.
- [x] Persist a bounded CRC-protected checkpoint containing active installed app,
  return stack, armed app ids, and requested wake duration. Do not add storage
  compatibility readers.
- [x] Flush app state/filesystem, pending display work, and serial responses;
  stop upload services and release radios before sleep.
- [x] Configure ESP-HAL deep sleep with POWER/GPIO3 wake and timer wake when
  requested.
- [x] On boot, inspect wake cause, validate/consume the checkpoint, rebuild
  registry and armed metadata, and start the saved app with reason `wake`.
- [x] Restore return behavior but not VM frames, screens, foreground timers,
  handles, services, or temp apps.
- [x] Fall back to normal root boot on corrupt/missing app/checkpoint and retain
  a diagnostic.
- [x] Make `system.startReason()` return `boot`, `launch`, `return`, or `wake`
  from real lifecycle state.

**Automated tests:** deferred request, cleanup handler, checkpoint codec/CRC,
temp rejection, flush failure, radio/display teardown ordering, timer wake
restore, POWER wake restore, missing app fallback, corrupt checkpoint, and
return stack after wake.

**Hardware gates:**

- [x] Install an app that saves state in `power.sleep`, request a three-second
   timer wake, observe USB loss/reconnect, and verify `wake`, state, panel, and
   armed registrations.
- [ ] Enter sleep without a timer, wake with physical POWER, and verify the same
   restoration contract.
- [ ] Arm the long-tap helper, launch it with physical POWER long tap, let the app
   request sleep, and verify firmware itself assigned no action before the app
   ran.

**Acceptance:** Planned sleep is app-requested, physically real, recoverable,
and lifecycle-correct on both wake sources.

**Commit:** `feat: add native x4 planned sleep and wake`

---

## Task 9: Implement Serial OTA With Health Confirmation And Rollback

**Owning layers:** device protocol codec/session, X4 inactive-slot writer,
`squidc` CLI.

- [x] Add heap-free firmware-update protocol messages for info,
  begin/chunk/commit/status/abort. Include active slot, slot sizes, build id,
  durable offset, and terminal status.
- [x] Add protocol tests for framing, bounds, offset/order errors, retry,
  abort, reconnect status, and response capacity.
- [x] Extend target build to produce and inspect the raw ESP app image.
- [x] Implement host validation using established ESP image parsing where
  available; do not hand-parse with ad hoc string/byte scanning.
- [x] Add `device firmware-info` and `device firmware-update` with progress,
  throughput, ETA, durable retry, and explicit image/slot/build reporting.
- [x] On firmware:
  - identify the inactive `0x280000` OTA slot from the partition table;
  - reject active-slot writes and oversized images;
  - erase/write in bounded sector steps with serial acknowledgements;
  - compute SHA-256 while writing and verify expected length/hash;
  - read back the written region before activation;
  - leave OTA metadata unchanged on every failure;
  - mark the candidate pending and activate it only after successful commit.
- [x] Reboot after the final response is durably sent.
- [x] At new-image boot, mark the slot valid only after LittleFS mount,
  registry scan, runtime init, and serial readiness.
- [x] Let the target-owned bootloader roll back a candidate that resets or fails before
  the health gate.
- [x] Preserve app-store LittleFS and SD content across slot changes.

**Hardware gates:** valid app0-to-app1 update, valid app1-to-app0 update,
corrupt/truncated image rejected before activation, interrupted serial transfer
resumed/aborted safely, forced pre-health failure rolls back, build/slot identity
verified after reconnect, and recovery flash path remains usable.

Hardware verification covered both slot directions at approximately 13.2 KiB/s,
post-boot `valid` health state, host-side truncated-image rejection without slot
activation, and an interrupted transfer resumed from its durable checkpoint after
reconnect. A forced pre-health reset marked the candidate `aborted` and returned
to the prior build. Erasing OTA metadata and using the normal target flash path
recovered the device without erasing LittleFS or SD content.

**Acceptance:** A trusted serial client can update either inactive slot without
bootloader mode, corrupt images never activate, and a non-healthy candidate
automatically returns to the prior image.

**Commit:** `feat: add native x4 serial ota updates`

---

## Task 10: Prove Wi-Fi/BLE Coexistence And Terminal Status

**Owning layers:** existing `esp-radio`/TrouBLE integration, BLE status
characteristic, CLI client, target hardware tests.

- [x] Start with a throwaway hardware spike using the actual host and, where
  useful, the second attached ESP32-C3. Answer whether AP association, HTTP,
  BLE advertising, a live GATT connection, and data transfer coexist with the
  current `esp-radio/coex` feature.
- [x] Save temporary evidence outside tracked docs; remove spike code before
  the implementation commit.
- [x] If coexistence fails, instrument and fix or upgrade the owning
  `esp-radio`/TrouBLE boundary. Do not serialize the services behind the API or
  claim lease-level tests prove over-the-air coexistence.
- [x] Build one app with a unified profile enabling both `http` and `ble`.
- [x] Restore a real terminal BLE status that BlueZ/btleplug can observe after
  durable commit and handler completion. Preserve one adapter for the entire
  operation and bounded characteristic-resolution retries.
- [x] Make CLI success require terminal status; serial CRC remains independent
  content-integrity proof, not a completion substitute.
- [x] Test stop/reset/app replacement/runtime error cleanup for both radios and
  staged files.

**Hardware stress gate:** associate host to X4 AP while BLE advertises, keep a
BLE connection open while HTTP uploads, issue repeated HTTP `HEAD` requests
during BLE upload, complete both transfers with exact CRC, observe terminal
GATT status, then stop/reset and verify zero leases and reusable memory.

Hardware verification used the unified `file-transfer-regression` profile.
GATT remained connected through HTTP upload, the host completed 79 HTTP `HEAD`
requests during BLE upload, and the CLI observed terminal BLE completion after
the app handler. Both 8,982-byte files matched CRC32 `5290be40`. Runtime reset
released both radio leases and the upload profile, and recovered reusable heap.
The spike isolated the initial timeout to SD promotion rather than RF
coexistence; target-frequency SD traffic and the single-mount copy path removed
that bottleneck.

**Acceptance:** HTTP and BLE are simultaneously usable over the air and the
unified CLI receives authoritative completion for both.

**Commit:** `fix: complete native x4 radio coexistence`

---

## Task 11: Close Remaining X4 Contract Gaps

- [x] Add GRAY1 BinBook degradation/expansion into the supported streaming
  render path. Test current compiler output through host validation and live X4
  rendering.
- [x] Audit X4 target metadata against native hardware evidence. Narrow any
  unimplemented display, refresh, storage, input, power, or radio claim rather
  than preserving aspirational metadata as fact.
- [x] Keep display optimization, file-management expansion, reading history,
  metadata caching, and throughput tuning as separate roadmap work.
- [x] Promote native X4 app-store, lifecycle, input, sleep, OTA, serial, HTTP,
  BLE, SD, reader, and display gates into the target-aware
  `squidc hardware test --target xteink-x4` inventory.
- [x] Ensure target-aware runs select only X4-native checks and never invoke
  Zephyr setup, west, Twister, Kconfig, FFI ABI generation, XIAO, or Super Mini
  wrappers.
- [x] Update current docs as native facts. Remove current-state claims that
  Zephyr is canonical or that target commands select a backend.

**Acceptance:** Target metadata is honest, GRAY1 content renders, and one
target-aware command selects every parity hardware gate.

**Commit:** `test: make native x4 parity target-aware`

---

## Task 12: Final Parity Audit And Zephyr-Removal Handoff

- [ ] Run the complete standard verification bundle from a clean build.
- [ ] Run all target-aware X4 hardware tests sequentially on the final image.
- [ ] Verify an idle final state: no active app operation, upload, staged file,
  pending event, radio lease, display flush, or device error.
- [ ] Repeat cold boot with and without SD inserted. Internal apps/lifecycle
  must remain available; SD-dependent content must fail honestly.
- [ ] Repeat storage-format and prove OTA slots plus SD content are preserved.
- [ ] Repeat a serial OTA after all parity features are enabled so final image
  RAM/stack/storage behavior is covered.
- [ ] Capture fresh panel evidence after input-trigger launch, wake restore,
  GRAY1 render, HTTP upload, and BLE upload where the visible result differs.
- [ ] Search current native code, targets, docs, examples, tests, and scripts for
  dependencies on Zephyr implementation paths or backend selectors.
- [ ] Remove completed parity roadmap entries. Keep the separately approved
  authenticated network OTA delivery item.
- [ ] Write a new implementation plan dedicated to deleting:
  - `firmware/zephyr`;
  - `squidvm-ffi` and generated C ABI glue if no non-Zephyr owner remains;
  - XIAO/Super Mini target definitions and generated docs;
  - west/Twister/Kconfig scripts and dependencies;
  - Zephyr-only tests, fixtures, build directories, and docs;
  - internal CLI backend planning types that no longer have multiple values.

**Final acceptance matrix:**

| Capability | Required evidence |
| --- | --- |
| Build/flash | selector-free X4 build, recovery flash, OTA image |
| App store | eight-app bound, resources, state, atomic failure, cold boot |
| Lifecycle | launch, exit/return, registry, arm/disarm, timer/input activation |
| Input | six ADC keys, POWER short/long/double, queue/ownership diagnostics |
| Power | app-requested sleep, timer wake, POWER wake, checkpoint restoration |
| OTA | both directions, integrity rejection, interruption, rollback, health |
| Display | primitives, GRAY1/GRAY2 BinBook, fast/full paths, live panel |
| Storage | internal app LittleFS, SD content, format isolation, missing SD |
| Radio | AP, station, scan, simultaneous Wi-Fi/BLE, terminal BLE status |
| Transfers | serial, resumable HTTP, BLE, exact size/CRC, cleanup |
| Tooling/docs | target-aware native tests, no current Zephyr dependency |

**Push gate:** Commit and push only after every required automated and hardware
row is green or the user explicitly changes the product contract. Do not mark
the parity goal complete merely because Zephyr deletion has its own later plan.

**Commit:** `docs: declare native x4 ready for zephyr removal`
