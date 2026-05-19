# Squid Firmware

This directory contains SquidScript reference firmware.

The current firmware goal is to exercise the SquidScript language specification
on real constrained hardware. The ESP32-C3 Super Mini is the first hardware
harness because it is cheap, flashable, and already verified over USB
Serial/JTAG. It is not a product target and it is not XTEINK X4 staging firmware.

Current status:

- `squid-firmware` builds a Super Mini reference firmware image with a serial
  shell for installing and running SQBC v2 bytes from RAM.
- The shared host-testable VM loads real SQBC v2 bytecode, dispatches generic
  `event.on("...")` handlers, mutates in-RAM state, traces `state.load`,
  `state.save`, and `app.exit`, dispatches `hardware.gpio.*` to the Super Mini
  status LED, and rejects the browser-only SQBC v1 IR container.
- `squid-firmware` still builds the earlier XTEINK X4 hello-world display
  bring-up image, but X4 display behavior is not part of the reference VM
  milestone.

Useful commands from the repository root:

```sh
scripts/c3-supermini-build.sh
scripts/c3-supermini-flash.sh
scripts/c3-supermini-test-reference-firmware.sh
cargo run -p squidc -- build compiler/rust/fixtures/conformance/headless_counter.squid --out target/reference-firmware/headless_counter.sqbc
cargo run -p squidc -- doctor
```

The preferred hardware target check for the reference firmware milestone is:

```sh
scripts/c3-supermini-test-reference-firmware.sh
```

Use `scripts/c3-supermini-test-reference-firmware.sh --skip-flash` when the
current firmware image is already flashed. The hardware test compiles the
headless counter fixture,
installs SQBC v2 bytes over USB serial, runs the app, sends `SELECT`, `SELECT`,
and `BACK`, then verifies state and trace output.

`espflash` may print `Monitor options were provided, but --monitor/-M flag
isn't set` during flashing on some host configurations. The project scripts do
not suppress this warning; it is harmless when flashing continues and the
hardware test reaches `OK hardware test esp32c3-super-mini reference firmware`.

The normal compile/upload/run path uses `squidc run`. If `--port` is omitted,
`squidc` probes visible serial ports with `HELLO` and uses the single
SquidScript firmware target it finds:

```sh
cargo run -p squidc -- run examples/blinky-supermini/main.squid
cargo run -p squidc -- device key SELECT
cargo run -p squidc -- device output
```

Named multi-app flows use grouped app commands:

```sh
cargo run -p squidc -- app install tests/hardware/c3-supermini/generic-events/break-reminder.squid
cargo run -p squidc -- app install tests/hardware/c3-supermini/generic-events/reader-clock.squid
cargo run -p squidc -- app install tests/hardware/c3-supermini/generic-events/main.squid
cargo run -p squidc -- app launch main
```

V4 snippet/session checks still use `squidc repl` script mode:

```sh
cargo run -p squidc -- repl --port /dev/ttyACM0 --script tests/repl/default-dev.session
cargo run -p squidc -- repl --port /dev/ttyACM0 --script tests/repl/release-strips-debug.session
cargo run -p squidc -- repl --port /dev/ttyACM0 --script tests/repl/render-drawlog.session
cargo run -p squidc -- repl --port /dev/ttyACM0 --script tests/repl/hardware-gpio-status-led.session
scripts/c3-supermini-test-timer-armed-app.sh
scripts/c3-supermini-test-generic-triggered-apps.sh
```

The default `run`/`app install`/`repl` profile is `dev`; the default firmware
profile for the Super Mini reference target is also `dev`. Confirm the physical
onboard LED toggles when testing blinky with `device key SELECT`. See
`docs/developer_repl_protocol.md` for the serial protocol. `--target` is
optional and should be paired with
`--check-target` only when you explicitly want host-side compatibility checks
against a target definition.

`squidc doctor` performs read-only host/toolchain/device readiness checks. It
does not flash, install apps, reset the board, or run hardware tests. See
`docs/squidc_cli.md` for the grouped `squidc` command surface.

The timer armed-app hardware test uploads two SQBC apps, runs the `main` app,
lets the firmware timer fire, then verifies `OUTPUT.GET` contains
`main start`, `armed register`, and `armed timer`.

The generic triggered-apps hardware test is the canonical SQBC firmware
regression path for the app-stack work. It installs `main`, `reader-clock`, and
`break-reminder`, verifies `app.start`, `app.arm`, `event.addSource`, session timer,
armed timer, and key exit behavior over USB serial.

The current Super Mini app store is a temporary RAM-only development harness.
It has a six-slot RAM app registry so app lifecycle behavior can be tested
before a persistent filesystem or flash-backed app store exists.

If `espflash` can list `/dev/ttyACM0` but cannot open it, grant the current
login temporary access to the serial node:

```sh
sudo setfacl -m u:$USER:rw /dev/ttyACM0
```

This ACL is device-instance local and may need to be repeated after unplugging
or re-enumerating the board. For a persistent setup, add the user to `dialout`
and start a fresh login session:

```sh
sudo usermod -aG dialout $USER
```

Host checks:

```sh
cargo test
(cd firmware/squid-firmware && cargo test --target x86_64-unknown-linux-gnu)
```
