# BLE Object Transfer — Runtime Design

**Status**: design, revised after review (slice 1)
**Date**: 2026-06-05
**Slice**: continues from `2fb64cb` (BLE object transfer trigger metadata)

## Context

`2fb64cb` (June 1) completed the metadata/handler-payload slice: parser, semantic validation, SQBC section 10 (`SECTION_BLE_TRIGGERS`), Rust FFI exports (`sqvm_trigger_ble_profile_count_from_reader` / `sqvm_trigger_ble_profile_read_from_reader`), and the seven payload field names whitelisted in `sqvm_dispatch_start_resumable_with_payload` for the eventual `ble.object.*` event.

The June metadata slice never wired a C-side caller for `sqvm_dispatch_start_resumable_with_payload` (`firmware/zephyr/src/vm_runtime.c:448` still uses `sqvm_dispatch_start_resumable`). No GATT/OTS service is registered, no staging-to-event handoff exists, and the host-side test driver is missing. This slice delivers the runtime path on the device, the host test driver, and a verification on ESP32-C3 hardware.

The spec was reviewed in detail; 14 gaps were identified. The architecture was sharpened: the firmware becomes a generic file-transfer primitive, and the SquidScript app handles policy (including install) through a new `app.install(file_ref)` builtin.

## Goals

1. **Firmware = file transfer primitive.** Stream BLE chunks to a LittleFS staging file at `/sq/tmp/ble-object-<app_id>-<profile_id>.tmp`. On transfer complete, deliver a `file.*` reference to the armed app via `ble.object.complete`. The firmware does no SQBC validation, no install, no CRC32.
2. **App = policy.** The armed SquidScript app decides what to do with the file. The slice adds `app.install(file_ref)` so an app can install a received file, but the firmware stays generic.
3. **Standard GATT OTS** (Bluetooth SIG UUID 0x1825), minimal OTS, L2CAP CoC for data. Interoperable with nRF Connect, LightBlue, and other OTS-capable clients.
4. **Event dispatch through the existing `app.arm` mechanism** — no BLE-specific special case, no new lifecycle path.
5. **Verify on ESP32-C3 hardware** with skip behavior when host Bluetooth is unavailable.

## Non-goals (out of scope for this slice)

- Per-app uninstall opcode. No `AppUninstall` exists; left as a roadmap follow-up.
- OTS Directory Listing, multiple objects, OTS-level Checksum characteristic, OTS OLCP. Minimal OTS only.
- `role != "server"`. Only the server role ships.
- Firmware-side SQBC validation, CRC32, or install. The app decides; the firmware stays generic.
- `sink` field in the trigger metadata. Sinks were a firmware-side abstraction; the app handles dispatch now.
- Non-standard OACP Create extensions. The standard OACP Create payload is unchanged; we do not smuggle `expected_crc32` or other side-channels through it.
- OTS client role. Only the server role ships.
- HTTP-upload parity. The spec rule "follow the same installer rules as HTTP uploads" is intent, not implementation.

## Locked-in decisions

| # | Decision | Rationale |
| --- | --- | --- |
| 1 | Standard GATT OTS profile (Bluetooth SIG UUID 0x1825) | Interoperable with nRF Connect, LightBlue, and other OTS-capable phone apps |
| 2 | Minimal OTS: OACP Create/Write/Execute/Abort, L2CAP CoC, single Current Object, OTS Feature bits Create/Write/Execute/Abort | Smaller device footprint; no Delete/Checksum/Patch/Read on the server side |
| 3 | Zephyr `bt_ots` module with custom callbacks (`obj_name_written`, `obj_created`, `obj_write`) | Library-quality seam; tested upstream implementation; we only add app glue |
| 4 | New `firmware/zephyr/src/ble_ots.c` parallel to `ble_smoke.c` | Small, well-bounded units; independent native ztests |
| 5 | Object Name carries `<app_id>/<profile_id>/<extension>` | Client-driven routing; the third segment is the file *extension* (e.g. `.sqbc`), not a filename |
| 6 | Reject when client doesn't follow convention; runtime owns wire-format validation, app does semantic object-type confirmation | Clean separation of concerns |
| 7 | Ephemeral staging: firmware `fs_unlink`s the staging file after the `ble.object.complete` event handler returns | App must consume the file (e.g., via `app.install(file_ref)`) before returning; bounded filesystem, no orphans |
| 8 | `SQ_VM_RUNTIME_BLE_PROFILE_ARMED_MAX = 2` (single global cap, no per-app declared cap) | Matches `SQ_VM_RUNTIME_ARMED_TIMER_MAX = 2`; natural ceiling via `SQ_APP_STORE_MAX_APPS` |
| 9 | All-or-nothing per-app arming | If arming would push the table over the cap, the whole arm fails |
| 10 | Python + bleak test driver, L2CAP CoC only (no GATT-writes fallback) | Cross-platform (Linux/macOS/Windows); mainstream toolchain; standard OTS |
| 11 | Skip pattern follows `scripts/c3-supermini-test-ble-smoke.sh:100-113` | Mirrors the existing BLE skip pattern; "OK ... skipped" log line + exit 0 |
| 12 | Event dispatch follows the existing `app.arm` mechanism exactly | No BLE-specific special case; the armed app launches into the foreground |
| 13 | Emit `ble.object.error` on disconnect mid-stream with `error: "client-abort"` | Observability for client-side aborts |
| 14 | OACP Create validation chain rejects malformed names/extensions with `BT_GATT_OTS_OACP_RES_INV_PARAM` | Standard OTS response; we surface semantic reasons in the disconnect-time `ble.object.error` payload for app-level observability |
| 15 | Single in-flight OTS session globally; second `OACP Create` while busy → `BT_GATT_OTS_OACP_RES_OBJ_LOCKED` | Matches the BT_OTS_OBJ_PROP_* model; simpler than multi-session state |
| 16 | `app.install(file_ref, appId)` builtin (new) | App-side install entry point; firmware owns SQBC format knowledge, app triggers install |
| 17 | `CONFIG_BT_OTS=y` + `CONFIG_BT_OTS_OACP_{CREATE,WRITE,EXECUTE}_SUPPORT=y` + `CONFIG_BT_OTS_OBJ_NAME_WRITE_SUPPORT=y` are the Kconfig surface | Auto-selects `BT_L2CAP_DYNAMIC_CHANNEL`, `BT_GATT_DYNAMIC_DB`, `BT_SMP`, `EXPERIMENTAL` |
| 18 | Threading: OTS callbacks (BT context) populate a single-slot pending event; the main poll path drains it | The existing `app.arm` path is single-threaded; we cannot call `sq_vm_runtime_submit_work()` from a BT callback without a producer→consumer handoff |

