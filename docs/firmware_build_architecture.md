# SquidScript Firmware Build Architecture

Status: Current direction
Purpose: Define the real-firmware build model for SquidScript targets.

## Current Backend

Zephyr is the only supported real-firmware backend. The primary firmware app is
`firmware/zephyr`, built with Zephyr CMake/Kconfig through `west`.

Rust remains part of the firmware architecture only at the VM boundary:

- `squidc-core`, `squidvm-core`, and SQBC tooling remain Rust.
- `squidvm-ffi` exposes VM behavior through a small C ABI/staticlib boundary.
- Zephyr owns the real firmware host layer, scheduler integration, drivers,
  storage, serial/shell, logging, networking, Wi-Fi, power, and diagnostics.

There is no fallback Rust ESP firmware path and no compatibility shim for the
old serial protocol or flash layout. If behavior fails on ESP32-C3 under
Zephyr, treat it as a Zephyr implementation, driver, configuration, or
workaround task.

The ESP32-C3 Super Mini firmware embeds a target-specific fallback SquidScript
app from `firmware/zephyr/fallback/esp32c3-supermini-main.squid`. CMake compiles
that source with `squidc build`, converts the resulting SQBC into generated C,
and links it as a read-only fallback storage backend. Boot policy selects this
fallback as logical `main` only when the app store mounted, the registry scan
succeeded, and no installed `main` exists. Installed `main` always takes
precedence, and app-store failures remain warnings/diagnostics rather than
being hidden by fallback launch.

## Repository Interfaces

From the repository root:

```sh
scripts/zephyr-setup.sh
scripts/zephyr-test-protocol.sh
scripts/c3-supermini-build.sh
scripts/c3-supermini-flash.sh
scripts/c3-supermini-zephyr-monitor.sh
```

The build and flash wrappers delegate to Zephyr-specific scripts and source
`scripts/zephyr-env.sh`. By default, `SQUID_ZEPHYR_HOME` is
`target/zephyr`, with `west` installed in `target/zephyr/venv` and the Zephyr
workspace in `target/zephyr/workspace`.

`scripts/zephyr-setup.sh` prepares that local tooling area. It may install
generic host tools with Homebrew (`cmake`, `ninja`, `dtc`, `wget`, and `xz`),
creates the Python venv, installs `west`, initializes and updates the Zephyr
workspace from `firmware/zephyr/west.yml`, installs Zephyr's base/build-test
Python requirements plus `firmware/zephyr/requirements-twister.txt`, and runs
`west blobs fetch hal_espressif` for Espressif RF blob support. If no SDK is
detected, it runs Zephyr's supported `west sdk install` flow for the
`riscv64-zephyr-elf` GNU toolchain under `target/zephyr/sdk`; pass
`--skip-sdk` to leave SDK installation manual. The setup path does not use
`rpm-ostree`.

`ZEPHYR_BOARD` selects the board. The ESP32-C3 Super Mini wrappers default to
Zephyr's `esp32c3_supermini` board target. Override `ZEPHYR_BOARD` when testing
a different ESP32-C3 board variant.

## Rust VM Static Library

`firmware/zephyr/cmake/squidvm_ffi.cmake` builds and links `squidvm-ffi` into
Zephyr targets. Native simulator builds use the host Rust target so ztests can
exercise the C header and linker path. ESP32-C3 builds use
`riscv32imc-unknown-none-elf` with the `squidvm-ffi` `zephyr` feature, which
compiles the FFI crate as `no_std` with a firmware panic handler.

The helper resolves `cargo` and `rustc` through the stable `rustup` toolchain
instead of relying on whatever Rust binaries appear first on `PATH`. This is
required on hosts where Homebrew Rust is present but the rustup target library
for `riscv32imc-unknown-none-elf` is installed under the rustup toolchain.

