# HANDOFF — BLE-received app launch faults `-5` (DoD #6)

Temporary handoff for a fresh session. Delete this file once the bug is fixed.
Durable versions of the findings: `docs/firmware_app_load_install_notes.md`
("Known issue" section) and saved memory `ble-received-app-launch-minus5.md`.

## Goal (DoD #6)

A `.sqbc` pushed over BLE OTS installs **and the installed app launches with no
`-5`**, in one flow. Hardware: XIAO ESP32-C3 on `/dev/ttyACM0`, address
`58:8C:81:AC:52:5A`. Today: install works; launching the received app faults
`runtime=vm_error code=-5 (EIO)` until reboot.

## ROOT CAUSE (found this session — it is NOT flash corruption)

Earlier sessions assumed a flash/cache problem. **Disproven.** Hardware-proven
facts:

- The received `.sqbc` is **byte-perfect on flash** and reads back perfectly
  in-session on the poll thread — full-file checksum/signature matched the host
  bytes (`len=3277 mid@1638=11 last=0x2a ff=0`) at both the staging path and the
  final `apps/installed-app/main.sqbc` path, before launch.
- At launch, the VM **never loads installed-app at all.** Device-error trace of
  one DoD push:
  ```
  sib ble-install sc=1 ev=app.start              # ble-install launched
  vmrd l=489 ... nstall/main.sqbc                 # ble-install bytecode loaded
  sib ble-install sc=1 ev=ble.object.complete     # OTS handler dispatched (set_current=TRUE!)
  vmrd l=489 ... nstall/main.sqbc                 # ble-install reloaded for the event
  sib ble-install sc=0 ev=app.exit                # EXIT_FOR_LAUNCH dispatches app.exit
  runtime=vm_error code=-5 (EIO)                  # <-- app.exit dispatch FAULTS
  ```
  (`sib` = entry of `start_installed_app_bytes`; `vmrd` = bytes the VM worker
  thread actually read at context init.) **There is no `sib installed-app`** —
  installed-app is never launched.

**The chain:** the `ble.object.complete` event is dispatched to the
already-foreground app `ble-install` with **`set_current=true`** (a *re-launch*,
which resets+reloads the VM context). That handler calls `app.install` +
`app.launch("installed-app")`, setting `LAUNCH_REQUESTED`. The lifecycle then
runs `EXIT_FOR_LAUNCH`, which dispatches the current app's **`app.exit`** — and
**that `app.exit` dispatch faults (`vm_error -5`)**, putting the runtime in
`ERROR`, so the lifecycle never advances to the `START` step that would load
installed-app. `active` stays `ble-install`.

`ble-install` has **no** `app.exit` handler, and a serial-install + CLI-launch of
the *same bytes* (which also runs `EXIT_FOR_LAUNCH → app.exit`) works fine. The
only difference is the preceding **`set_current=true` re-launch dispatch of
`ble.object.complete`**. Reboot "fixes" it only because it clears the VM
runtime state and the launch is then driven fresh.

## NEXT STEPS (for the new session)

1. **Reproduce on native_sim (no BLE/hardware needed).** Drive a foreground app
   that receives an event dispatched with `set_current=true` (the timer/BLE
   "due event" path — see below), whose handler calls `app.launch(other)`, then
   assert the subsequent `app.exit` dispatch does not fault. Armed-timer events
   likely take the same `set_current=true` path, so a timer-handler-that-launches
   test may reproduce it without BLE. Make this the failing TDD test.
2. **Find why `app.exit` faults** after a `set_current=true` re-dispatch. Two
   leading directions:
   - The due-event dispatch should arguably use `set_current=false` when the
     target **is already the current foreground app** (no re-launch / no context
     reset). Re-launching the current app via `start_installed_app_bytes`
     (`set_current=true` → `reset_vm_context`) mid-life may corrupt lifecycle /
     return-stack / VM context state that then breaks `app.exit`.
   - Or the VM/dispatch invariant violated by dispatching `app.exit` on a context
     that was just reset+reloaded for an event while `LAUNCH_REQUESTED` is set.
3. Fix at the correct layer (lifecycle/event-routing semantics or VM dispatch),
   add the host test, then re-run DoD #6 on hardware.

### Key code locations

- BLE/timer "due event" drain → dispatch: `firmware/zephyr/src/device_protocol.c`
  ~`1438` (drain `sq_ble_ots_drain_pending_event`), `1482`
  `sq_app_lifecycle_next_step`, `1509` `STEP_START_APP` → `start_resolved_app`.
