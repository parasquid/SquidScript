# SquidScript Runtime Limits

This document is the agent- and app-author-facing entry point for the
bounded runtime resource caps enforced by the ESP32-C3 Zephyr firmware.
The build-time tuning source is
`firmware/zephyr/runtime_limits.json`; the C headers consume the
generated `firmware/zephyr/src/runtime_limits.h` (regenerate with
`scripts/generate-runtime-limits-header.py`). The C headers that own the
backing arrays (`vm_runtime.h`, `app_store.h`, `device_protocol.h`,
`serial_transport.h`) use `#ifndef` guards so the C defaults still build
standalone if the generated header is missing. The caps are also
documented in `docs/firmware_build_architecture.md` (per-target build
budget context) and in `docs/language_spec.md` (author-facing phrasing
of the same caps).

## Foreground app session limits (per active app)

The active foreground app has a single per-session runtime instance. The
following caps bound the resources one app can hold at once.

| Limit | Value | Macro |
| --- | --- | --- |
| Foreground timer slots | 4 | `SQ_VM_RUNTIME_TIMER_MAX` |
| Armed (background) timer slots | 2 | `SQ_VM_RUNTIME_ARMED_TIMER_MAX` |
| Active device-binding slots | 3 | `SQ_VM_RUNTIME_ACTIVE_BINDING_MAX` |
| Input button slots | 8 | `SQ_VM_RUNTIME_INPUT_BUTTON_MAX` |
| Event name max length | 23 UTF-8 bytes (+ NUL) | `SQ_VM_RUNTIME_EVENT_LEN` |
| Trace record slots | 4 | `SQ_VM_RUNTIME_TRACE_MAX` |
| Trace record length | 26 bytes | `SQ_VM_RUNTIME_TRACE_LEN` |
| Output line slots | 6 | `SQ_VM_RUNTIME_OUTPUT_MAX` |
| Output line length | 54 bytes | `SQ_VM_RUNTIME_OUTPUT_LEN` |
| Drawlog record slots | 4 | `SQ_VM_RUNTIME_DRAWLOG_MAX` |
| Drawlog record length | 48 bytes | `SQ_VM_RUNTIME_DRAWLOG_LEN` |

## BLE object-receive profile table

| Limit | Value | Macro |
| --- | --- | --- |
| Registered BLE profile entries | 2 | `SQ_VM_RUNTIME_BLE_PROFILE_MAX` |

The BLE profile routing table is a static array of
`SQ_VM_RUNTIME_BLE_PROFILE_MAX` entries (~640 bytes each, ~1.25 KiB
total). Profiles are registered imperatively when an app runs
`service.ble.start` (one receive per app, set/replace). The natural upper
bound is `SQ_APP_STORE_MAX_APPS = 8` but the runtime cap of 2 is tighter
and matches the single-session GATT policy (one in-flight transfer at a
time across all profiles). Registering a third profile returns `-EINVAL`
to the VM (verified by `firmware/zephyr/tests/ble-trigger-table/src/main.c`).

`service.timer.every(...)`, `service.timer.after(...)`, and `app.triggers`
all share the foreground or armed timer caps. Registering one beyond the
cap returns `-ENOSPC` to the VM (verified by
`firmware/zephyr/tests/protocol/src/main.c:3146` for foreground timers).

## App store limits

| Limit | Value | Macro |
| --- | --- | --- |
| Installed apps | 8 | `SQ_APP_STORE_MAX_APPS` |
| App file path | 64 bytes | `SQ_APP_STORE_APP_FILE_PATH_MAX` |
| App state path | 60 bytes | `SQ_APP_STORE_APP_STATE_PATH_MAX` |
| Device-config path | 40 bytes | `SQ_APP_STORE_DEVICE_CONFIG_PATH_MAX` |

## Wire-format limits

| Limit | Value | Macro |
| --- | --- | --- |
| Device response bytes | 1088 B | `SQ_DEVICE_RESPONSE_BYTES` |
| Resource path bytes | 80 B | `SQ_DEVICE_RESOURCE_PATH_BYTES` |
| Serial frame budget | 256 B | `SQ_SERIAL_MAX_FRAME_LEN` |
| FFI storage chunk | 640 B | `SQVM_STORAGE_TRANSFER_CAPACITY` |

## Adding or changing a cap

The JSON is authoritative. If a change here is needed:

1. Edit `firmware/zephyr/runtime_limits.json` to set the new value.
2. Run `scripts/generate-runtime-limits-header.py` to regenerate
   `firmware/zephyr/src/runtime_limits.h`. Commit the regenerated
   header alongside the JSON change.
3. Measure the new linker DRAM on both targets with
   `scripts/zephyr-ram-audit.sh`; update the explicit `239,472`-class
   reference numbers in `docs/firmware_build_architecture.md` if they
   appear in the build-budget narrative.
4. Update the matching row in this table.
5. Verify the real behavior test still passes
   (`firmware/zephyr/tests/protocol/src/main.c` exercises the caps
   end-to-end; `scripts/zephyr-test-protocol.sh` runs the suite).