Current Zephyr ztest coverage proves the firmware can include
`src/squidvm_ffi.h`, link the Rust static library, call context metadata ABI
functions, and link the resumable dispatch/storage request entry points. The
Rust FFI tests exercise resumable dispatch through the C ABI for SQBC chunk
reads, state load, state save, completion, and trace ordering. Full VM hosting
now has a Zephyr-side storage adapter that maps FFI storage requests to
backend callbacks for SQBC reads and app state load/save/reset. Native Zephyr
ztests initialize the linked Rust VM through `squidvm_ffi.h`, run a real SQBC
fixture with resumable dispatch, complete its storage requests through the
adapter, and verify trace callback ordering. Zephyr also has a file-backed
storage backend that uses `fs_*` APIs for SQBC byte-range reads and app-state
load/save/reset paths; native ztests cover it through a host-mounted filesystem.
The Zephyr VM runtime scratch buffer is sized to the FFI storage transfer
capacity so the firmware does not reserve a full max-app buffer when the VM
only needs one bounded code/storage chunk for resumable dispatch.

The VM host ABI C header and inventory are generated and checked by
`scripts/check-squidvm-ffi-abi.py`. The manifest at
`compiler/rust/crates/squidvm-ffi/abi/manifest.json` describes the current
`sqvm_`, `sqdp_`, and `sqdc_` exports, `SqvmCallbacks` fields, public ABI
types, constants, callback coverage expectations, and the C definitions used
to emit `src/squidvm_ffi.h`. Do not edit `src/squidvm_ffi.h` directly. After
changing the ABI, update the manifest and run
`python3 scripts/check-squidvm-ffi-abi.py --write-header --write-doc --write-generated`. The
checker validates Rust `#[no_mangle] extern "C"` exports, the generated C
header, `src/vm_runtime.c` callback wiring, concrete Rust/Zephyr test evidence,
and the generated inventory section in
`docs/zephyr_vm_host_abi_coverage.md`; the normal Python tooling tests run the
checker in `--check` mode.

