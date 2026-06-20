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
| Retained display ops per screen | 48 | `SQ_VM_RUNTIME_DISPLAY_OP_MAX` | no |
| Retained device error slots | 8 | `SQ_VM_RUNTIME_DEVICE_ERROR_MAX` | no |
| Device error line length | 48 bytes | `SQ_VM_RUNTIME_DEVICE_ERROR_LEN` | no |
| Content library page entries | 8 | `SQ_VM_RUNTIME_CONTENT_LIST_MAX` | no |
| Content library ref length | 128 bytes | `SQ_VM_RUNTIME_CONTENT_REF_LEN` | no |

`service.timer.every(...)`, `service.timer.after(...)`, and `app.triggers`
share the foreground or armed timer caps. Registering one beyond the active cap
returns `-ENOSPC` to the VM. Physical input sampling can enqueue up to
`SQ_VM_RUNTIME_INPUT_EVENT_QUEUE_MAX` press events while the foreground VM is
busy; overflow drops the newest input event and records `input_queue_overflow`
in device errors. Display flushes run on a separate bounded worker so input and foreground event dispatch can continue while the e-paper controller is busy.

`content.binbook.list(...)` materializes at most one content page of
`SQ_VM_RUNTIME_CONTENT_LIST_MAX` entries per call. Each `ref` is an opaque
logical identifier bounded by `SQ_VM_RUNTIME_CONTENT_REF_LEN`.

`binbook.chapters(...)` materializes at most one chapter page of
`SQ_VM_RUNTIME_CONTENT_LIST_MAX` entries per call. Chapter titles are copied
from BinBook `CHAPTER_INDEX` records into fixed per-entry firmware buffers
bounded by `SQ_VM_RUNTIME_CONTENT_NAME_LEN`. `binbook.chapter(...)` reads one
chapter entry and uses one of the same bounded title buffers.

## BLE File-Transfer Profile Table

| Limit | Hard value | Macro |
| --- | ---: | --- |
| Registered BLE profile entries | 2 | `SQ_VM_RUNTIME_BLE_PROFILE_MAX` |

The BLE profile routing table is a static array of
`SQ_VM_RUNTIME_BLE_PROFILE_MAX` entries. Each entry stores the installed-app
registry slot, the app-local profile instance id, accepted file extensions, and
the complete-event name. App-id text lives in the app registry and is resolved
only at dispatch or path-construction boundaries.

Profiles are registered imperatively when an installed foreground app or the
target fallback app runs `service.ble.start("file-transfer", ...)`; one
file-transfer profile per app is set or replaced. Temp-run apps cannot register
BLE receive profiles because they do not have a stable app slot. The natural
upper bound is `SQ_APP_STORE_MAX_APPS = 8` plus the reserved fallback slot, but
the runtime cap of 2 is tighter and matches the current single-session GATT
policy.

The route table must resolve an uploaded extension to exactly one receiver. If
two active routes accept the same extension, or if a route points at a registry
slot that no longer resolves, firmware records an `invariant.ble.*` diagnostic
through `device errors` and rejects the transfer instead of selecting an
arbitrary receiver.

## HTTP Upload Profile Metadata

| Limit | Hard value | Macro |
| --- | ---: | --- |
| HTTP profile text field length | 32 bytes | `SQVM_HTTP_PROFILE_TEXT_CAP` |
| Accepted HTTP upload extensions | 4 | `SQVM_HTTP_PROFILE_ACCEPT_MAX` |
| HTTP upload event routes | 8 | `SQVM_HTTP_PROFILE_EVENT_MAX` |
| HTTP upload socket-to-storage chunk buffer | 2048 bytes | `SQ_HTTP_UPLOAD_CHUNK_MAX` |

The current HTTP upload runtime supports one active foreground route. Profile
metadata uses the same compact field caps as BLE profile metadata. The active
route stores the app id, profile id, accepted extensions, and completion event;
the HTTP request body streams to storage through the fixed upload chunk buffer
rather than app RAM.

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