6. Update `docs/language_spec.md` if the cap is author-visible.

## Runtime-Tunable Overrides (Design)

The JSON is build-time. For some caps, users want to lower the cap at
runtime without rebuilding the firmware (e.g., a deployment that knows it
only ever needs 2 timers can save 80 B of `runtime.4` by lowering the
active count from 4 to 2, or a target that wires 3 buttons can keep the
build-time 8-slot cap and override down to 3 to reduce per-poll work).

This section is the design for runtime-tunable cap overrides. It is
**not yet implemented**; the design specifies the storage, boot
application, sizing strategy, validation, wire surface, and migration so
the implementation can be a clean follow-up slice.

### Storage

A new SQDC file at `/device/runtime.sqdc` holds the overrides. It uses
the same SQDC binary format as `/device/active.sqdc` (records keyed by
`section.key`) but a separate file so the firmware-owned active
configuration and the user-tunable runtime overrides don't share a
write path.

| Key | Type | Bounds | Meaning |
| --- | --- | --- | --- |
| `vm_runtime.timer_max` | u8 | `0 < N <= SQ_VM_RUNTIME_TIMER_MAX` | Active foreground timer slots |
| `vm_runtime.armed_timer_max` | u8 | `0 < N <= SQ_VM_RUNTIME_ARMED_TIMER_MAX` | Active armed timer slots |
| `vm_runtime.input_button_max` | u8 | `0 < N <= SQ_VM_RUNTIME_INPUT_BUTTON_MAX` | Active input button slots |
| `vm_runtime.active_binding_max` | u8 | `0 < N <= SQ_VM_RUNTIME_ACTIVE_BINDING_MAX` | Active device-binding slots |
| `vm_runtime.output_max` | u8 | `0 < N <= SQ_VM_RUNTIME_OUTPUT_MAX` | Active output line slots |
| `vm_runtime.drawlog_max` | u8 | `0 < N <= SQ_VM_RUNTIME_DRAWLOG_MAX` | Active drawlog record slots |

Caps **not** tunable at runtime (would change storage layout, not just
behavior):

- `SQ_VM_RUNTIME_EVENT_LEN` (sizes `event[24]` arrays in every timer/input/trigger slot)
- `SQ_VM_RUNTIME_OUTPUT_LEN`, `SQ_VM_RUNTIME_DRAWLOG_LEN`, `SQ_VM_RUNTIME_TRACE_LEN`, `SQ_VM_RUNTIME_DEVICE_ERROR_LEN` (string widths)
- All `SQ_VM_RUNTIME_WIFI_*` profile path / IP buffers (sized for fixed app/CLI message shapes)
- `SQ_APP_STORE_*` (app store, app ID, path caps)
- `SQ_DEVICE_RESPONSE_BYTES` and other protocol framing
- `SQ_SERIAL_MAX_FRAME_LEN`
- `SQVM_*` (FFI ABI, must match Rust side)

The JSON `runtime_limits.json` is the **build-time maximum**. The
`/device/runtime.sqdc` file is the **runtime active count**. The
firmware applies the override as a soft cap; the JSON hard cap is never
exceeded.

### Sizing Strategy

Each tunable cap gets a paired `runtime->active_*_max` field in the
runtime struct, alongside the existing `runtime->*_count` field:

| Struct field | Backing array (sized by JSON) | Active count (sized by override) |
| --- | --- | --- |
| `runtime->active_timer_max` | `runtime->timers[SQ_VM_RUNTIME_TIMER_MAX]` | `runtime->timer_count` |
| `runtime->active_armed_timer_max` | `runtime->armed_timers[SQ_VM_RUNTIME_ARMED_TIMER_MAX]` | `runtime->armed_timer_count` |
| `runtime->active_input_button_max` | `runtime->input_buttons[SQ_VM_RUNTIME_INPUT_BUTTON_MAX]` | `runtime->input_button_count` |
| `runtime->active_binding_max` | `runtime->active_bindings[SQ_VM_RUNTIME_ACTIVE_BINDING_MAX]` | `runtime->active_binding_count` |

The JSON-sized array is the **memory budget**. The runtime active count
is the **behavioral cap**. Registration paths that currently check
`count < SQ_VM_RUNTIME_*_MAX` change to `count < runtime->active_*_max`.
The loop bounds that iterate over the JSON-sized array (e.g.,
`for (i = 0; i < SQ_VM_RUNTIME_TIMER_MAX; i++)` to scan the timer table
for the next-due timer) stay as-is — they walk the full backing
storage, but skip slots beyond `active_*_max` when checking candidates.

### Boot Application

On boot, after `sq_vm_runtime_init`:

1. Open `/device/runtime.sqdc` from the runtime-owned storage.
2. If absent, set every `runtime->active_*_max` to the JSON-sized max
   (current behavior).
