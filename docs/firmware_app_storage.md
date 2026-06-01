# Firmware App Storage

Status: Zephyr storage model.

Zephyr owns real firmware storage. SquidScript app storage is a logical model
implemented on target-specific Zephyr flash-map volumes, NVS records, and
LittleFS file layouts where files are needed.

## Logical Model

Host tooling installs compiled SQBC payloads and package resources through the
Zephyr command surface. Firmware validates app IDs, byte counts, payload
integrity, and SQBC structure before publishing an app in the registry.

For `.squid.zip` packages, host tooling validates and unpacks the ZIP first.
Firmware is not required to parse ZIP archives directly. The host installs
`main.sqbc` and streams normalized package resources as read-only app files.

On firmware startup, Zephyr scans the app store and rebuilds the registry from
valid installed apps. If installed `main` is present, firmware launches it as
the root foreground app. If no installed foreground root is available, firmware
launches its target-specific built-in fallback app as logical `main`.

The fallback app is authored as SquidScript, compiled to SQBC by the firmware
build, and executed through the same VM, SQBC reader, runtime callbacks, and
foreground lifecycle path as installed apps. It is not published into the
installed app registry and does not change authored-app entry rules: projects
still need an explicit app entry point. Firmware selects the fallback only
after app-store mount and registry scan succeed and no installed `main` exists,
so app-store failures and broken installed apps remain visible diagnostics
rather than silently falling through to fallback behavior.

`RUN.TEMP` bytecode is staged as a temporary app-store file instead of being
buffered in RAM. It is not published into the installed app registry and does
not overwrite `main`.

The ESP32-C3 Zephyr reference firmware keeps eight installed-app registry
entries resident in RAM, covering the current measured app workloads without
retaining the previous eleven-entry table. If more valid app directories are
present, registry rebuild returns `ENOSPC` so the limit is visible instead of
silently truncating the app list.

## Physical Storage

The portable app/compiler contract does not expose physical storage layout.
Board metadata and firmware docs describe the actual Zephyr flash-map,
partition, NVS, and LittleFS choices for each target.

Use LittleFS for app bytecode and package resources when a file layout is
needed. Use NVS or LittleFS records for app state based on implementation
tests. Do not migrate old Rust firmware on-device data; pre-1.0 storage is
replace-directly.

The current Zephyr VM host storage boundary is `firmware/zephyr/src/vm_storage`.
It translates `squidvm-ffi` resumable storage requests into backend callbacks:
SQBC byte-range read, app state load, app state save, and app state reset. The
adapter is bounded by the 768-byte FFI transfer capacity and is tested with an
in-memory backend, including a native Zephyr ztest that runs real SQBC through
the linked Rust VM and completes its state load/save flow through the adapter.
Runtime dispatch does not drain an unbounded chain of pending storage requests
in one worker pass. `sq_vm_runtime_dispatch_slice` starts or resumes one VM
event and completes at most the caller-provided number of pending storage
requests; the Zephyr async runtime worker uses a one-request slice and is
rescheduled by `sq_vm_runtime_poll` / `sq_vm_runtime_wait_idle` until the event
finishes. The synchronous `sq_vm_runtime_dispatch` helper remains available for
native tests and other callers that already own execution; it drives slices
until completion.

`firmware/zephyr/src/vm_fs_storage` is the current file-backed implementation
of that callback boundary. It uses Zephyr `fs_*` APIs to read byte ranges from
an SQBC path and to load, save, and reset an app-state path. Native Zephyr
ztests mount a host-backed filesystem through `FS_NATIVE_MOUNT` and verify the
backend through `vm_storage`, so the behavior is covered without bypassing
Zephyr's filesystem layer. The backend records SQBC read count, maximum read
length, and total read length for tests and diagnostics-facing assertions.
Installed app launch/dispatch uses these file-backed reads through the
`sq_app_store_vm_storage` backend; native Zephyr ztests install padded
`main.sqbc` files larger than one storage transfer and verify dispatch reads
only bounded SQBC byte ranges instead of reading the full installed payload.
The armed-app lifecycle test also installs a trigger app larger than the legacy
full-app limit and verifies arm registration still reads trigger metadata
through the reader path.
The resident installed-app VM storage keeps separate bounded path buffers for
the fixed app bytecode and state-file path shapes: 64 bytes for
`/apps/<app-id>/main.sqbc` and 60 bytes for `/state/<app-id>.state`.
Runtime output history is intentionally retained in a six-entry fixed
window. This keeps the current lifecycle and hardware-test assertion window
available through `device output` without making debug output an unbounded
resident RAM sink.