- `start_installed_app_bytes`: `device_protocol.c:891` (resets context when
  `set_current`; the `sib` diagnostic is at its top).
- Lifecycle state machine: `firmware/zephyr/src/app_lifecycle.c`
  `sq_app_lifecycle_next_step` (`352` LAUNCH_REQUESTED→app.exit `set_current=false`;
  `332` EXIT_FOR_LAUNCH→START target `set_current=true`). Find the IDLE+`due_app`
  case that sets `set_current=true` for the event dispatch.
- Context init + the `vmrd` diagnostic: `firmware/zephyr/src/vm_runtime.c` ~`441`
  (`if (!context_ready)`), `454` `sqvm_context_init_in_place`.

## REAL changes this session (KEEP)

- `firmware/zephyr/src/app_store.{c,h}` — `sq_app_store_commit_external_file()`:
  installs a received staging file into the app store by **rename** (not copy),
  saving a full re-write of the payload. (User explicitly wants to keep the
  rename.)
- Deferred install: `app.install` from a handler is queued
  (`sq_vm_runtime_request_install`, `pending_install` in `vm_runtime.{c,h}`,
  queued in `vm_runtime_app_lifecycle.c`) and performed VM-idle in
  `sq_device_protocol_poll`. (Reasonable design; not the bug, but harmless.)
- `examples/ble-install/main.squid` — imperative `service.ble.start` receiver.
- `docs/firmware_app_load_install_notes.md` — corrected "Known issue" writeup.

## DIAGNOSTIC changes this session (REVERT before finalizing)

- `device_protocol.c`: the `sib ... record_device_error` block at the top of
  `start_installed_app_bytes` (the only remaining diagnostic there; the `dbg`/
  `dbg2` blocks were already removed).
- `vm_runtime.c`: the `vmrd ... record_device_error` block right after
  `sqvm_context_init_in_place` (uses `extern ... sq_vm_fs_dbg_*`).
- `vm_fs_storage.c`: the `sq_vm_fs_dbg_sum/len/path` globals + the accumulation
  in `fs_storage_read_sqbc`.
- `runtime_limits.h` and `vm_runtime.h`: `SQ_VM_RUNTIME_DEVICE_ERROR_MAX` raised
  `2 → 8`. **Note:** at 12 the `errors` protocol response overflowed and
  returned `-5`; 8 works. Restore to 2 when done.

## Ruled OUT (do not re-investigate)

Install method (copy / rename / defer-to-idle / 200 ms settle), ESP32 flash
cache (`CONFIG_ESP_FLASH_HOST=y` → cache-managed `esp_flash_*`; LittleFS remount
no effect), BT RX stack size (4096→8192), per-chunk vs single-handle write
pattern, write thread (BT RX wq vs system wq — both fail), active BLE connection
during the write (serial install + launch works while connected). The file is
correct and readable; the fault is purely in the VM launch/lifecycle path.

## Hardware workflow

```
source scripts/zephyr-env.sh; export ESPFLASH_PORT=/dev/ttyACM0
cargo run -q -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd
west flash -d build/zephyr/xiao-esp32c3-gdeq0426t82-sd
cargo run -q -p squidc -- device storage-format
cargo run -q -p squidc -- app install /tmp/ble-install.sqbc      # built from examples/ble-install
cargo run -q -p squidc -- app launch ble-install
# push (tools/ots-push, bleak in the zephyr venv):
( cd tools/ots-push && python3 -m ots_push push 58:8C:81:AC:52:5A ble-install sqbc-install /tmp/payload2k.sqbc )
cargo run -q -p squidc -- device errors    # trace ring (currently size 8)
cargo run -q -p squidc -- device output    # expect "payload2k running" when fixed
cargo run -q -p squidc -- device lifecycle  # expect active=installed-app when fixed
```
Fixtures: `/tmp/ble-install.sqbc` (receiver), `/tmp/payload2k.sqbc` (3277-byte
payload that prints "payload2k running"; build any app with `squidc app build`).
All hardware/git commands run outside the sandbox; never run two hardware
commands in parallel. **The dev board currently has the diagnostic firmware
flashed.**

## Compiler-side stack-overflow detection (separate user question)

Firmware C thread stack overflow can't be caught at compile time (runtime call
depth). Loud runtime detection options here are limited: `HW_STACK_PROTECTION`
won't link (espressif esp32c3 port doesn't wire up RISC-V PMP regions);
`CONFIG_STACK_SENTINEL` builds but broke the protocol link when enabled and there
is no USB-CDC console to read a panic. Worth a small dedicated effort
(roadmap), not blocking this bug — and this bug is **not** a stack overflow.
```