The repo-local Zephyr setup installs the lightweight Twister dependencies
needed to run `firmware/zephyr/tests/protocol` through Zephyr's test runner.
Use `scripts/zephyr-test-protocol.sh` for that protocol suite; it selects
`native_sim/native/64`, which avoids requiring host 32-bit libc headers.
Generated protocol test fixtures come from checked-in SquidScript sources under
`firmware/zephyr/tests/protocol/fixtures`. The protocol test CMake target runs
`scripts/generate-zephyr-protocol-fixtures.py`, which compiles each `.squid`
fixture with `squidc build` and emits `squidscript_protocol_fixtures.h` in the
build directory. Zephyr protocol ztests should include that generated header
and reference the generated `<fixture>_sqbc` arrays. Do not add hand-maintained
SQBC byte arrays to `firmware/zephyr/tests/protocol/src/main.c`; add a minimal
`.squid` fixture instead so builtin ID and SQBC layout changes are exercised
through the current compiler.
Temp-run state uses the same file-backed VM storage backend as installed apps,
with a bounded temp state path cleared before each temp launch, so the firmware
does not reserve a resident saved-state-capacity RAM buffer for temp runs.
The ESP32-C3 linked Rust VM context reservation is capped at 7,872 bytes and
checked against the FFI-reported context size in Zephyr ztests. Native simulator
ztests use a larger host-only context reservation because the host Rust ABI has
larger pointer-sized VM structures; this does not change the ESP32-C3 runtime
RAM budget.
Protocol transfer sessions use explicit firmware-side bounds: 80-byte internal
staging-path buffers for fixed staging filenames and 80-byte resource-path
buffers for package-relative resource paths. The app-store path cap remains
larger because it formats full filesystem paths that include the mount point,
app ID, resource directory, and package-relative resource path.
LittleFS open-file slots are bounded at two because firmware storage paths
open and close one file per app-store, VM storage, or device-config operation.
Directory slots remain at the Zephyr default because recursive format/delete
walks can hold nested directories open.
Zephyr filesystem filename buffer is capped at 80 bytes, matching the current
package-relative resource-path protocol bound and avoiding 128-byte `fs_dirent`
name slots in every directory/stat stack frame.
Resource diagnostics are encoded directly into the caller-owned 1088-byte
protocol response buffer and do not keep a resident metric staging array.
Runtime diagnostic history is bounded to four 26-byte trace lines, six
54-byte output lines, and four 48-byte draw-log lines so recent debugging data
remains available without retaining unbounded VM text in RAM. Transient VM result
records are bounded to 26 fields, matching the largest current service result
shape and avoiding unused per-record field slots in the resident VM context.
The runtime object keeps small flags out of 32-bit alignment gaps and stores
fixed-array counters as byte-sized values when their backing arrays are capped
below 255.
Runtime physical input state is bounded to two GPIO button slots; the ESP32-C3
Super Mini reference path only needs the confirmed BOOT/GPIO9 binding plus one
additional slot for targeted diagnostics.
Runtime event-name storage is bounded to 24-byte slots, which preserves the
current measured event workload, including the 20-byte `timer.breathe.marker`
fixture, while avoiding the previous 32-byte slot size across timers, armed
timers, input bindings, and dispatch state.
Foreground runtime timers are bounded to two slots for current one-shot and
repeating timer workloads; armed app timers use a separate two-slot table.
Runtime active device bindings are bounded to three entries for the current
indicator, display/input, and targeted diagnostic binding workloads.
Runtime device-config drafts are bounded to five records with 48-byte string
values, which fits the current inline GPIO button binding shape and package
`.sqdevice` edit/rebind path without retaining an extra unused draft slot.
Installed-app VM launch storage uses 64-byte SQBC path storage and 60-byte
state path storage for the fixed `/apps/<app>/main.sqbc` and
`/state/<app>.state` shapes instead of retaining two general 128-byte app-store
path buffers.
The ESP32-C3 firmware target uses a 32-bit, 4-byte `size_t`; the Zephyr build
uses the RV32 ILP32 ABI for ESP32-C3, and `app_store.c` keeps a
`BUILD_ASSERT(sizeof(size_t) == sizeof(uint32_t))` guard for that assumption.
Replacing `size_t` with `uint32_t` is useful for FFI clarity or cross-target
bounds, but it should not be counted as an ESP32-C3 RAM reduction unless a
measured symbol or linker segment also shrinks.
Zephyr's deferred logger buffer and process-thread stack are explicitly
bounded at 512 bytes each; app-visible diagnostics use protocol output, trace,
draw-log, lifecycle, and resources responses instead of relying on a large
firmware log ring.
The protocol/main thread stack is currently 4,864 bytes and the VM worker stack
is 16,640 bytes. Resource diagnostics expose each stack's high-water use
separately so budget reductions can be tied to measured workloads instead of
inferred from static allocation alone. Current same-build hardware coverage
measured protocol/main stack use at 3,904 bytes with 960 bytes free, and VM
worker stack use at 16,112 bytes with 528 bytes free. The stack harness
enforces minimum unused-stack floors of 768 bytes for protocol/main and 384
bytes for the VM worker, printing the captured resource frame if either floor
is crossed.
For C stack attribution without hardware, build with GCC stack-usage emission
enabled and summarize the generated `.su` files:

```sh
SQUID_ZEPHYR_STACK_USAGE=1 scripts/c3-supermini-build.sh
scripts/c3-supermini-stack-usage-report.sh
```

This reports Zephyr app C source stack estimates only. It does not measure Rust
VM stack use, interrupt stack use, callee effects hidden behind library calls,
or real runtime high-water marks. The report also rolls the displayed top rows
up by source file and includes source-known cumulative call paths between
functions that have `.su` rows. Use cumulative rows before splitting helpers:
a lower per-function row can still increase the active caller-plus-callee path
if the helper remains live under the caller. Keep using `device resources`
and the hardware stack harness for final stack-budget validation.

For static DRAM attribution, build the ESP32-C3 firmware and summarize
DRAM-resident symbols:

```sh
scripts/c3-supermini-build.sh
scripts/zephyr-static-buffer-report.sh
```

The static-buffer report groups symbols as SquidScript-owned, platform-owned,
or unknown. SquidScript-owned fixed buffers include the VM runtime object, VM
worker stack, protocol response buffer, app registry, transfer sessions,
protocol scratch, serial transport, and resident app-store VM storage. The
platform group covers Zephyr kernel stacks, system heap, work queue stack,
network packet and buffer pools, ESP/Wi-Fi driver storage, logging storage, and
other target-owned symbols. Treat platform symbols as separate
target-configuration work unless the current task explicitly covers platform
RAM policy. Unknown symbols should remain small; if a large unknown appears in
the top-symbol report, classify it before using the group totals as evidence
for SquidScript-owned reductions.

The app registry scan currently reuses its path scratch buffer after opening
the app directory, reuses its directory entry for `main.sqbc` stats, and uses a
narrow app-file path buffer for the fixed `/apps/<app>/main.sqbc` shape. Its
public wrapper owns a 64-byte app-file path scratch buffer and the scan helper
owns the Zephyr directory and directory-entry structures. The current source
report attributes 80 bytes to `sq_app_store_scan_registry`, 160 bytes to
`sq_app_store_scan_registry_with_path`, and 256 cumulative bytes to
`sq_app_store_scan_registry -> sq_app_store_scan_registry_with_path ->
join_path2`.
Package resource install and staged-resource commit paths likewise reuse one
path scratch buffer after validating the app's `main.sqbc` with an open/close
check instead of a directory-entry stat, so each currently emits a 176-byte C
stack estimate instead of 432 bytes. Their parent-directory creation helper
uses the caller-owned path scratch and calls `fs_mkdir` directly, accepting
`-EEXIST` for already-created resource parent directories instead of probing
with `fs_stat`, so `ensure_resource_parent_dirs` now emits 48 bytes instead of
176 bytes and the source-known `commit_resource_install` path drops from
496 bytes to 304 bytes. Resource install and commit validation now pass their
caller-owned path scratch into `validate_app_main_sqbc_with_path`, so the
validation helper emits 32 bytes and the current cumulative
`commit_resource_install` path is 240 bytes. The current cumulative
`sq_app_store_commit_staged_resource` path is 208 bytes via
`validate_app_main_sqbc_with_path` and `format_app_path`.
Direct app install and staged-install commit paths use the fixed
`/apps/<app>/main.sqbc` path scratch; their emitted C stack estimates are now
96 bytes each instead of 288 and 304 bytes respectively. Staged-install begin
uses the same fixed app-file path cap for its app directory scratch and now
emits 112 bytes.
Package `.sqdevice` loads format the resource path directly from validated
resource bytes, so `sq_vm_runtime_device_config_load_resource` now emits a
176-byte C stack estimate instead of 304 bytes.
Recursive app-store format/delete walks reuse the caller-owned path buffer
instead of allocating a full child path per recursion, so `delete_files_under`
now emits a 160-byte C stack estimate instead of 320 bytes after the filename
buffer cap reduction. Storage format also reuses its format path scratch when
recreating top-level app-store directories, so the source-known
`storage_format` path now emits 352 bytes instead of 448 bytes, and the top
source-known main/protocol path is now 608 bytes instead of 656 bytes.
Direct app install and staged-install begin also reuse their existing
app-file/app-directory scratch when preparing top-level app-store directories,
so the source-known direct app install path now emits 224 bytes instead of 352
bytes, staged-install begin emits 240 bytes instead of 368 bytes, and the top
source-known main/protocol path is now 576 bytes.
Resource install and staged-resource commit validate the app's `main.sqbc`
with a narrow app-file-path helper instead of keeping `fs_file_t` in the full
resource path-scratch frame, so each function now emits 160 bytes instead of
176 bytes, the source-known `commit_resource_install` path emits 352 bytes
instead of 368 bytes, and the top source-known main/protocol path is now 560
bytes.
Resource parent directory creation now skips the directory-entry stat probe by
calling `fs_mkdir` and accepting `-EEXIST` for existing parents. That keeps
`ensure_resource_parent_dirs` bounded and avoids parent-directory recursion in
resource commit.
The framed `storage-format` protocol command uses a protocol-scratch format job
instead of deleting the whole app store in one handler call. The first accepted
request clears the foreground runtime, transfer sessions, and mutable registry;
each later step deletes one file/empty directory or recreates one top-level
directory, returning `PENDING` until the final `OK`. This keeps serial protocol
ownership bounded during administrative app-store cleanup. The synchronous
`sq_app_store_format_filesystem` helper remains available for startup/test code
that already owns execution and is not part of an interactive protocol loop.
VM dispatch storage completions are also sliced. The runtime worker completes
at most one pending SQBC/state storage request per scheduled worker pass, then
lets poll/wait scheduling resume the event if more storage is pending. This
keeps runtime-visible storage progress bounded while preserving the synchronous
`sq_vm_runtime_dispatch` helper for native tests and owned-execution callers.
Transfer-begin filesystem preparation now also reuses the caller-owned staging
path scratch for temp runs and staged resources. The source-known
`sq_app_store_begin_temp_run` and `sq_app_store_begin_staged_resource` paths
now emit 176 bytes, `begin_resource_install` emits 208 bytes, and
`begin_install` emits 272 bytes through the staged-install path. The top
source-known main/protocol path is now 496 bytes and has moved to app launch:
`launch_app -> start_installed_app -> sq_vm_runtime_start ->
sq_vm_runtime_start_event -> sq_vm_runtime_init`. Linker DRAM remains
185,024 bytes and the RAM audit remains 185,008 bytes.
Installed-app launch now passes event bytes and length directly into
`sq_vm_runtime_start_event`, so launch no longer carries the string-based
`sq_vm_runtime_start` wrapper. The cumulative launch path now emits 272 bytes
and `start_installed_app` emits 192 bytes. The top source-known main/protocol
path is now 480 bytes through
`dispatch_key -> dispatch_event_from_parts -> sq_vm_runtime_start_event ->
sq_vm_runtime_init`. Linker DRAM remains 185,024 bytes and the RAM audit
remains 185,008 bytes.
Key dispatch now decodes key events into the runtime-owned event scratch
instead of keeping a separate `SQ_VM_RUNTIME_EVENT_LEN` stack buffer. That
reduces `dispatch_key` from 96 bytes to 80 bytes, reduces its cumulative path
to 272 bytes, and moves the top source-known main/protocol path to 464 bytes
through `begin_install -> sq_app_store_begin_staged_install ->
prepare_filesystem_with_path -> ensure_directory`. Linker DRAM remains
185,024 bytes and the RAM audit remains 185,008 bytes.
Staged install begin reuses the protocol transfer session's 80-byte
`staging_path` buffer for filesystem preparation and app-directory creation
before formatting the final temp `main.sqbc.tmp` path. Installed-app launch,
event dispatch, resource commit, temp-run commit, and transfer-begin handlers
use byte slices, caller-owned path scratch, runtime-owned backend storage, and
null validation outputs where those forms avoid live protocol-stack staging.
Install commit updates or inserts the committed app's mutable registry entry
with `sq_app_store_update_registry_entry_with_path` instead of scanning the
full app directory after every install. The current emitted `commit_install`
frame is 48 bytes, `sq_app_store_commit_staged_install` is 96 bytes, and
registry entry updates open `main.sqbc` and use `fs_seek`/`fs_tell` for size
instead of carrying Zephyr's large `fs_dirent` stat buffer.
The previous
`commit_install -> sq_app_store_scan_registry_with_path -> join_path2`
attribution is no longer on the install-commit protocol path. The remaining
registry scan path is the public/admin
`sq_app_store_scan_registry -> sq_app_store_scan_registry_with_path ->
join_path2` path at 256 cumulative bytes; it carries the public wrapper's
64-byte app-file path scratch, a scan helper `struct fs_dir_t`, a reused
`struct fs_dirent`, and small scalars.
Planned sleep checkpoint and restore use protocol-owned scratch for the
planned-resume record, encoded bytes, temp/final paths, and `fs_file_t`. This
keeps those SQPR temporaries off the protocol/main stack while preserving the
same planned-resume file format and temp-write-plus-rename checkpoint
protocol. The protocol scratch has an explicit owner field so diagnostic builds
reject overlapping scratch use instead of silently aliasing planned-resume
storage. Current source-known stack rows report
`sq_device_protocol_restore_planned_resume -> register_app_triggers ->
register_app_trigger_timer -> sq_vm_runtime_register_armed_timer` at
240 cumulative bytes and `sq_device_protocol_poll -> register_app_triggers ->
register_app_trigger_timer -> sq_vm_runtime_register_armed_timer` at
272 cumulative bytes.
Protocol dispatch decodes only the request opcode and sequence into the live
dispatch header because opcode handlers parse payloads from the original
request bytes, so `sq_device_protocol_handle_frame` emits 80 bytes.
VM dispatch uses a static callback table plus an explicit `user_data` pointer
across the FFI boundary instead of materializing the callback table on the C
stack, so `sq_vm_runtime_dispatch` now emits an 80-byte C stack estimate
instead of 432 bytes without adding resident runtime RAM.
Protocol frame dispatch keeps opcode-specific request parsing and response
formatting out of the top-level switch, so `sq_device_protocol_handle_frame`
now emits a 96-byte C stack estimate instead of 352 bytes.
Protocol transfer begin and commit validation pass a null action output through
the Rust FFI boundary when the C handler only needs session validation and not
the decoded action record. That keeps unused `SqdpAction` staging out of
`begin_install`, `begin_resource_install`, `commit_install`, and
`commit_resource_install`, each of which now emits 32 bytes instead of 96
bytes; `commit_temp_run` now emits 112 bytes instead of 144 bytes. Chunk
handlers still keep a real action record because they need decoded offsets and
payload byte slices.
Lifecycle diagnostics encode armed timers directly from the runtime timer array
by pointer, stride, and field offsets instead of copying timer records into a
C stack staging array, so `lifecycle_response` now emits a 96-byte C stack
estimate instead of 224 bytes.
The VM worker callback keeps app-start binding preparation in an out-of-line
helper, so steady event dispatch no longer carries that setup frame:
`runtime_work_handler` now emits 16 bytes instead of 224 bytes, while
`sq_vm_runtime_prepare_app_start` is attributed separately at 16 bytes. Saved
device-config setup is attributed separately at 80 bytes, and app
device-binding setup is attributed at 128 bytes.
The fixed `/system/device-config.sqdc` path uses a 40-byte path slot and direct
formatting. That keeps `sq_app_store_device_config_path` at 16 bytes and
`sq_vm_runtime_device_config_save` at 80 bytes in the ESP32-C3 stack report.
File-backed state and device-config reads detect oversized files with a
one-byte overflow read instead of staging a `struct fs_dirent` for a size
probe, so `fs_storage_load_state` now emits 48 bytes instead of 192 bytes and
`runtime_device_config_read_file` now emits 32 bytes instead of 192 bytes.
The Zephyr VM context reserve follows the measured 32-bit Rust FFI context
size. The compact substring-capable VM string interner keeps the measured
context under the current 7,872-byte ESP32-C3 reserve.
Wi-Fi scan result backing uses the runtime transfer scratch because the Rust FFI
copies scan results out of the callback before returning to the VM. This keeps
scan SSID/BSSID/auth/network arrays out of the resident runtime object. The
ESP32-C3 Zephyr backend returns bounded real scan snapshots through the driver
scan callback; BSSID/MAC data is not exposed to SquidScript, and Zephyr auth
labels are normalized to the portable SquidScript auth labels. The diagnostic
firmware keeps a transfer-owner marker for this shared scratch so scratch,
storage-completion, and Wi-Fi-scan users fail on overlap instead of silently
aliasing the same bytes. The owner marker guards C-side scratch construction and
service callback phases; enabled Wi-Fi scan results still rely on the Rust FFI
copying result records before returning to VM execution.
The Rust VM uses one string interner for SQBC literals, firmware static strings,
and dynamic runtime text. Dynamic text can reuse exact SQBC/static matches and
contiguous substrings of existing dynamic/static text. ESP32-C3 firmware symbols
must be read from the current image under test. The validated Wi-Fi/BLE-enabled
reference build reports 239,232 bytes of linker DRAM, 239,216 bytes through
`scripts/zephyr-ram-audit.sh`, and an 11,920-byte `runtime.4` static runtime
symbol. The static-buffer report classifies that image's top symbols as
123,310 bytes platform-owned, 31,616 bytes SquidScript-owned, and 10,729 bytes
unknown small symbols. Rebuild and rerun the reports before treating these
values as current for a different firmware image.
The resident protocol response buffer is 1,088 bytes, sized for the current
largest bounded protocol response. Resource metric values use the protocol's
U32 TLV type because ESP32-C3 diagnostic counters fit within 32-bit ranges;
request transfer lengths and CRCs continue to use U64 where needed. The larger
resource ceiling is intentional:
`device resources` includes heap largest-free-block support/value fields so host
RAM diagnostics can distinguish "not exposed safely by this Zephyr build" from
an actual zero-byte largest block. `device resources --reset-heap-max` sends the
normal resources request with bool field tag `1`, causing firmware to reset
Zephyr's heap allocation high-water statistic to the current allocated bytes
before returning the sampled resource frame.
The resident serial receive buffer is 256 bytes, with host upload chunking
derived from the same encoded frame budget. Transfer chunk requests use
36 bytes of protocol overhead, so current host tooling sends 220-byte upload
chunks, and the maximum app-id plus resource-path transfer-begin request remains
within the same fixed receive buffer.
Protocol polling reuses runtime app-id/event scratch for lifecycle and armed
timer transitions. App-arm trigger discovery is split out of the steady poll
frame and uses a dedicated resident trigger metadata storage backend instead of
allocating a per-call SQBC path and filesystem storage wrapper or overwriting
the active foreground launch storage backend. The emitted C stack report
attributes trigger registration separately from the steady poll path.
Protocol event dispatch passes event bytes directly into
`sq_vm_runtime_start_event` instead of staging a NUL-terminated event buffer on
the protocol stack, reducing `dispatch_event_from_parts` from 112 bytes to 96
bytes. The existing string-based `sq_vm_runtime_start` remains as a wrapper for
callers that already own NUL-terminated event names.
Installed-app foreground handoff reuses the runtime-owned pending-launch app-id
slot as temporary rollback storage while switching `current_app`, instead of
allocating a protocol-stack previous-app buffer. Host `app-launch` and
app-driven `app.launch` both dispatch the current foreground app's `app.exit`
before pushing the return target and starting the requested app.
Display draw-log, GPIO, indicator, timer, app lifecycle, and Wi-Fi VM service
calls now cross the Rust FFI boundary into Zephyr callbacks. Zephyr now
performs installed-app foreground handoff for `app.launch` and `app.exit` with
a bounded return stack, foreground timer cleanup, and `device lifecycle`
diagnostics. `app.arm` and `app.disarm` now manage bounded timer-triggered
armed app registrations and dispatch armed timer events as foreground app
starts. Planned sleep writes a firmware-owned lifecycle record under
`/sq/system/planned-resume.sqpr`, preserving the active foreground app id,
foreground return stack app ids, and armed app ids across ESP32-C3 timer
wake. It does not persist VM frames, current screen, or foreground timers;
apps persist their own content state through `state.save()` and `state.load()`.
See `docs/app_lifecycle_state_machine.md` for the explicit lifecycle phases,
transition rules, start reasons, and failure cases.
The current Zephyr Wi-Fi callbacks use Zephyr Wi-Fi management for
status, scan, AP start/stop, AP IP reporting, station connect/disconnect, and
station DHCP/IP status reporting. `device wifi-profile` stores one volatile
bounded station profile in runtime memory without echoing SSIDs or passwords.
AP start also starts Zephyr's bounded DHCPv4 server on the AP interface; an
external-client association and lease proof remains separate hardware coverage.
The ESP32-C3 reference configuration intentionally uses 6/6 native network
packet pools and 16/16 network buffer pools instead of Zephyr's
Ethernet-oriented defaults because current firmware networking is
low-throughput control-plane traffic, not a TCP or bulk-transfer workload. It
also uses measured socket-service,
network-management event, ESP timer task, and network RX stack budgets for the
same current scope.
The app-store layer now derives bounded file paths for `main.sqbc` and app
state from a mount point plus validated app ID, and ESP32-C3 firmware attempts
to mount the `storage_partition` LittleFS volume at `/sq` during boot without
blocking framed serial command transport if storage is unavailable. Install-time
app directory creation, boot-time registry scanning, package-resource lookup
paths, diagnostics, resources, app lifecycle orchestration, and Wi-Fi service
records are connected through the current Zephyr command and VM host surface.
For the current ESP32-C3 Super Mini Zephyr 4 MB layout, `storage_partition` is
the 192 KiB region at offset `0x3b0000`. The default table also provides
`image-0`, `image-1`, and `image-scratch` firmware-update partitions; the
current SquidScript workflow flashes firmware directly and does not yet expose
a user-facing A/B or OTA update flow.