Installed app execution keeps a bounded VM context and one SQBC code transfer
window resident. VM initialization reads the SQBC header, section table, string
pool, state table, function table, handler table, optional trigger table, and
optional screen table through caller-owned scratch. Handler/function/screen
code ranges are then requested on demand as `SQVM_STORAGE_REQUEST_SQBC_READ`
records and completed from LittleFS through the same storage adapter. App arm
registration reads trigger metadata through the reader API using a separate
resident app-store VM storage backend from the foreground launch backend. This
keeps trigger metadata discovery from overwriting the file-backed SQBC/state
paths currently owned by the active foreground app. Trigger registration does
not dispatch or keep a background VM resident. The `device resources` response
reports `runtime_static_bytes` for the resident runtime object and
`vm_sqbc_chunk_bytes` for the maximum SQBC code/read transfer window.

The built-in fallback app uses a read-only in-memory SQBC storage backend. Its
state load returns no saved state and its state save/reset callbacks are no-ops,
so the fallback can exercise normal VM service paths without creating app-store
state records.

`firmware/zephyr/src/app_store` owns the current app-store layout boundary. It
prepares `/apps`, `/state`, and `/tmp` under the mounted store, validates app
IDs for path construction, formats the store by deleting stale files and
per-app/per-resource directories below those roots, and derives VM storage
paths:

```text
/sq/apps/<app-id>/main.sqbc
/sq/state/<app-id>.state
```

The ESP32-C3 Zephyr reference firmware stores app IDs in bounded resident
buffers and accepts app IDs up to 39 bytes, with one byte reserved for the C
string terminator. Host serial tooling and the Rust `sqdp_` FFI use the same
limit so oversized IDs fail before firmware path construction or registry
storage.

On ESP32-C3 Zephyr builds, firmware attempts to mount the `storage_partition`
LittleFS volume at `/sq` during boot and then prepares those directories. If
the store is unavailable, the serial protocol stays available and reports the
condition through diagnostics/logging rather than blocking framed command
transport. The app store can create per-app directories during install, write
`main.sqbc`, and rebuild a bounded in-memory registry by scanning
`/sq/apps/*/main.sqbc`; native Zephyr ztests cover directory creation, file
size metadata, and registry lookup.
The current ESP32-C3 Super Mini Zephyr 4 MB flash layout reserves
`storage_partition` at offset `0x3b0000` with size `0x30000` bytes, so the
LittleFS app store is 192 KiB. Firmware is built for the `image-0` slot; the
default Zephyr partition table also includes `image-1`, `image-scratch`, and
`coredump` partitions, but SquidScript does not currently expose a user-facing
A/B or OTA firmware update flow.
Flashing a new firmware image does not erase the LittleFS app store partition.
If a persisted pre-fix app prevents boot or serial command processing during
development, erase only the app-store partition before retrying:

```sh
esptool --port /dev/ttyACM0 erase-region 0x3b0000 0x30000
```

Temp runs use `/sq/tmp/temp-run.sqbc.tmp` as their staging artifact. Firmware
writes chunks directly to that file and lets the VM read SQBC byte ranges back
through the same storage callback shape used by installed apps. The
begin/chunk/commit session validation for installed apps, temp runs, and
resources is implemented in the Rust `sqdp_` FFI layer with caller-owned
buffers; Zephyr C performs the LittleFS writes and commits, then reports
successful chunks back to Rust so byte counts and CRC32 progress advance only
after storage succeeds.

Package resources are stored below the app directory using package-relative
paths:

```text
/sq/apps/<app-id>/resources/<resource-path>
```

Resource paths must be relative, non-empty, and made of safe path segments.
Absolute paths, empty segments, `.` segments, and `..` traversal are rejected.
Installing resources requires the app's `main.sqbc` to exist first, which keeps
resources tied to a published app directory. Native Zephyr ztests cover nested
resource directory creation, lookup-path derivation, file size metadata, and
path traversal rejection. Framed device commands expose install, registry, and
resource behavior through the Zephyr command surface.