3. If present, parse each record. For each key:
   - If the value is `<= 0` or `> JSON max`, log a warning, set to JSON
     max, continue.
   - If the value is `< runtime->*_count` (i.e., would orphan currently
     active entries — for the boot case the counts are zero, so this
     branch is unreachable at boot but matters for runtime writes), log
     a warning, set to JSON max, continue.
   - Otherwise, set `runtime->active_*_max` to the override value.
4. The override is applied for the lifetime of the runtime instance.
   Changes to `/device/runtime.sqdc` after boot take effect on the next
   `sq_vm_runtime_init` (next foreground app launch), not on the
   current one.

### Validation

Two failure modes the runtime must reject:

1. **Out-of-range override** — `value > JSON max` or `value == 0`.
   Firmware keeps the JSON max, logs a retained diagnostic with the
   offending key and value. No crash, no silent partial application.
2. **Runtime write that would orphan entries** — when a host writes
   a new value through the device protocol (see wire surface below),
   if `runtime->*_count > proposed_value`, the write is rejected with
   a clear protocol error and the current value is preserved.

### Wire Surface

Read:

- `device resources` already reports `SQ_VM_RUNTIME_*_MAX` in the
  `runtime.static_buffers` resource. Add a new `runtime.active_caps`
  resource reporting the current `active_*_max` values (or the JSON max
  if no override is loaded). The CLI surfaces this in `device
  resources` output.

Write:

- Add a new device protocol op `runtime cap set` with the payload
  shape `<key_len> <key> <value>` (e.g., `vm_runtime.timer_max 2`).
  Returns `0` on success, a clear `EINVAL`/`ENOSPC` error otherwise.
- The CLI gets `squidc device runtime-cap get <key>` and `squidc
  device runtime-cap set <key> <value>` (parallel to `device
  wifi-profile`). Writing a value persists it to
  `/device/runtime.sqdc` and reports success; the change applies on
  the next runtime init (next app launch), not immediately.

The write path is bounded:

- A bad key (not in the tunable list) returns `EINVAL`.
- A bad value (`0` or `> JSON max`) returns `ERANGE` and leaves the
  file untouched.
- A value `< current count` (would orphan) returns `EBUSY` and leaves
  the file untouched. The host can stop the foreground app first, then
  write the new cap.

### Migration

Devices with no `/device/runtime.sqdc` continue to use the JSON max.
This is the current behavior — no migration is required for the
existing fleet.

Devices that want a smaller active cap can create the file via the
CLI: `squidc device runtime-cap set vm_runtime.timer_max 2` writes
the record, persists to `/device/runtime.sqdc`, and the next app
launch applies it. To remove an override, `squidc device runtime-cap
clear` (or per-key `clear <key>`) deletes the file or record and the
next app launch falls back to the JSON max.

### Implementation Sketch

The implementation is a follow-up slice with TDD:

1. **Failing ztest first** — exercise the boot-time apply path with a
   fixture SQDC, verify the runtime uses the override. Then the
   out-of-range and orphan-rejection paths.
2. **Storage** — add `sq_vm_runtime_load_runtime_caps` and
   `sq_vm_runtime_save_runtime_caps` (parallel to
   `sq_vm_runtime_device_config_load/save`). The runtime struct gets
   the `active_*_max` fields.
3. **Boot hook** — call `sq_vm_runtime_load_runtime_caps` from
   `sq_vm_runtime_init` after the existing init. Failure (no file,
   parse error, validation failure) is non-fatal; fallback is JSON max.
4. **Registration gates** — change `count < SQ_VM_RUNTIME_*_MAX`
   checks in `runtime_activate_input_button`,
   `runtime_activate_binding`, `sq_vm_runtime_register_timer`, and the
   output/drawlog append paths to use `active_*_max` instead.
5. **Protocol op** — add `SQ_OPCODE_RUNTIME_CAP_SET` and
   `SQ_OPCODE_RUNTIME_CAP_GET`. Add a U16 value type for the
   `runtime.active_caps` resource metric.
6. **CLI** — `squidc device runtime-cap get/set/clear`.
7. **Docs** — add the runtime cap section to `docs/squidc_cli.md` and
   the runtime override path to `docs/developer_repl_protocol.md`. The
   `docs/runtime_limits.md` table gets a "Runtime tunable?" column.

### Open Questions

1. Should runtime cap changes be ARMED-app-aware? An armed app might
   reference `SQ_VM_RUNTIME_RETURN_STACK_MAX` indirectly through its
   planned-sleep record. If the override lowers the return stack max
   below the active return stack depth, the next planned-sleep restore
   could be lossy. The simplest rule is: only allow lowering to a
   value `>= max(current_count, any persisted sleep state depth)`.
2. Should the CLI show the override and the JSON max side-by-side, or
   only the active value? Both are useful for debugging.
3. Should the device protocol `runtime.active_caps` report be
   opt-in (only when an override is loaded) or always-on? Always-on is
   simpler and the field is small (≤ 6 bytes).
4. Does the active cap need to be exposed in the app's
   `system.info()` or similar, or is the device protocol enough? The
   app doesn't need to know its own cap to behave correctly, but
   diagnostics might.