## Target Definitions

Target JSON describes portable capabilities and board metadata. It must not
make Zephyr Kconfig symbols, devicetree nodes, thread handles, or driver state
part of the SquidScript language contract.

Backend-specific generated artifacts may include:

- Zephyr devicetree overlays.
- Kconfig fragments.
- C target configuration headers.
- Flash-map and storage layout metadata.
- Firmware manifest/provenance data.
- Generated app capability tables.

The Zephyr build generates `squidscript_target_defaults.h` from
`SQUID_ZEPHYR_TARGET_JSON`. The ESP32-C3 Super Mini wrappers default that
variable to `targets/esp32c3-super-mini.target.json`. The generated header is
used for SquidScript-facing target defaults such as `indicator.default` and
for the GPIO-capability mask used by device-binding validation. The generator
also validates those defaults against `SQUID_ZEPHYR_TARGET_OVERLAY` so the
target JSON indicator GPIO, polarity, and PWM frequency cannot silently drift
from the Zephyr overlay. Zephyr devicetree still owns driver nodes, PWM
channels, and pinctrl setup.

Generated target defaults are trusted firmware metadata, not author/package
input. Firmware should keep them visible through the documented runtime
configuration view when that view exists, but it may apply them through a
direct target-specific path instead of the general draft/rebind machinery when
that avoids measured stack or RAM pressure. The direct path must produce the
same active binding state as the generated default and must not change the
ordering rule: target defaults apply before saved global config and app-local
`device {}` bindings, so authors can still override them through normal
package, saved, or runtime `device.config.rebind(...)` flows. Defaults whose
metadata is not generated and validated by the target build remain ordinary
device config and should use the normal rebind path.

## Runtime Boundary

The Zephyr host calls the Rust VM through C ABI functions for:

- context allocation sizing and initialization
- SQBC byte-range reads
- event dispatch
- storage suspend/resume
- state inspection and mutation
- diagnostics and error mapping
- service result conversion

The browser simulator remains separate. It shares the SquidScript service
contract but does not use Zephyr or the firmware C ABI.

## Non-Goals

- Do not maintain the old Rust ESP firmware as a fallback.
- Do not reimplement the VM in C/C++.
- Do not expose Zephyr objects in SquidScript source, compiler core, SQBC, or
  portable app APIs.
- Do not add SQBC or firmware compatibility modes before 1.0.
