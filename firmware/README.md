# Squid Firmware

This directory contains SquidScript reference firmware.

The current firmware goal is to exercise the SquidScript language specification
on real constrained hardware. The ESP32-C3 Super Mini is the first hardware
harness because it is cheap, flashable, and already verified over USB
Serial/JTAG. It is not a product target and it is not XTEINK X4 staging firmware.

Current status:

- `squid-firmware` builds a Super Mini reference firmware image with a serial
  shell for installing and running SQBC v2 bytes from RAM.
- The shared host-testable VM loads real SQBC v2 bytecode, dispatches `onStart`
  and `onKey.*` handlers, mutates in-RAM state, traces `state.load`,
  `state.save`, and `app.exit`, and rejects the browser-only SQBC v1 IR
  container.
- `squid-firmware` still builds the earlier XTEINK X4 hello-world display
  bring-up image, but X4 display behavior is not part of the reference VM
  milestone.

Useful commands from the repository root:

```sh
scripts/c3-supermini-build.sh
scripts/c3-supermini-flash.sh
scripts/c3-supermini-monitor.sh
scripts/c3-supermini-install-sqbc.sh
scripts/c3-supermini-smoke.sh
scripts/squidc-build.sh build compiler/rust/fixtures/conformance/headless_counter.squid --target esp32c3-super-mini --out target/reference-firmware/headless_counter.sqbc
```

The preferred hardware acceptance check for the reference firmware milestone is:

```sh
scripts/c3-supermini-smoke.sh
```

Use `scripts/c3-supermini-smoke.sh --skip-flash` when the current firmware image
is already flashed. The smoke test compiles the headless counter fixture,
installs SQBC v2 bytes over USB serial, runs the app, sends `SELECT`, `SELECT`,
and `BACK`, then verifies state and trace output.

`espflash` may print `Monitor options were provided, but --monitor/-M flag
isn't set` during flashing on some host configurations. The project scripts do
not suppress this warning; it is harmless when flashing continues and the smoke
test reaches `OK smoke esp32c3-super-mini reference firmware`.

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
