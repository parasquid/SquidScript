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
- Install and launch a minimal SQBC app through the Zephyr command surface.
- Dispatch key events and verify state/output traces.
- Verify persistent app storage and app state through Zephyr storage.
- Verify GPIO/indicator behavior, including a final visible board-state check.
- Verify Wi-Fi scan without credentials.
- Verify Wi-Fi station behavior only when credentials are explicitly provided.

The obsolete Rust firmware scripts are not current hardware target tests. As
Zephyr coverage lands, keep the suite ordered so stateful reset/install tests
run before the final visible board-state check.

For the current ESP32-C3 Super Mini Zephyr target, the blinky check installs
and launches `examples/blinky-supermini/main.squid`. Serial `device output`
should show repeated `blink false` / `blink true` lines and `device errors`
should be empty before the final visible check. After that final check starts,
do not run another serial command unless debugging the final board state.

The indicator breathe check installs and launches
`examples/breathe-supermini/main.squid`. Serial `device output` should include
`breathe ready`, `device errors` should be empty, and the final visible board
state should be a repeating onboard LED breathe pattern.
