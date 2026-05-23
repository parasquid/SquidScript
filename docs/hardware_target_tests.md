# Hardware Target Tests

Hardware target tests exercise a connected physical board. They are not unit
tests. Never run hardware commands in parallel against the same serial device:
concurrent flash, monitor, REPL, hardware-test, or `squidc device` commands can
interleave serial bytes, reset the board, or leave hardware in a misleading
state.

Hardware target tests and serial/flashing commands must run outside the Codex
sandbox. Sandboxed sessions do not reliably expose `/dev/ttyACM*`,
`/dev/ttyUSB*`, or `/dev/serial/by-id`.

## Current Target

The current real firmware target is Zephyr-backed ESP32-C3 work under
`firmware/zephyr`.

Build and flash:

```sh
./scripts/c3-supermini-build.sh
./scripts/zephyr-ram-audit.sh
./scripts/c3-supermini-flash.sh
```

Monitor:

```sh
./scripts/c3-supermini-zephyr-monitor.sh
```

Set `ZEPHYR_BOARD` when the default board identifier is not correct for the
attached ESP32-C3 board. The repository default is an unverified clone-board
default, not a sourced hardware fact.

## Test Inventory

The Zephyr-only hardware suite is not complete yet. The required inventory is:

- Build and flash the Zephyr diagnostic firmware.
- Check Zephyr RAM budget output before flashing.
- Verify the diagnostic boot banner over the serial monitor.
- Install and launch real SquidScript apps through the Zephyr command surface.
- Verify `app.launch`, `app.exit`, `app.arm`, timer-triggered armed dispatch,
  and `device lifecycle` process/armed stack diagnostics.
- Dispatch key events and verify state/output traces.
- Verify persistent app storage and app state through Zephyr storage.
- Verify GPIO/indicator behavior, including a final visible board-state check.
- Verify Wi-Fi scan without credentials.
- Verify Wi-Fi station behavior only when credentials are explicitly provided.

The obsolete Rust firmware scripts are not current hardware target tests. As
Zephyr coverage lands, keep the suite ordered so stateful reset/install tests
run before the final visible board-state check.

The Zephyr app lifecycle check is
`scripts/c3-supermini-test-app-lifecycle.sh`. It installs the real SquidScript
fixtures under `tests/hardware/c3-supermini/generic-events`, launches `main`,
verifies `reader-clock` starts via `app.launch`, verifies `break-reminder` is
registered on the armed stack through `app.arm`, waits for the armed timer to
start `break-reminder`, then sends `SELECT` so `app.exit` returns to the
previous app on the process stack.

The Zephyr app state check is `scripts/c3-supermini-test-app-state.sh`. It
installs `tests/hardware/c3-supermini/state-counter/main.squid`, launches it,
sends `SELECT` key events, verifies explicit `state.load()` / `state.save()`
debug output and non-empty `device state` bytes, resets the runtime without
formatting storage, relaunches the app, and verifies `state.load()` restores
the saved count.

`scripts/c3-supermini-measure-stack-usage.sh` runs after the stateful app and
app lifecycle checks in the full ESP32-C3 Super Mini suite. It records
`device resources` output under `target/hardware-tests/stack-usage/` and
verifies `vm_worker_stack_size_bytes`, `vm_worker_stack_used_bytes`, and
`vm_worker_stack_unused_bytes` are internally consistent. The current firmware
keeps the VM worker stack budget at 16 KiB while this measurement data is used
to decide whether a later reduction is safe.

`scripts/c3-supermini-test-wifi-scan-api.sh` runs before the final visible
LED check. It installs `tests/hardware/c3-supermini/wifi-scan-summary` and
launches a summary-only SquidScript app that calls `service.wifi.scan()`
without credentials. The app prints only `ok`, `error`, and `count`; the script
rejects raw BSSID, MAC, or local IP patterns in captured output. The default
RAM-guarded firmware may report `unsupported` when the Zephyr Wi-Fi driver is
not enabled.

For the current ESP32-C3 Super Mini Zephyr target,
`scripts/c3-supermini-test-blinky.sh` is the final full-suite check. It
installs and launches `examples/blinky-supermini/main.squid`. Serial
`device output` should show repeated `blink false` / `blink true` lines and
`device errors` should be empty before the final visible check. After that
final check starts, do not run another serial command unless debugging the
final board state.

The indicator breathe check installs and launches
`examples/breathe-supermini/main.squid`. Serial `device output` should include
`breathe ready`, `device errors` should be empty, and the final visible board
state should be a smooth repeating onboard LED breathe pattern. For the current
ESP32-C3 Super Mini target, that visible check exercises the logical
`indicator0` device, which the Zephyr overlay maps to the common-clone GPIO8
onboard LED through ESP32-C3 LEDC PWM.
