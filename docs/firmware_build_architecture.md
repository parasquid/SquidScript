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
The repo-local Zephyr setup installs the lightweight Twister dependencies
needed to run `firmware/zephyr/tests/protocol` through Zephyr's test runner.
Use `scripts/zephyr-test-protocol.sh` for that protocol suite; it selects
`native_sim/native/64`, which avoids requiring host 32-bit libc headers.
Temp-run state uses the same file-backed VM storage backend as installed apps,
with a bounded temp state path cleared before each temp launch, so the firmware
does not reserve a resident saved-state-capacity RAM buffer for temp runs.
The ESP32-C3 linked Rust VM context reservation is capped at 10,400 bytes and
checked against the FFI-reported context size in Zephyr ztests. Native simulator
ztests use a larger host-only context reservation because the host Rust ABI has
larger pointer-sized VM structures; this does not change the ESP32-C3 runtime
RAM budget.
Protocol transfer sessions use explicit firmware-side bounds: 72-byte internal
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
Resource diagnostics are encoded directly into the caller-owned 826-byte
protocol response buffer and do not keep a resident metric staging array.
Runtime diagnostic history is bounded to four 26-byte trace lines, five 54-byte
output lines, and four 48-byte draw-log lines so recent debugging data remains
available without retaining unbounded VM text in RAM. Transient VM result
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
The protocol/main thread stack is currently 3,264 bytes and the VM worker stack
is 18,016 bytes. Resource diagnostics expose each stack's high-water use
separately so budget reductions can be tied to measured workloads instead of
inferred from static allocation alone. The 3,264-byte protocol stack keeps
788 bytes of headroom over the last measured 2,476-byte protocol peak and
needs physical revalidation with the hardware stack harness. The stack harness
also enforces minimum unused-stack floors of 768 bytes for protocol/main and
384 bytes for the VM worker, printing the captured resource frame if either
floor is crossed.
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
The app registry scan currently reuses its path scratch buffer after opening
the app directory, reuses its directory entry for `main.sqbc` stats, and uses a
narrow app-file path buffer for the fixed `/apps/<app>/main.sqbc` shape. Its
emitted C stack estimate is 224 bytes instead of retaining separate
app-directory/SQBC-path buffers, a second directory entry, and the general
128-byte path buffer; the current 80-byte Zephyr filename buffer trims the
remaining `fs_dirent` name slot.
Package resource install and staged-resource commit paths likewise reuse one
path scratch buffer after validating the app's `main.sqbc` with an open/close
check instead of a directory-entry stat, so each currently emits a 176-byte C
stack estimate instead of 432 bytes.
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
buffer cap reduction.
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
size: `sqvm_context_size()` currently emits 10,392 bytes in the ESP32-C3 build,
so the C runtime reserves 10,400 bytes instead of 10,880 bytes. That reduces
the static `runtime.3` block from 15,232 bytes to 14,752 bytes.
Runtime field ordering and byte-sized fixed-array counters further reduce
`runtime.3` from 14,752 bytes to 14,720 bytes without changing runtime
capacity.
The resident protocol response buffer is 826 bytes, matching the current
resources-response ceiling: 806 bytes of metric payload plus the 20-byte frame
header. This trims the previous 848-byte buffer without changing the response
set.
The resident serial receive buffer is 256 bytes, with host upload chunking
derived from the same encoded frame budget. Transfer chunk requests use
36 bytes of protocol overhead, so current host tooling sends 220-byte upload
chunks, and the maximum app-id plus resource-path transfer-begin request remains
within the same fixed receive buffer.
Protocol polling reuses runtime app-id/event scratch for lifecycle and armed
timer transitions. App-arm trigger discovery is split out of the steady poll
frame and reuses the caller-owned launch storage path buffers instead of
allocating a per-call SQBC path and filesystem storage wrapper. The emitted C
stack report now attributes `sq_device_protocol_poll` at 32 bytes,
`register_app_triggers` at 64 bytes, and per-trigger timer decode/register at
96 bytes, down from the earlier combined 400-byte trigger-registration frame.
Protocol event dispatch passes event bytes directly into
`sq_vm_runtime_start_event` instead of staging a NUL-terminated event buffer on
the protocol stack, reducing `dispatch_event_from_parts` from 112 bytes to 96
bytes. The existing string-based `sq_vm_runtime_start` remains as a wrapper for
callers that already own NUL-terminated event names.
Installed-app foreground handoff reuses the runtime-owned pending-launch app-id
slot as temporary rollback storage while switching `current_app`, instead of
allocating a protocol-stack previous-app buffer. That reduces
`start_installed_app` from 128 bytes to 96 bytes and clears the scratch before
returning.
Display draw-log, GPIO, indicator, timer, app lifecycle, and Wi-Fi VM service
calls now cross the Rust FFI boundary into Zephyr callbacks. Zephyr now
performs installed-app foreground handoff for `app.launch` and `app.exit` with
a bounded return stack, foreground timer cleanup, and `device lifecycle`
diagnostics. `app.arm` and `app.disarm` now manage bounded timer-triggered
armed app registrations and dispatch armed timer events as foreground app
starts. The current Zephyr Wi-Fi callbacks use Zephyr Wi-Fi management for
status, scan, AP start/stop, AP IP reporting, station connect/disconnect, and
station DHCP/IP status reporting. `device wifi-profile` stores one volatile
bounded station profile in runtime memory without echoing SSIDs or passwords.
AP start also starts Zephyr's bounded DHCPv4 server on the AP interface; an
external-client association and lease proof remains separate hardware coverage.
The ESP32-C3 reference configuration intentionally uses smaller native
networking packet/buffer pools than Zephyr's Ethernet-oriented defaults because
current firmware networking is low-throughput control-plane traffic, not a TCP
or bulk-transfer workload. It also uses measured socket-service,
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
