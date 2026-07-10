# SquidScript Runtime Limits

This document is the agent- and app-author-facing entry point for bounded
runtime resources enforced by the Zephyr firmware.

Target JSON selects a build-time hard-cap profile through
`firmware.zephyr.runtimeLimits`. The ESP32-C3 Zephyr targets currently select
`targets/runtime-limits/esp32c3-zephyr.json`. Target Kconfig generation emits
the selected profile into `target/zephyr/generated/*-target.conf`, and
`firmware/zephyr/src/runtime_limits.h` bridges those `CONFIG_SQ_*` symbols to
the C `SQ_*` macros. The JSON fallback values in the generated header keep
standalone C builds working when Kconfig is absent.

The hard caps size backing arrays and wire buffers. Runtime-tunable active caps
may lower selected behavioral limits, but they never exceed the hard cap and do
not shrink the compiled storage layout.

## Foreground App Session Limits

The active foreground app has a single per-session runtime instance.

| Limit | Hard value | Macro | Runtime tunable |
| --- | ---: | --- | --- |
| Foreground timer slots | 4 | `SQ_VM_RUNTIME_TIMER_MAX` | yes |
| Armed timer slots | 2 | `SQ_VM_RUNTIME_ARMED_TIMER_MAX` | yes |
| Active device-binding slots | 3 | `SQ_VM_RUNTIME_ACTIVE_BINDING_MAX` | yes |
| Input button slots | 8 | `SQ_VM_RUNTIME_INPUT_BUTTON_MAX` | yes |
| Queued input event slots | 8 | `SQ_VM_RUNTIME_INPUT_EVENT_QUEUE_MAX` | no |
| Event name max length | 23 UTF-8 bytes (+ NUL) | `SQ_VM_RUNTIME_EVENT_LEN` | no |
| Trace record slots | 4 | `SQ_VM_RUNTIME_TRACE_MAX` | no |
| Trace record length | 26 bytes | `SQ_VM_RUNTIME_TRACE_LEN` | no |
| Output line slots | 6 | `SQ_VM_RUNTIME_OUTPUT_MAX` | yes |
| Output line length | 54 bytes | `SQ_VM_RUNTIME_OUTPUT_LEN` | no |
| Drawlog record slots | 4 | `SQ_VM_RUNTIME_DRAWLOG_MAX` | yes |
| Drawlog record length | 48 bytes | `SQ_VM_RUNTIME_DRAWLOG_LEN` | no |
| Framebuffer size | 48000 bytes | `FB_FRAMEBUFFER_SIZE` | yes (per target) |
| Retained device error slots | 8 | `SQ_VM_RUNTIME_DEVICE_ERROR_MAX` | no |
| Device error line length | 48 bytes | `SQ_VM_RUNTIME_DEVICE_ERROR_LEN` | no |
| Runtime record values | 9 | `MAX_RUNTIME_RECORDS` | no |
| Runtime list items | 8 | `MAX_RUNTIME_LIST_ITEMS` | no |
| File/content library page entries | 8 | `SQ_VM_RUNTIME_CONTENT_LIST_MAX` | no |
| File/content library ref length | 128 bytes | `SQ_VM_RUNTIME_CONTENT_REF_LEN` | no |

`service.timer.every(...)`, `service.timer.after(...)`, and `app.triggers`
share the foreground or armed timer caps. Registering one beyond the active cap
returns `-ENOSPC` to the VM. Physical input sampling can enqueue up to
`SQ_VM_RUNTIME_INPUT_EVENT_QUEUE_MAX` press events while the foreground VM is
busy; overflow drops the newest input event and records `input_queue_overflow`
in device errors. Display flushes run on a separate bounded worker so input and foreground event dispatch can continue while the e-paper controller is busy.

`file.list(...)` and `content.binbook.list(...)` materialize at most one
library page of `SQ_VM_RUNTIME_CONTENT_LIST_MAX` entries per call. Each `ref`
is an opaque logical identifier bounded by `SQ_VM_RUNTIME_CONTENT_REF_LEN`.
Content-specific lists are layered over the same generic file listing model
instead of defining the firmware storage architecture.

The runtime record arena must hold one result record plus every item record in
a full page. `MAX_RUNTIME_RECORDS` is therefore at least
`MAX_RUNTIME_LIST_ITEMS + 1`.

