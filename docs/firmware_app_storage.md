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

On firmware startup, Zephyr scans the app store, rebuilds the registry from
valid installed apps, and boots installed `main` when present. If `main` is
missing or invalid, firmware remains in device-command mode.

`RUN.TEMP` bytecode is staged as a temporary app-store file instead of being
buffered in RAM. It is not published into the installed app registry and does
not overwrite `main`.

The ESP32-C3 Zephyr reference firmware keeps twelve installed-app registry
entries resident in RAM. If more valid app directories are present, registry
rebuild returns `ENOSPC` so the limit is visible instead of silently truncating
the app list.

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
adapter is bounded by the FFI transfer capacity and is tested with an in-memory
backend, including a native Zephyr ztest that runs real SQBC through the linked
Rust VM and completes its state load/save flow through the adapter.

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

Installed app execution keeps a bounded VM context and one SQBC code transfer
window resident. VM initialization reads the SQBC header, section table, string
pool, state table, function table, handler table, optional trigger table, and
optional screen table through caller-owned scratch. Handler/function/screen
code ranges are then requested on demand as `SQVM_STORAGE_REQUEST_SQBC_READ`
records and completed from LittleFS through the same storage adapter. App arm
registration reads trigger metadata through the reader API and does not
dispatch or keep a background VM resident. The `device resources` response
reports `runtime_static_bytes` for the resident runtime object and
`vm_sqbc_chunk_bytes` for the maximum SQBC code/read transfer window.

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
On targets with a firmware-defined default indicator, runtime initialization
and installed app start load that target default into the same in-memory SQDC
draft/rebind path before app code runs. Installed app launch clears and
rebuilds active logical bindings, applies saved global SQDC defaults, then
reads current SQBC top-level `device {}` metadata and applies packaged
`indicator.default` `.sqdevice` bindings, packaged display `.sqdevice`
bindings, and inline `gpio:GPIO<n>` indicator bindings before
`event.on("app.start")`. Input `gpio-button` bindings are also activated
before `event.on("app.start")`, so a physical button can dispatch the mapped
logical key event after launch. App-local top-level
`device {}` bindings run after target and saved global defaults, so app package
bindings can override them. Inline GPIO bindings are normalized into the same
in-memory SQDC draft/rebind path as packaged resources and do not install a
package resource.
Active config persistence through `device.config.save("flash")` writes
firmware-owned binary SQDC at `/sq/system/device-config.sqdc` on the ESP32-C3
reference target.

## App State

App state is separate from installed app bytecode. Scripts declare typed state
slots in SQBC metadata. Firmware persists declared primitive values, not the VM
stack, current screen, timers, armed triggers, or handles.

`state.load()` applies matching slots by name and declared type, leaves missing
slots at defaults, ignores removed slots, and fails with a VM error for
malformed records or matching-name type/value mismatches. `state.save()` writes
the current typed record. `state.reset()` clears the persistent state record for
the app.

`RUN.TEMP` state is volatile and RAM-backed, bounded to the VM saved-state
capacity. Only the temporary SQBC bytecode artifact is file-backed.
