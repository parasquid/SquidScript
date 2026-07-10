# SquidScript Runtime Limits

This document is the app-author-facing reference for bounded runtime resources.
Platform-neutral hard caps live in the `squidvm-limits` crate. Target JSON owns
physical and target-specific budgets such as display geometry, gesture timing,
and radio buffers. Protocol-specific wire limits live in
`squid-device-protocol`.

Hard caps size fixed storage and are not runtime-tunable. Native firmware must
import these constants instead of copying their numeric values.

## App And Lifecycle Limits

| Limit | Hard value | Source |
| --- | ---: | --- |
| Installed apps | 8 | `MAX_INSTALLED_APPS` |
| Process stack entries | 2 | `MAX_PROCESS_STACK` |
| Foreground timer slots | 4 | `MAX_FOREGROUND_TIMERS` |
| Armed timer registrations | 2 | `MAX_ARMED_TIMERS` |
| Armed input registrations | 8 | `MAX_ARMED_INPUTS` |
| Pending timer/input events | 8 | `MAX_PENDING_EVENTS` |
| Physical logical-button slots | 8 | `MAX_INPUT_BUTTONS` |
| Event name length | 24 UTF-8 bytes | `MAX_EVENT_NAME_BYTES` |
| App ID length | 40 UTF-8 bytes | `MAX_APP_ID_BYTES` |
| Saved state | 512 bytes | `MAX_SAVED_STATE_BYTES` |
| SQBC app image | 8192 bytes | `MAX_APP_BYTES` |

`service.timer.every(...)` and `service.timer.after(...)` share the foreground
timer cap. Armed timer and input registrations are separate resources. Timer
and input producers share the pending-event queue; overflow drops the newest
event and records a device diagnostic. Event dispatch remains ordered and does
not invoke a second VM job reentrantly.

An app ID or event name at the stated byte length is valid. These are UTF-8
byte limits, not character counts.

## Compiler And VM Limits

The remaining compiler and VM structural caps are also exported by
`squidvm-limits`, including instruction count, function count, trigger count,
string-pool bytes, runtime records, and runtime list items. Code that consumes
SQBC must use those exported constants so compiler and firmware validation stay
aligned.

## Device Protocol Limits

| Limit | Hard value | Source |
| --- | ---: | --- |
| App ID length | 40 bytes | `MAX_APP_ID_LEN` importing `MAX_APP_ID_BYTES` |
| Install transfer | 65536 bytes | `squid-device-protocol::MAX_APP_BYTES` |
| Resource transfer | 1048576 bytes | `MAX_RESOURCE_BYTES` |
| Resource path length | 128 bytes | `MAX_PATH_LEN` |

The install-transfer cap is a transport bound. Firmware still rejects an SQBC
image above the VM's 8192-byte `MAX_APP_BYTES` cap. Protocol activity reports
explicit size or field errors when a value exceeds its owning cap. `RUN.TEMP`
uses the VM image-size limit and remains RAM-backed.

## Native Upload Buffers

| Limit | Hard value | Source |
| --- | ---: | --- |
| SQBC upload profiles | 16 | `MAX_UPLOAD_PROFILES` |
| Native X4 HTTP body/storage chunk | 512 bytes | `BODY_BUF` and `UPLOAD_STAGE_CHUNK_BYTES` |
| Native X4 BLE transfer chunk | 192 bytes | `BLE_PIPELINE_CHUNK_BYTES` |
| Native X4 queued BLE chunks | 4 | `BLE_PIPELINE_DEPTH` |
| Native X4 BLE transfer-buffer budget | 2048 bytes | `BLE_PIPELINE_BUFFER_BUDGET_BYTES` |
| Native BLE connection inactivity | 30000 ms by default | `firmware.native.bleConnectionWatchdogMs` |

Upload storage is bounded independently of total file size. HTTP streams in
fixed chunks. BLE backpressure delays acknowledgement when its fixed queue is
full. The BLE inactivity watchdog is a connection-liveness bound; protocol
activity resets it.

## Changing A Hard Cap

1. Change the owning constant in `squidvm-limits`, target JSON, or
   `squid-device-protocol`.
2. Update every fixed allocation to import that constant.
3. Update this document when the cap is app-author-visible.
4. Run compiler, VM, protocol, firmware-core, X4, CLI, and target-build tests
   affected by the change.
5. For firmware-impacting caps, run the relevant X4 hardware gate.
