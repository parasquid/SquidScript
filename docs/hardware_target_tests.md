# Hardware Target Tests

Hardware target tests exercise a connected physical board. They are not unit
tests, and they must not run in parallel against the same serial device.

Hardware target tests and serial/flashing commands must run outside the Codex
sandbox. Sandboxed sessions do not reliably expose `/dev/ttyACM*`,
`/dev/ttyUSB*`, or `/dev/serial/by-id`, even after host reboot. Use escalated
command execution for ESP32-C3 Super Mini serial visibility checks and hardware
target tests:

```sh
cargo run -p squidc -- doctor
~/.cargo/bin/espflash list-ports --list-all-ports
./scripts/c3-supermini-test-hardware.sh
```

`squidc doctor` is read-only. It reports toolchain, optional size tooling,
serial visibility, firmware handshake, and hardware-test script readiness.
See `docs/squidc_cli.md` for the grouped `squidc` command surface.

The suite is ordered so tests that reset, install, or replace apps run before
visual/interactive checks. Blinky runs last because it intentionally leaves the
board in an observable state with the onboard LED toggling. If a later test
installs another app, it will replace that visible state.

The current hardware target is the ESP32-C3 Super Mini reference firmware on
the first auto-detected SquidScript firmware serial target. Set
`ESPFLASH_PORT=/path/to/device` when multiple devices are connected or
auto-detection is not enough.

## ESP32-C3 Super Mini

### Full Sequence

```sh
./scripts/c3-supermini-test-hardware.sh
./scripts/c3-supermini-test-hardware.sh --skip-flash
```

Runs the hardware target checks sequentially and deliberately runs the blinky
app last. This leaves the onboard LED visibly active after the automated serial
checks finish.

Current order:

1. Reference firmware protocol test.
2. GPIO REPL session.
3. Default dev REPL session.
4. Persistent app registry test.
5. Timer-armed app test.
6. Generic triggered-apps test.
7. Blinky REPL session.
8. Blinky app run and short monitor check.

Add new hardware tests before the final blinky app unless the new test is also
intended to be the final visible board-state check.

### Firmware Flash

```sh
./scripts/c3-supermini-build.sh
./scripts/c3-supermini-flash.sh
```

Builds and flashes the reference firmware. Normal flashing writes the
bootloader, partition table, and factory app partition. It does not erase the
`squidfs` app-storage partition. Use the persistent registry test or
`STORAGE.FORMAT` when a clean app store is needed.

### Reference Firmware Protocol

```sh
./scripts/c3-supermini-test-reference-firmware.sh
./scripts/c3-supermini-test-reference-firmware.sh --skip-flash
```

Builds/flashes the reference firmware unless `--skip-flash` is used, installs
the headless counter fixture as `main`, and verifies run, key, state, and trace
behavior over USB serial.

### Persistent App Registry

```sh
./scripts/c3-supermini-test-persistent-app-registry.sh
```

Formats app storage, installs the headless counter fixture as `main`, resets the
chip, verifies `APP.LIST` still reports `main`, then dispatches `app.start`.
This test proves installed SQBC survives a real firmware restart.

### Blinky App

```sh
cargo run -p squidc -- run examples/blinky-supermini/main.squid
cargo run -p squidc -- device output
cargo run -p squidc -- device monitor --max-lines 4
```

Compiles, uploads, and runs the blinky SquidScript app as `main`. The serial
output should include `blinky ready` followed by alternating `blink` values.
The onboard LED should visibly toggle.

`squidc device monitor` polls the firmware debug output buffer and prints newly
observed `output=...` lines to the terminal. Use `--raw` only when literal
serial bytes are needed.

### GPIO REPL Session

```sh
cargo run -p squidc -- repl --script tests/repl/hardware-gpio-status-led.session
```

Exercises `hardware.gpio.write`, `hardware.gpio.read`, and
`hardware.gpio.toggle` against the `status_led` alias and raw `GPIO8`.

### Blinky REPL Session

```sh
cargo run -p squidc -- repl examples/blinky-supermini/main.squid --script tests/repl/blinky-supermini.session
```

Runs the blinky app through the REPL workflow, sends key events, and verifies
serial output/state.

### Default Dev REPL Session

```sh
cargo run -p squidc -- repl --script tests/repl/default-dev.session
```

Verifies the default dev profile REPL behavior without requiring an explicit
`:profile dev` command.

### Timer-Armed App

```sh
./scripts/c3-supermini-test-timer-armed-app.sh
```

Installs the timer armed-app example and verifies `app.arm`,
`event.addSource`, and timer-dispatched output on real firmware.

### Generic Triggered Apps

```sh
./scripts/c3-supermini-test-generic-triggered-apps.sh
```

Installs the generic-events hardware fixture set from
`tests/hardware/c3-supermini/generic-events` and verifies app start/arm flow,
timer events, triggered app behavior, and key-event handling on real firmware.

## Rules

- Run these checks sequentially against a given board.
- Do not leave `squidc device monitor` open while running an install, flash, REPL, or
  hardware test script.
- Prefer auto-detected ports for normal `squidc` flows. Pass `--port` only when
  the host has multiple candidate serial devices or auto-detection fails.
- If the device is visible but access fails, use the documented ACL workaround:
  `sudo setfacl -m u:$USER:rw /dev/ttyACM0`.
