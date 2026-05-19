# Hardware Acceptance Tests

Hardware acceptance tests exercise a connected physical target. They are not
unit tests and they must not run in parallel against the same serial device.

The current hardware acceptance target is the ESP32-C3 Super Mini reference
firmware on `/dev/ttyACM0` or the first auto-detected SquidScript firmware
serial target.

## ESP32-C3 Super Mini

### Firmware Flash

```sh
./scripts/c3-supermini-build.sh
./scripts/c3-supermini-flash.sh
```

Builds and flashes the reference firmware.

### Blinky App

```sh
cargo run -p squidc -- run examples/blinky-supermini/main.squid
cargo run -p squidc -- output
cargo run -p squidc -- monitor --max-lines 4
```

Compiles, uploads, and runs the blinky SquidScript app as `main`. The serial
output should include `blinky ready` followed by alternating `blink` values.
The onboard LED should visibly toggle.

`squidc monitor` polls the firmware debug output buffer and prints newly
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

### Timer Background Smoke

```sh
./scripts/c3-supermini-timer-background-smoke.sh
```

Installs the timer background example and verifies `app.arm`,
`event.addSource`, and timer-dispatched output on real firmware.

### Generic Events E2E

```sh
./scripts/c3-supermini-generic-events-e2e.sh
```

Installs the generic-events fixture set and verifies app start/arm flow, timer
events, triggered app behavior, and key-event handling on real firmware.

## Rules

- Run these checks sequentially against a given board.
- Do not leave `squidc monitor` open while running an install, flash, REPL, or
  smoke script.
- Prefer auto-detected ports for normal `squidc` flows. Pass `--port` only when
  the host has multiple candidate serial devices or auto-detection fails.
- If the device is visible but access fails, use the documented ACL workaround:
  `sudo setfacl -m u:$USER:rw /dev/ttyACM0`.