`binbook.chapters(...)` materializes at most one chapter page of
`SQ_VM_RUNTIME_CONTENT_LIST_MAX` entries per call. Chapter titles are copied
from BinBook `CHAPTER_INDEX` records into fixed per-entry firmware buffers
bounded by `SQ_VM_RUNTIME_CONTENT_NAME_LEN`. `binbook.chapter(...)` reads one
chapter entry and uses one of the same bounded title buffers.

## Upload Profiles And Transport Buffers

| Limit | Hard value | Macro |
| --- | ---: | --- |
| SQBC upload profiles | 16 | `MAX_UPLOAD_PROFILES` |
| Accepted extensions per profile | 4 | `SQVM_UPLOAD_PROFILE_ACCEPT_MAX` |
| Transports per profile | 4 | `SQVM_UPLOAD_PROFILE_TRANSPORT_MAX` |
| Event routes per profile | 8 | `SQVM_UPLOAD_PROFILE_EVENT_MAX` |
| Upload profile text field length | 32 bytes | `SQVM_UPLOAD_PROFILE_TEXT_CAP` |
| Zephyr HTTP socket-to-storage chunk buffer | 2048 bytes | `SQ_HTTP_UPLOAD_CHUNK_MAX` |
| Native X4 HTTP body/storage chunk | 512 bytes | `BODY_BUF` / `UPLOAD_STAGE_CHUNK_BYTES` |
| Native X4 BLE transfer chunk | 192 bytes | `BLE_PIPELINE_CHUNK_BYTES` |
| Native X4 queued transfer chunks | 4 | `BLE_PIPELINE_DEPTH` |
| Native X4 transfer-buffer budget | 2048 bytes | `BLE_PIPELINE_BUFFER_BUDGET_BYTES` |
| Native X4 Trouble Host packet pool | 32 × 27-byte packets | `trouble-host` feature selection |
| Native BLE connection inactivity | 30000 ms by default | `firmware.native.bleConnectionWatchdogMs` |

SQBC carries unified upload profiles. Each profile stores the app-local id,
accepted extensions, enabled transport names, and completion route. Native
firmware retains one active foreground profile. Zephyr bounds the BLE adapter's
registered route table to 2 entries with `SQ_VM_RUNTIME_BLE_PROFILE_MAX` and
the HTTP adapter to one active foreground route; both adapters resolve unified
profile metadata and feed the same app-facing completion contract.

Profiles are activated imperatively when the foreground app runs
`service.upload.start(...)`; one profile is set or replaced for the active
runtime. Each transport accepts one in-flight upload at a time.

The route table must resolve a transport and uploaded extension to exactly one receiver. If
two active routes accept the same extension, or if a route points at a registry
slot that no longer resolves, firmware records an `invariant.ble.*` diagnostic
through `device errors` and rejects the transfer instead of selecting an
arbitrary receiver.

Neither transport allocates storage proportional to the uploaded file. The
Zephyr's HTTP producer streams through one fixed 2048-byte buffer; native X4
uses 512-byte body/storage chunks. The native X4 GATT
producer queues at most four fixed 192-byte chunks. When those
slots are occupied, the pending ATT Write With Response is not acknowledged
until the SD consumer frees a slot. The 2048-byte budget covers the transfer
queue, GATT producer and storage-consumer frames, and active session metadata;
it excludes the radio stack's general packet pool and persistent app profile
metadata. The native BLE inactivity watchdog is a connection-liveness bound,
not an upload-size or duration limit; protocol activity resets it.

## App Store Limits

| Limit | Hard value | Macro |
| --- | ---: | --- |
| Installed apps | 8 | `SQ_APP_STORE_MAX_APPS` |
| Generic app-store path | 128 bytes | `SQ_APP_STORE_PATH_MAX` |
| App ID storage | 40 bytes | `SQ_APP_STORE_APP_ID_MAX` |
| App file path | 64 bytes | `SQ_APP_STORE_APP_FILE_PATH_MAX` |
| App state path | 60 bytes | `SQ_APP_STORE_APP_STATE_PATH_MAX` |
| Device-config path | 40 bytes | `SQ_APP_STORE_DEVICE_CONFIG_PATH_MAX` |
| Runtime-config path | 40 bytes | `SQ_APP_STORE_RUNTIME_CONFIG_PATH_MAX` |
| Planned-resume path | 48 bytes | `SQ_APP_STORE_PLANNED_RESUME_PATH_MAX` |

## Wire-Format Limits