Device configuration parsing and SQDC encoding for firmware use a bounded
Rust FFI core in `squidvm-ffi`, not ad hoc C parsing. The core operates on
caller-owned fixed `SqdcConfig` records, validates safe package-relative
`.sqdevice` paths, parses SQDEVICE text resources, applies primitive draft
updates, and encodes/decodes binary SQDC without heap allocation. The current
Zephyr runtime wires `device.config.load("package:...")` to installed
read-only package resources for the foreground app and wires
`device.config.set(...)` to the runtime draft config. The runtime can validate
and activate the current `indicator.default` GPIO binding with
`device.config.rebind(...)`, and `service.indicator.*` uses the active binding.
The runtime also tracks non-indicator active bindings whose SQDEVICE draft
declares a matching `service` alias, such as `display.status`, so package
display bindings can be applied before app start. Rust FFI plans top-level app
`device {}` declarations: it classifies supported package `.sqdevice`
resources, inline `gpio:GPIO<n>` indicator resources, and inline
`gpio-button:GPIO<n>:key.<KEY>:activeLow|activeHigh` input resources. The Rust
planner normalizes inline resources to SQDC draft records. Zephyr C remains
responsible for LittleFS reads, generated target-metadata checks, and hardware
activation. Inline GPIO and `.sqdevice` GPIO bindings that drive physical GPIO
must name a GPIO-capable pin from the selected target metadata before Zephyr
activates them. Inline GPIO-button input bindings activate as polled GPIO
inputs; a pressed edge dispatches the configured logical key event to the
foreground app.
On targets with firmware-defined defaults, runtime initialization and installed
app start expose those defaults through the same in-memory SQDC draft shape
that author/device config uses. Firmware may apply trusted generated defaults
through a direct target-specific path instead of routing them through
`device.config.rebind(...)`, provided the generated metadata has already been
validated against the target definition and hardware overlay and the resulting
active binding is equivalent. Author-provided, package-provided, and saved
global device config still use the normal draft/rebind path.
Installed app launch clears and rebuilds active logical bindings, applies
target defaults, applies saved global SQDC defaults, then reads current SQBC
top-level `device {}` metadata and applies packaged `indicator.default`
`.sqdevice` bindings, packaged display `.sqdevice` bindings, and inline
`gpio:GPIO<n>` indicator bindings before `event.on("app.start")`. Input
`gpio-button` bindings are also activated before `event.on("app.start")`, so a
physical button can dispatch the mapped logical key event after launch.
App-local top-level `device {}` bindings run after target and saved global
defaults, so app package bindings can override them. Inline GPIO bindings are
normalized into the same in-memory SQDC draft/rebind path as packaged
resources and do not install a package resource.

For app authors, `device {}` is the default path for static app-owned
bindings. It declares what the app needs, lets firmware activate the binding
before `app.start`, and avoids manual runtime config calls in startup code.
Use `device.config.load("package:...")` when the app deliberately controls
device configuration at runtime, such as loading one of several package
resources, editing the draft with `device.config.set(...)`, rebinding an
active service, saving a device-level hardware choice, or running diagnostics
against the device-config service path.
Active config persistence through `device.config.save("flash")` writes
firmware-owned binary SQDC at `/sq/system/device-config.sqdc` on the ESP32-C3
reference target. This storage is for active hardware/service bindings, not
app state; app data should use the normal `state.save()` path.

## App State

App state is separate from installed app bytecode. Scripts declare typed state
slots in SQBC metadata. Firmware persists declared primitive values, not the VM
stack, current screen, foreground timers, or handles.

`state.load()` applies matching slots by name and declared type, leaves missing
slots at defaults, ignores removed slots, and fails with a VM error for
malformed records or matching-name type/value mismatches. Loaded string values
that exactly match an existing SQBC string-pool literal or firmware static
string reuse that reference instead of consuming dynamic string storage; other
loaded strings are retained in the VM string interner. `state.save()` writes
the current typed record.
`state.reset()` clears the persistent state record for the app.

`RUN.TEMP` state is volatile and RAM-backed, bounded to the VM saved-state
capacity. Only the temporary SQBC bytecode artifact is file-backed.

## Planned Sleep Lifecycle Checkpoint

Planned sleep uses separate firmware-owned lifecycle storage at
`/sq/system/planned-resume.sqpr` on the ESP32-C3 reference target. This record
is not app state and is consumed by boot policy only after a supported wake
source resumes the MCU.

The planned-resume record contains only lifecycle routing metadata:

- active foreground app id
- foreground return stack app ids
- armed app ids

It does not contain VM stack frames, local variables, current screen,
foreground timers, trigger event rows, or app content state. On wake, firmware
restarts the restored foreground app with `app.start` and
`system.startReason() == "wake"`, re-registers armed app triggers by reading
current installed app metadata, and preserves return-stack behavior for
`app.exit()`. Apps remain responsible for saving and loading their own content
state with `state.save()` and `state.load()`.
See `docs/app_lifecycle_state_machine.md` for planned sleep and wake restore
in the broader app lifecycle state machine.