## Architecture (Zephyr side)

```
firmware/zephyr/src/                              firmware/zephyr/src/
  ble_smoke.c                   ble_ots.c             device_protocol.c
  (advertising,   <---------->  (OTS GATT,    <----->  register_app_ble_profile_triggers()
   disconnect                     OACP,                reads SQBC section 10 via
   restart state                  L2CAP CoC,            sqvm_trigger_ble_profile_*_from_reader,
   machine)                       Object Name          populates sq_ble_profile_table
                                  validation,           called on app.arm(appId)
                                  pending-event        cleared on app.disarm(appId)
                                  slot, fs_unlink            |
                                  plumbing)                     v
                                                               app_store.c
                                                               sq_app_store_install_from_file_ref()
                                                               (new: validates SQBC magic,
                                                                calls sq_app_store_install_app,
                                                                updates app registry)
```

`ble_smoke.c` continues to manage advertising + disconnect-restart. On a successful disconnect, it calls `sq_ble_ots_reset_session()` to clear in-flight OTS state and `fs_unlink` any open staging file. The OTS GATT service is **always registered** in the GATT database. The OTS service UUID is included in advertising data only when `sq_ble_ots_armed_count() > 0`; when no app is armed, the OTS UUID is omitted from the advertisement (clients that scan for it won't see the service), but a connected client that probes the GATT database directly will find the service and any OACP Create attempt will be rejected with `BT_GATT_OTS_OACP_RES_INV_PARAM` (or `_OBJ_LOCKED` if mid-transfer) by `obj_created` because the dispatch table is empty.

## Trigger table

```c
// In firmware/zephyr/src/runtime_limits.h
#define SQ_VM_RUNTIME_BLE_PROFILE_ARMED_MAX 2

// In firmware/zephyr/src/ble_ots.h
struct sq_ble_profile_entry {
    char app_id[SQ_APP_STORE_APP_ID_MAX];
    char profile_id[SQVM_BLE_PROFILE_TEXT_CAP];
    char accept_exts[SQVM_BLE_PROFILE_ACCEPT_MAX][SQVM_BLE_PROFILE_TEXT_CAP];
    uint8_t accept_count;
    SqvmBleProfileEventRoute events[SQVM_BLE_PROFILE_EVENT_MAX];
    uint8_t event_count;
};

static struct sq_ble_profile_entry sq_ble_profile_table[SQ_VM_RUNTIME_BLE_PROFILE_ARMED_MAX];
static size_t sq_ble_profile_table_count;
```

(`app_id[SQ_APP_STORE_APP_ID_MAX]` matches the existing `app_lifecycle.h:23` convention; the previous revision's `+ 1` was a typo.)

**Lifecycle hooks** in `device_protocol.c`:

| Event | Action |
| --- | --- |
| `app.arm(appId)` | `register_app_ble_profile_triggers(appId)` reads the app's SQBC section 10 via `sqvm_trigger_ble_profile_count_from_reader` / `sqvm_trigger_ble_profile_read_from_reader`, validates `count ≤ SQ_VM_RUNTIME_BLE_PROFILE_ARMED_MAX - current_count`, and appends entries. All-or-nothing: if any check fails, no entries are added. |
| `app.disarm(appId)` | `clear_app_ble_profile_triggers(appId)` removes all entries with that `app_id` |
| `Reset` / `StorageFormat` | Clear the entire table; abort any in-flight session; `fs_unlink` the staging file |
| BT connection `BT_CONN_CB` connect | No-op; the table is process-global |
| BT connection `BT_CONN_CB` disconnect | `sq_ble_ots_reset_session()` clears in-flight OTS state; `fs_unlink` the staging file; emit `ble.object.error` with `error: "client-abort"` if a transfer was in progress |

**Lookup API** used by `ble_ots.c`:

```c
const struct sq_ble_profile_entry *sq_ble_profile_lookup(
    const char *app_id, const char *profile_id);
```

**RAM cost**: `SQ_VM_RUNTIME_BLE_PROFILE_ARMED_MAX = 2` × ~640 bytes ≈ 1.25 KiB. The dispatch table is statically sized at the cap. The natural upper bound is `SQ_APP_STORE_MAX_APPS = 8` but the runtime cap of 2 is tighter.

## Spec additions

### `service.ble.profile` options

```squid
app.triggers {
  service.ble.profile("object-transfer", {
    id: "sqbc-install",
    accept: [".sqbc"],
    events: {
      complete: "ble.object.complete",
      error: "ble.object.error"
    }
  })
}
```

The `sink` field proposed in the previous revision is **removed**. The firmware does not care what the file is for; the app decides. The runtime's role is to deliver a `file.*` ref. The app can call `app.install(file_ref)` (if the file is a SQBC) or any other file-handling API.

### Event record fields (`ble.object.complete`)

```text
file            // NEW: a file.* reference, valid only inside the ble.object.complete handler
bytesReceived   // how many bytes were received from the client
totalBytes      // declared Object Size from OACP Create
objectName      // the full Object Name the client wrote
```

Fields removed from the previous revision's draft: `profile`, `id`, `upload`, `crc32`, `error`. Profile/id are dispatch keys, not event data. The `upload` field is gone (the file is ephemeral; no "final path"). `crc32` is the app's concern, not the firmware's. `error` is only on the `ble.object.error` event, not on `complete`.

### `ble.object.error` payload

```text
file            // present if the staging file was open; app can read it before the runtime unlinks
bytesReceived   // bytes received before the failure
totalBytes      // declared size
objectName      // the full Object Name
error           // semantic reason: "client-abort", "staging-fs-error", etc.
```

The OACP Create-time reject reasons (bad name, unknown app, unknown profile, extension not in `accept`, oversize) are sent to the client as standard OTS reject codes (`BT_GATT_OTS_OACP_RES_INV_PARAM` / `_OBJ_LOCKED`) and are **not** re-emitted as `ble.object.error` events — the wire-level reject is the signal.

## OTS protocol flow

The standard Bluetooth SIG OTS flow. Phase order is **Object Name write → OACP Create → L2CAP CoC write → OACP Execute=WRITE**, per the Zephyr OTS API (`target/zephyr/workspace/zephyr/include/zephyr/bluetooth/services/ots.h:604-752` callbacks; `obj_name_written` fires before `obj_created`).

| Phase | Client action | Runtime action | OTS response on failure |
| --- | --- | --- | --- |
| Discovery | GATT discover 0x1825, read OTS Feature | `bt_ots` registered with `BT_OTS_OACP_FEAT_CREATE \| BT_OTS_OACP_FEAT_WRITE \| BT_OTS_OACP_FEAT_EXECUTE \| BT_OTS_OACP_FEAT_ABORT` | n/a |
| Name | Write OTS Object Name = `<app_id>/<profile_id>/<extension>` | `obj_name_written` callback validates the format (3 segments, `is_safe_app_id`, non-empty, extension starts with `.`); sets `obj->metadata.name` to the parsed `app_id` for callback scoping | n/a (write is rejected by Zephyr's GATT layer only on protocol error) |
| Create | OACP Create with Object Size | `obj_created` callback: parse name, look up `(app_id, profile_id)` in dispatch table, validate extension against `accept`, validate size ≤ `SQ_DEVICE_INSTALL_MAX_BYTES`, open staging file at `/sq/tmp/ble-object-<app_id>-<profile_id>.tmp`, set in-flight session state. Returns negative errno from `obj_created` to send a standard OTS reject. | `BT_GATT_OTS_OACP_RES_OPCODE_NOT_SUP` (Create not in features) / `_INV_PARAM` (name format / unknown app / unknown profile / extension mismatch / size > 64 KiB) / `_OBJ_LOCKED` (session already in flight) |
| Stream | L2CAP CoC send data (PSM 0x0025) | `obj_write` callback fires per L2CAP SDU; the Zephyr OTS module validates `offset + len <= alloc` for us; we just `fs_write` to the staging file. If `fs_write` returns a non-recoverable error, we return the negative errno to abort the OTS write procedure. | n/a (L2CAP layer error if the runtime returns negative) |
| Execute | OACP Execute op=WRITE | `obj_write`'s final callback indicates `rem == 0`; we then close the staging file and populate `sq_ble_ots_pending` (see Threading below). The poll path will then launch the armed app and dispatch the event. | `BT_GATT_OTS_OACP_RES_SUCCESS` (event will follow via the pending slot) |
| Abort | OACP Abort (procedure 0x07) | Close staging file, `fs_unlink`, clear in-flight session, do not emit any event. The client has explicitly abandoned the transfer. | n/a |
| Disconnect (mid-stream) | BLE disconnect | `BT_CONN_CB` disconnect path: `fs_unlink` staging file, clear session, populate the pending slot with `is_complete=false` and `error_reason="client-abort"`. The poll path will dispatch `ble.object.error` to the armed app. | n/a (client gone) |

### Object Name parsing and rejection rules

```c
// Returns 0 on success; non-zero (a BT_GATT_OTS_OACP_RES_* code) on failure.
static int parse_ble_object_name(
    const char *name,
    char *app_id_out, size_t app_id_cap,
    char *profile_id_out, size_t profile_id_cap,
    char *extension_out, size_t extension_cap)
{
    // 1. Must contain exactly two '/' characters, splitting into 3 segments
    const char *p = strchr(name, '/');
    if (p == NULL) return BT_GATT_OTS_OACP_RES_INV_PARAM;
    const char *q = strchr(p + 1, '/');
    if (q == NULL) return BT_GATT_OTS_OACP_RES_INV_PARAM;
    if (strchr(q + 1, '/') != NULL) return BT_GATT_OTS_OACP_RES_INV_PARAM;

    // 2. Extract the three segments
    size_t app_len = p - name;
    size_t prof_len = q - (p + 1);
    const char *extension = q + 1;
    size_t extension_len = strlen(extension);

    // 3. None may be empty
    if (app_len == 0 || prof_len == 0 || extension_len == 0)
        return BT_GATT_OTS_OACP_RES_INV_PARAM;

    // 4. app_id passes is_safe_app_id
    if (app_len >= app_id_cap) return BT_GATT_OTS_OACP_RES_INV_PARAM;
    memcpy(app_id_out, name, app_len); app_id_out[app_len] = '\0';
    if (!is_safe_app_id(app_id_out))
        return BT_GATT_OTS_OACP_RES_INV_PARAM;

    // 5. profile_id fits
    if (prof_len >= profile_id_cap) return BT_GATT_OTS_OACP_RES_INV_PARAM;
    memcpy(profile_id_out, p + 1, prof_len); profile_id_out[prof_len] = '\0';

    // 6. extension starts with '.' and fits
    if (extension[0] != '.') return BT_GATT_OTS_OACP_RES_INV_PARAM;
    if (extension_len >= extension_cap) return BT_GATT_OTS_OACP_RES_INV_PARAM;
    memcpy(extension_out, extension, extension_len + 1);

    return 0;
}
```

**OACP Create-time validation chain**:

| Check | OACP reject code | App-side signal |
| --- | --- | --- |
| Name doesn't have exactly 2 `/` segments | `INV_PARAM` | OTS reject only (no event) |
| Any segment empty | `INV_PARAM` | OTS reject only |
| `app_id` fails `is_safe_app_id` | `INV_PARAM` | OTS reject only |
| `app_id` is not in the dispatch table | `INV_PARAM` | OTS reject only |
| `profile_id` is not in that app's armed set | `INV_PARAM` | OTS reject only |
| Extension not in profile's `accept` | `INV_PARAM` | OTS reject only |
| Declared Object Size > `SQ_DEVICE_INSTALL_MAX_BYTES` (64 KiB) | `INV_PARAM` | OTS reject only |
| A transfer is already in flight (single-session policy) | `OBJ_LOCKED` | OTS reject only |

## Staging lifecycle

1. **`obj_created` opens the staging file** at `/sq/tmp/ble-object-<app_id>-<profile_id>.tmp` via a new helper that mirrors `sq_app_store_begin_staged_install` (`firmware/zephyr/src/app_store.c:588-610`) but with the BLE path family. The file is `FS_O_CREATE | FS_O_WRITE` mode.
2. **`obj_write` writes chunks** via `fs_write` (caller-owned buffers from the L2CAP SDU; no buffering in firmware). The Zephyr OTS module guarantees `offset + len <= alloc` for us, so the runtime does not need an offset-overflow check; the size limit was enforced at OACP Create time.
3. **On Execute=WRITE completion** (last `obj_write` callback with `rem == 0`), the runtime closes the staging file and populates `sq_ble_ots_pending` with `is_complete=true` (see Threading below).
4. **The event handler runs from the main poll path.** The staging file's lifetime is the duration of the event handler. After the handler returns (detected via `runtime->lifecycle_phase` going back to `SQ_VM_RUNTIME_LIFECYCLE_IDLE`), the runtime `fs_unlink`s the staging file and clears the pending slot.
5. **On disconnect mid-stream**, `BT_CONN_CB` disconnect path populates `sq_ble_ots_pending` with `is_complete=false` and `error_reason="client-abort"`. The poll path dispatches `ble.object.error` to the armed app; after the handler returns, the runtime `fs_unlink`s the staging file.
6. **On OACP Abort**, the runtime closes + `fs_unlink`s the staging file synchronously in the BT callback context, clears the in-flight session, and does not emit any event. (We could populate the pending slot instead, but the client's intent is "abandon," so silent cleanup is cleaner.)

The ephemeral model is the simplest possible: the app has a narrow window to consume the file, the firmware cleans up, and `/sq/tmp/` doesn't accumulate orphan staging files.

## Event dispatch

### Producer → consumer handoff (the threading seam)

The existing dispatch model is single-threaded:

- `firmware/zephyr/src/vm_runtime.c:146` — `sq_vm_runtime_submit_work()` returns `-EBUSY` if a VM job is already in flight.
- `firmware/zephyr/src/device_protocol.c:1368-1371` — `sq_device_protocol_poll()` drains the armed timer queue from the main loop; lifecycle steps like `SQ_APP_LIFECYCLE_STEP_START_APP` are produced in this same poll.

The OTS callbacks (`obj_created`, `obj_write`, `obj_name_written`, OACP Abort) fire on a different context (BT RX thread / system workqueue, depending on Zephyr's `k_work_submit` dispatch). They cannot directly call `sq_vm_runtime_submit_work()` or `sq_device_protocol_poll()` because:

- `sq_vm_runtime_submit_work()` would race with the main-loop polling path.
- The VM worker thread holds state that the OTS callback must not touch (and vice versa).
- LittleFS operations are not safe to interleave with multiple contexts — the staging file's `fs_write` (in the BT callback) and `fs_unlink` (after the event handler) must be serialized.

The handoff is a single-slot pending event queue:

```c
// In ble_ots.h
struct sq_ble_ots_pending_event {
    bool active;
    char app_id[SQ_APP_STORE_APP_ID_MAX];
    char profile_id[SQVM_BLE_PROFILE_TEXT_CAP];
    char object_name[SQVM_BLE_PROFILE_OBJ_NAME_CAP];
    size_t bytes_received;
    size_t total_bytes;
    char staging_path[SQ_APP_STORE_PATH_MAX];
    bool is_complete;  // true for ble.object.complete, false for ble.object.error
    char error_reason[SQVM_BLE_PROFILE_TEXT_CAP];  // populated only on error
};

static struct sq_ble_ots_pending_event sq_ble_ots_pending;
```

**Producer (BT callback context):**
- `obj_write` final (`rem == 0`): populate the slot, set `active=true`, `is_complete=true`. No `fsync` required (LittleFS is fsync-on-close; we close on the last write).
- `BT_CONN_CB` disconnect (mid-stream): populate the slot, set `active=true`, `is_complete=false`, `error_reason="client-abort"`. Do not `fs_unlink` here — the consumer will do it after the event handler returns.

**Consumer (main poll path, `sq_device_protocol_poll()`):**
- On each iteration, if `sq_ble_ots_pending.active`:
  1. Snapshot the slot into a local; clear the slot (`active=false`).
  2. If `is_complete`, return `SQ_APP_LIFECYCLE_STEP_START_APP` with `app_id` from the slot and `event="ble.object.complete"`. The next poll will pick up the lifecycle step and launch the armed app.
  3. If `!is_complete`, return `SQ_APP_LIFECYCLE_STEP_START_APP` with `app_id` from the slot and `event="ble.object.error"`. The error event fires even if the armed app is no longer foreground (we re-arm the trigger metadata first via the existing planned-resume path).
  4. After the event handler returns (next poll iteration, when `runtime->lifecycle_phase == IDLE`), `fs_unlink(sq_ble_ots_pending.staging_path)` and clear the slot.

This matches the existing `app.arm` mechanism exactly: the armed app is launched as the foreground, the event handler runs, and the lifecycle phase returns to IDLE. The `fs_unlink` is a side-effect of the post-event cleanup, parallel to the planned-resume path's `write_planned_resume_file` (`device_protocol.c:1383`).

### `app.install(fileRef, appId)` builtin (new)

**Spec text** (`docs/language_spec.md` §32, to be added):

```text
app.install(fileRef, appId)
```

- `fileRef`: a `file.*` reference, typically the `file` field of a `ble.object.complete` event.
- `appId`: the destination app ID. Must pass `is_safe_app_id`; the existing app at that ID is replaced.
- Returns success or a structured error.
- The file is consumed atomically: the runtime reads it once, validates the SQBC magic header, and writes it to `/sq/apps/<appId>/main.sqbc`. The original staging file is `fs_unlink`d as part of the function (the BLE ephemeral model already promises cleanup after the event handler returns, so this is consistent).
- The destination app does not get auto-launched. The current app must call `app.launch(appId)` explicitly after `app.install` returns.

Runtime support required: `app.install`.

**C-level implementation** (`firmware/zephyr/src/app_store.c`):

```c
int sq_app_store_install_from_file_ref(
    const char *mount_point,
    const char *app_id,
    const char *staging_path)
{
    if (mount_point == NULL || !is_safe_app_id(app_id) || staging_path == NULL) {
        return -EINVAL;
    }

    // 1. Read the file in 1 KiB chunks (caller-owned buffer on the VM worker stack)
    // 2. Validate the SQBC magic header (first 8 bytes)
    // 3. Call sq_app_store_install_app(mount_point, app_id, bytes, len)
    // 4. Update the app registry
    // 5. fs_unlink the staging file (it's been consumed)
    // 6. Return success or a structured error
}
```

This is a thin wrapper around the existing `sq_app_store_install_app` (`firmware/zephyr/src/app_store.c:557-585`), with the SQBC magic validation added. The function does not compute a CRC32; the SQBC magic is sufficient for "is this a real SQBC file?" The OTS-level `OACP Calculate Checksum` is not exposed (we did not enable `CONFIG_BT_OTS_OACP_CHECKSUM_SUPPORT`).

**VM export** (Rust side, `compiler/rust/crates/squidvm-ffi/src/lib.rs`):
- New `sqvm_app_install_file` C ABI export, registered in `abi/manifest.json`
- Zephyr FFI callback: `runtime_app_install_file` (similar to `runtime_app_armed_stack` at `firmware/zephyr/src/vm_runtime_app_lifecycle.c:144`)

## In-flight session policy

**One transfer at a time across all profiles.** While a transfer is in flight (between `obj_created` and the final `obj_write` or disconnect), a second `OACP Create` is rejected with `BT_GATT_OTS_OACP_RES_OBJ_LOCKED`. The dispatch table may contain 2 armed profiles, but only one can be the active "current object" at a time. The dispatch lookup happens at `obj_created` time using the parsed `app_id`/`profile_id` from the Object Name write.

A second BT connection mid-transfer: same as a single connection (we only have one ACL connection per `CONFIG_BT_MAX_CONN=1`; the second connection would be rejected by Zephyr's BT stack before OACP). Documented as a known limitation; raising `CONFIG_BT_MAX_CONN` is a follow-up.

## OACP Abort semantics

- Client sends `OACP Abort` (procedure 0x07 per `target/zephyr/workspace/zephyr/subsys/bluetooth/services/ots/ots_oacp_internal.h:34`).
- The Zephyr OTS module calls our registered abort handler (or invokes default behavior) to handle the abort. We implement it as: close staging file, `fs_unlink`, clear in-flight session, do not emit any event. The client has explicitly abandoned the transfer; there's no error to surface.
- `Reset` / `StorageFormat` during a transfer: the device protocol handler clears the trigger table, then `sq_ble_ots_reset_session()` (called from the same handler) closes + `fs_unlink`s the staging file. No event emitted (the device is being reset; the app is going away).

## RAM budget

The slice adds:

| Component | Estimated bytes | Source |
| --- | --- | --- |
| `bt_ots` module (server side) | ~8 KiB static | `target/zephyr/workspace/zephyr/subsys/bluetooth/services/ots/*.c` |
| GATT dynamic DB additions | ~1 KiB | `CONFIG_BT_GATT_DYNAMIC_DB=y` (auto-selected by `BT_OTS`) |
| L2CAP CoC TX buffer pool | ~280 B | 1 buffer × `BT_L2CAP_SDU_BUF_SIZE(256)` + `CONFIG_BT_CONN_TX_USER_DATA_SIZE`. `target/zephyr/workspace/zephyr/subsys/bluetooth/services/ots/ots_l2cap.c:36-38` |
| Optional L2CAP CoC RX buffer pool | ~`RX_MTU + 8` B | `ots_l2cap.c:40-43` |
| `sq_ble_ots_pending` slot | ~256 B | New struct, see above |
| `sq_ble_profile_table[2]` | ~1.25 KiB | See Trigger table |
| `app.install` 1 KiB scratch buffer | 1 KiB on VM worker stack | New builtin |
| **Subtotal (firmware additions)** | **~12 KiB** | |
| Existing prj.conf BT caps to revisit | `BT_L2CAP_TX_BUF_COUNT=3`, `BT_CONN_TX_MAX=3` | May need raising to ≥4 for OTS to work; will measure on slice 11 |

**Kconfig surface** (new lines in `firmware/zephyr/prj.conf`):

```kconfig
CONFIG_BT_OTS=y
CONFIG_BT_OTS_OACP_CREATE_SUPPORT=y
CONFIG_BT_OTS_OACP_WRITE_SUPPORT=y
# CONFIG_BT_OTS_OACP_EXECUTE_SUPPORT may need adding; verify in slice 3
CONFIG_BT_OTS_OBJ_NAME_WRITE_SUPPORT=y
# CONFIG_BT_OTS_L2CAP_CHAN_TX_MTU=256 (default)
# CONFIG_BT_OTS_L2CAP_CHAN_RX_MTU=BT_BUF_ACL_RX_SIZE (default)
CONFIG_BT_OTS_MAX_OBJ_CNT=0x02
# CONFIG_BT_OTS_OBJ_MAX_NAME_LEN=120 (default; covers our routing format)
```

`BT_OTS=y` auto-selects `BT_L2CAP_DYNAMIC_CHANNEL`, `BT_GATT_DYNAMIC_DB`, `BT_SMP`, and marks `EXPERIMENTAL` (see `target/zephyr/workspace/zephyr/subsys/bluetooth/services/ots/Kconfig:6-11`).

The 12 KiB adds to the 31,616-byte SquidScript-owned DRAM baseline documented in `ROADMAP.md:212-216`. ESP32-C3 total RAM is 400 KiB. Post-OTS linker RAM is measured in slice 11 and added to the "ESP32-C3 RAM Hardening" section of `ROADMAP.md`.

**Zephyr OTS API status**: marked `[EXPERIMENTAL]` (`ots.h:17`, `Kconfig:7, 11`). Zephyr upstream may change this API; the slice pins a specific Zephyr version via `target/zephyr/workspace/zephyr/west.yml` and notes the API stability risk in the slice 11 RAM verification report.

## Hardware test coverage boundary

The hardware test verifies:

- Compile + flash + boot the XIAO target with the new firmware
- Install `ble-install` via serial (`squidc app install`)
- Arm the app via REPL
- Push a `.sqbc` via BLE (L2CAP CoC)
- Verify the app receives the event (e.g., logs the file size)
- Optionally: call `app.install(file_ref)` and verify a second installed app appears

What is **not** verified on hardware in this slice:

- Object Name format rejections (covered by native ztest only)
- Disconnect mid-stream `ble.object.error` path (covered by native ztest only)
- OACP Abort (covered by native ztest only)
- Single-session `OBJ_LOCKED` rejection (covered by native ztest only)
- Concurrent connections (`CONFIG_BT_MAX_CONN=1`; not relevant)
- Re-arming after disconnect (covered by native ztest only)

This is a known coverage boundary, documented here so future agents don't assume hardware-test coverage of these paths.

## Test driver (CoC only)

`tools/ots-push/` (new Python package, lives in repo):

- `__init__.py`, `cli.py`, `client.py`
- Depends on `bleak` (PyPI, cross-platform)
- CLI: `python -m ots_push push <device-name-or-address> <app_id> <profile_id> <source.sqbc>`
- Discovery: `bleak.BleakScanner.find_device_by_name(...)` or by address
- OTS discover: scan services for UUID 0x1825
- Data path: **L2CAP CoC only** (no GATT-writes fallback). Use `bleak`'s L2CAP CoC support where available; on platforms where `bleak` doesn't support CoC, the driver exits with a clear "CoC unsupported on this platform" skip message.
- Skip pattern (mirrors `scripts/c3-supermini-test-ble-smoke.sh:100-113`):
  - `import bleak` fails → exit 0 with `"OK ... skipped because bleak is unavailable"`
  - `bleak.BleakScanner()` returns no adapters → exit 0 with `"OK ... skipped because no Bluetooth adapter is available"`
  - Connect/timeout → exit 0 with `"OK ... skipped because the Bluetooth adapter is not usable on this host"`
  - `bleak` does not support L2CAP CoC → exit 0 with `"OK ... skipped because bleak on this platform does not support L2CAP CoC"`

## Hardware test wrapper

`scripts/zephyr-test-ble-object-transfer.sh` (new, defaults to XIAO ESP32-C3 e-paper dev target):

1. Source `scripts/zephyr-env.sh` to set up the Zephyr environment
2. Build and flash the XIAO target: `cargo run -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd` (type-check / build) followed by the canonical flash command for the XIAO target (existing convention used by `scripts/c3-supermini-test-ble-smoke.sh`)
3. Compile `examples/ble-install/main.squid` (new example) to a `.sqbc` via `squidc build`
4. Install the example via `squidc app install --port <port> examples/ble-install/main.sqbc --as ble-install`
5. `app.arm("ble-install")` via the REPL/serial CLI
6. Run `python -m ots_push push <device> ble-install sqbc-install examples/ble-install/main.sqbc`
7. Verify via `squidc app list --port <port>` that `ble-install` is registered (and, if `app.install` is exercised, that the second app appears)
8. Cleanup: `app.disarm` + uninstall

Skip behavior is fully encapsulated in `ots_push` (per step 6); the wrapper itself just propagates the exit code.

## Native ztests

`firmware/zephyr/tests/ble-object-transfer/` (new, mirrors `ble-smoke/`):

- `trigger-table.test.c`: trigger table add/remove, cap enforcement, all-or-nothing arming
- `object-name.test.c`: `parse_ble_object_name` for valid inputs, format errors, unsafe app_id, empty segments, extension not starting with `.`
- `staging-lifecycle.test.c`: ephemeral cleanup, OACP Abort cleanup, disconnect-mid-stream cleanup, single-session `OBJ_LOCKED` rejection
- `event-dispatch.test.c`: pending-event slot handoff, poll-path drain, ordering of `fs_unlink` vs event return
- `app-install.test.c`: `app.install(file_ref)` happy path on a known SQBC; magic-mismatch error path; `app_id` rejection

Each test is a function-pointer-stub ztest on `native_sim`, modeled on `firmware/zephyr/tests/ble-smoke/src/main.c`.

## Language spec changes

- **`docs/language_spec.md` section 30** — update the "Rules":
  - Remove: "Firmware should validate uploaded content only after the transfer completes and the staged file is flushed. Failed validation must delete or quarantine the staged file without publishing it."
  - Add: "Firmware delivers the file as-is to the armed app. Validation is the app's responsibility (e.g., via `app.install(file_ref)` which validates the SQBC magic header)."
  - Add: "The staging file is ephemeral: it is `fs_unlink`d after the `ble.object.complete` event handler returns. The app must consume the file (copy, install, log) before returning from the handler."
- **`docs/language_spec.md` section 32** — add `app.install(fileRef, appId)` to the app registry namespace.
- **`docs/sqbc_binary_format.md` section 10** — remove the `sink` field from the profile record layout (was proposed in the previous revision; rolled back). Add a note that the `file.*` ref is the new event payload field for `ble.object.complete`.
- **`docs/runtime_limits.md`** — add `SQ_VM_RUNTIME_BLE_PROFILE_ARMED_MAX` row in the runtime caps table; tighten the `runtime_limits.md:36` sentence fragment about armed/foreground timer caps.
- **`docs/zephyr_vm_host_abi_coverage.md`** — add the BLE family to the table, pointing at the new `firmware/zephyr/tests/ble-object-transfer/` ztests and the new `app.install` VM export.
- **`docs/firmware_app_storage.md`** — document the BLE staging path family `/sq/tmp/ble-object-<app_id>-<profile_id>.tmp` alongside the existing install/resource/temp-run staging paths; document `sq_app_store_install_from_file_ref`.
- **`docs/hardware_target_tests.md`** — add the new `scripts/zephyr-test-ble-object-transfer.sh` to the inventory; document the skip pattern; document the known coverage boundary.
- **`docs/firmware_state_machines.md`** — add a "BLE Object Transfer" section to the begin/chunk/commit document, noting it follows a different state machine (OTS / L2CAP CoC) than the serial install path.
- **`docs/firmware_build_architecture.md`** — add the BLE family to the C stack report and the trigger-table diagram; add the OTS RAM cost to the linker-DRAM baseline.

## Open follow-up items (add to `ROADMAP.md` after this slice lands)

- Per-app uninstall opcode (no `AppUninstall` exists; BLE trigger table entries for uninstalled apps currently only clear on `Reset` / `StorageFormat`).
- OTS client role (currently server-only).
- L2CAP CoC support in `bleak` on macOS / Windows (verify cross-platform in slice 10).
- `OACP Calculate Checksum` if any app needs a verified-byte-count.
- Multi-connection OTS support if hardware gets `CONFIG_BT_MAX_CONN > 1`.
- A "Concepts" or "Glossary" section in `docs/language_spec.md` consolidating the arming model and the `armed-*` terminology aliases (deferred from this design's review).

## References

- Bluetooth SIG Object Transfer Service v1.0 (GATT-based).
- Zephyr OTS API: `target/zephyr/workspace/zephyr/include/zephyr/bluetooth/services/ots.h`.
- Zephyr OACP response codes: `target/zephyr/workspace/zephyr/subsys/bluetooth/services/ots/ots_oacp_internal.h:40-61`.
- Zephyr OTS Kconfig: `target/zephyr/workspace/zephyr/subsys/bluetooth/services/ots/Kconfig`.
- Zephyr L2CAP PSM `0x0025` (standard OTS L2CAP PSM): `ots_l2cap.c:34`.
- Zephyr CRC32 helpers: `crc32_ieee(data, len)` and `crc32_ieee_update(crc, data, len)` from `target/zephyr/workspace/zephyr/include/zephyr/sys/crc.h:284, 296`.
- Zephyr OTS API status: `[Experimental]` (`ots.h:17`, `Kconfig:7, 11`). Zephyr upstream may change this API; the slice pins a specific Zephyr version via `target/zephyr/workspace/zephyr/west.yml`.