| Limit | Hard value | Macro |
| --- | ---: | --- |
| Device response bytes | 1280 B | `SQ_DEVICE_RESPONSE_BYTES` |
| Staging path bytes | 80 B | `SQ_DEVICE_STAGING_PATH_BYTES` |
| Resource path bytes | 80 B | `SQ_DEVICE_RESOURCE_PATH_BYTES` |
| Install payload bytes | 65536 B | `SQ_DEVICE_INSTALL_MAX_BYTES` |
| Resource install payload bytes | 1048576 B | `MAX_RESOURCE_BYTES` |
| Serial frame budget | 1024 B | `SQ_SERIAL_MAX_FRAME_LEN` |
| FFI storage chunk | 640 B | `SQVM_STORAGE_TRANSFER_CAPACITY` |

`errors-get` deliberately truncates retained diagnostic output to the response
budget and reports `errors_truncated=N` before the retained newest lines. A
larger retained error ring must not make `device errors` fail.

`resources-get` uses compact numeric metric IDs on the constrained device wire
format. Host tooling maps those IDs back to human-readable names.

## Runtime-Tunable Active Caps

The following active caps can be lowered at runtime:

| Key | Bounds | Runtime field |
| --- | --- | --- |
| `vm_runtime.timer_max` | `0 < N <= SQ_VM_RUNTIME_TIMER_MAX` | `active_timer_max` |
| `vm_runtime.armed_timer_max` | `0 < N <= SQ_VM_RUNTIME_ARMED_TIMER_MAX` | `active_armed_timer_max` |
| `vm_runtime.input_button_max` | `0 < N <= SQ_VM_RUNTIME_INPUT_BUTTON_MAX` | `active_input_button_max` |
| `vm_runtime.active_binding_max` | `0 < N <= SQ_VM_RUNTIME_ACTIVE_BINDING_MAX` | `active_binding_max` |
| `vm_runtime.output_max` | `0 < N <= SQ_VM_RUNTIME_OUTPUT_MAX` | `active_output_max` |
| `vm_runtime.drawlog_max` | `0 < N <= SQ_VM_RUNTIME_DRAWLOG_MAX` | `active_drawlog_max` |

Overrides are stored as SQDC integer records in the firmware app-store system
file `/sq/system/runtime.sqdc` on the current Zephyr reference mount. Code
should derive this path with `sq_app_store_runtime_config_path(...)`; portable
docs and host tooling should describe it as firmware-owned runtime config, not
as an app-visible filesystem contract.

On `sq_vm_runtime_init`, firmware initializes active caps to the hard caps and
then loads runtime config when a store mount is attached. A missing config file
uses the hard caps. Invalid explicit loads return an error; automatic init
keeps hard defaults so corrupt runtime config does not prevent firmware startup.

Setting an active cap validates the key and value, rejects values above the
hard cap with `-ERANGE`, and rejects values below currently active entries with
`-EBUSY`. Clearing a key restores that key to the hard cap. Clearing all keys
restores all active caps to hard caps and removes the runtime config file when
there are no non-default records left.

Device protocol opcodes:

| Opcode | Name | Payload |
| ---: | --- | --- |
| 82 | `runtimecapget` | optional string field `1` key |
| 83 | `runtimecapset` | string field `1` key, u32 field `2` value |
| 84 | `runtimecapclear` | optional string field `1` key |

CLI:

```sh
cargo run -p squidc -- device runtime-cap get
cargo run -p squidc -- device runtime-cap get vm_runtime.timer_max
cargo run -p squidc -- device runtime-cap set vm_runtime.timer_max 2
cargo run -p squidc -- device runtime-cap clear vm_runtime.timer_max
cargo run -p squidc -- device runtime-cap clear
```

`device resources` reports both static and active cap metrics using names such
as `cap.static.timer`, `cap.active.timer`, and `cap.static.device_error`.

## Changing A Hard Cap

If a target hard cap changes:

1. Edit the selected profile under `targets/runtime-limits/`.
2. Run `scripts/generate-zephyr-target-kconfig.py` to regenerate checked-in
   target Kconfig fragments.
3. Run `scripts/generate-runtime-limits-header.py` if the profile fallback
   values should change.
4. Update the matching row in this document.
5. Run the relevant metadata drift tests and firmware ztests.
6. For firmware-impacting caps, run the relevant hardware check when hardware
   is available.

Update `docs/language_spec.md` when the cap is app-author-visible.
