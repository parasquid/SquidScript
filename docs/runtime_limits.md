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
| Input button slots | 2 | `SQ_VM_RUNTIME_INPUT_BUTTON_MAX` |
| Event name max length | 23 UTF-8 bytes (+ NUL) | `SQ_VM_RUNTIME_EVENT_LEN` |
| Trace record slots | 4 | `SQ_VM_RUNTIME_TRACE_MAX` |
| Trace record length | 26 bytes | `SQ_VM_RUNTIME_TRACE_LEN` |
| Output line slots | 6 | `SQ_VM_RUNTIME_OUTPUT_MAX` |
| Output line length | 54 bytes | `SQ_VM_RUNTIME_OUTPUT_LEN` |
| Drawlog record slots | 4 | `SQ_VM_RUNTIME_DRAWLOG_MAX` |
| Drawlog record length | 48 bytes | `SQ_VM_RUNTIME_DRAWLOG_LEN` |

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
