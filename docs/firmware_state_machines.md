# Firmware State Machines

This document describes explicit firmware state machines that support
SquidScript runtime and device-protocol behavior. The app foreground lifecycle
is defined separately in `docs/app_lifecycle_state_machine.md`; this document
covers lower-level firmware states that must remain observable, bounded, and
testable.

## Protocol Transfer Sessions

Installed app uploads, temporary app runs, and resource uploads use the same
session phases. The wire protocol remains begin/chunk/commit. Rust FFI helpers
validate TLV fields, lengths, offsets, CRC32, and commit readiness; Zephyr C
advances the explicit phase only after the corresponding storage operation has
been accepted.

```mermaid
stateDiagram-v2
  [*] --> Idle
  Idle --> Receiving: begin accepted
  Receiving --> Receiving: chunk accepted
  Receiving --> Committing: commit request validated
  Committing --> Idle: publish/start succeeds
  Committing --> Committing: retryable publish/start failure
  Receiving --> Idle: reset/storage-format clears sessions
  Committing --> Idle: reset/storage-format clears sessions
```

Phase rules:

- `Idle` means no transfer is active.
- `Receiving` means begin was accepted and chunks may advance byte progress.
- `Committing` means the session has passed commit validation and is attempting
  the target action: publish installed SQBC, start a temp SQBC, or publish a
  resource file.
- Reset and storage-format paths clear active sessions back to `Idle`.
- The phase is firmware-local state. It does not change SQDP frame fields or
  host-visible upload chunk sizing.

## Protocol Scratch Ownership

Protocol scratch is a single bounded work area shared by maintenance commands
that need resumable progress. Scratch users must acquire ownership before
writing command-specific state and release the same owner when work completes
or is cancelled.

```mermaid
stateDiagram-v2
  [*] --> Free
  Free --> Owned: acquire(owner)
  Owned --> Free: release(same owner)
  Owned --> Owned: release(other owner) rejected
  Owned --> Owned: acquire(any owner) rejected
```

Ownership rules:

- Scratch can be acquired only while it is free.
- No owner can overlap an active scratch owner.
- A mismatched release is rejected and leaves the current owner intact.
- Runtime reset and storage-format cleanup paths release scratch before they
  begin new protocol work.

## Device Input Buttons

Device input buttons use a per-binding press/release debounce state machine.
The current portable behavior dispatches only press events; release is tracked
as state and does not dispatch an event.

```mermaid
stateDiagram-v2
  [*] --> Inactive
  Inactive --> Released: binding configured, raw released
  Inactive --> Pressed: binding configured, raw pressed
  Released --> DebouncingPress: raw pressed during debounce window
  DebouncingPress --> Pressed: debounce elapsed, raw pressed
  Pressed --> DebouncingRelease: raw released during debounce window
  DebouncingRelease --> Released: debounce elapsed, raw released
  Pressed --> Pressed: stable pressed
  Released --> Released: stable released
```

Button rules:

- Runtime polling stays bounded and nonblocking.
- Input polling is skipped while a foreground VM event is running.
- Press transition dispatches the configured logical event through the normal
  SquidScript runtime path.
- Release transition updates the button phase only.
- Board GPIO polarity and pull configuration remain target-specific firmware
  details; the portable runtime observes logical pressed/released state.

## Indicator Patterns

The target indicator uses one pattern state machine instead of independent
blink and breathe flag clusters.

```mermaid
stateDiagram-v2
  [*] --> Steady
  Steady --> Breathe: service.indicator.breathe()
  Steady --> Blink: service.indicator.blink(onMs, offMs)
  Breathe --> Blink: service.indicator.blink(onMs, offMs)
  Blink --> Breathe: service.indicator.breathe()
  Breathe --> Steady: write/toggle/raw GPIO write
  Blink --> Steady: write/toggle/raw GPIO write
  Steady --> Steady: write/toggle/raw GPIO write
```

Pattern rules:

- `Steady` owns direct write/toggle/raw GPIO output.
- `Breathe` advances through the fixed duty-cycle table on bounded poll ticks.
- `Blink` alternates on/off brightness using the configured durations.
- Starting a new pattern replaces the previous pattern.
- Indicator polling is best-effort from the main runtime poll path; errors do
  not block unrelated protocol polling.

## BLE Object Transfer (OTS / L2CAP CoC)

The BLE Object Transfer Service (GATT UUID 0x1825) follows a different
state machine than the serial begin/chunk/commit install path. The Zephyr
`bt_ots` module owns the OACP/OBCP procedure state; the firmware's
`ble_ots.c` adds an in-flight session state on top of it.

| Phase | Trigger | Action | Outcome |
| --- | --- | --- | --- |
| Idle | — | No in-flight transfer; `app_install_file` and `app_install_app` idle | Ready for new transfer |
| OACP Create | Client writes Object Name + OACP Create with size | `sq_ble_ots_obj_created_internal` parses the Object Name (`app_id/profile_id/.ext`), rejects with `OBJ_LOCKED` if busy, opens staging file at `/sq/tmp/ble-object-<app_id>-<profile_id>.tmp` | In-flight session active; staging file open |
| OACP Write | Client streams OACP Write via L2CAP CoC (PSM 0x0025) | `sq_ble_ots_obj_write_internal` `fs_seek`s to the L2CAP SDU offset and `fs_write`s the chunk to the staging file | Staging file grows; `bytes_received` tracked |
| OACP Execute=WRITE | Final OACP Write (`rem == 0`) | Closes the staging file; populates `sq_ble_ots_pending` with `is_complete=true` | Pending slot ready; poll path dispatches `ble.object.complete` |
| OACP Abort | Client sends OACP Abort (procedure 0x07) | Closes + `fs_unlink`s the staging file; clears the in-flight session; no event emitted | Idle; in-flight slot cleared |
| BT disconnect (mid-stream) | `BT_CONN_CB` disconnect | Populates `sq_ble_ots_pending` with `is_complete=false` and `error_reason="client-abort"`; poll path dispatches `ble.object.error` | Idle after handler; staging file `fs_unlink`d |
| Reset / StorageFormat | Device protocol handler | `sq_ble_ots_reset_session` closes + `fs_unlink`s the staging file, clears the in-flight session, clears the trigger table; no event emitted | Idle; trigger table empty |

The producer/consumer handoff is a single-slot pending event queue
(`sq_ble_ots_pending`) that the OTS callbacks (BT context) populate and
the device-protocol poll (main loop) drains. `sq_ble_ots_drain_pending_event`
copies the `app_id` and `event` (`ble.object.complete` or `ble.object.error`)
into caller-owned buffers; the poll path then runs the event handler
(via `start_resolved_app` + the existing lifecycle machinery). After the
handler returns (detected via `lifecycle_phase == IDLE`),
`sq_ble_ots_cleanup_staging` `fs_unlink`s the staging file and clears the
pending slot. The `app.install(fileRef, appId)` builtin reads the staging
file before the handler returns, validates the SQBC magic, and registers
the app at `<mount>/apps/<appId>/main.sqbc`. Single-session policy:
`SQ_VM_RUNTIME_BLE_PROFILE_ARMED_MAX = 2` armed profile entries, but only
one can be the active in-flight transfer at a time.

## Bounded Queues

Trace lines, output lines, drawlog entries, and similar diagnostics are bounded
queues rather than state machines. They should stay simple FIFO-style buffers
unless a future feature introduces meaningful lifecycle phases, ownership, or
transitions that need validation.
