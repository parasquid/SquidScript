# squid-firmware

Rust reference firmware for exercising SquidScript language semantics.

The host-testable core is intentionally separated from ESP32-C3 hardware
entrypoints. Run TDD checks from this directory:

```sh
cargo test --target x86_64-unknown-linux-gnu
```

Build the ESP32-C3 Super Mini reference firmware:

```sh
cargo build --release --features hardware --bin c3-supermini-serial-hello
```

From the repository root, the preferred end-to-end hardware check is:

```sh
scripts/c3-supermini-test-reference-firmware.sh
```

Use `--skip-flash` to keep the current firmware image and repeat only the SQBC
install/run/key/state/trace verification.

On Linux, `/dev/ttyACM0` is commonly owned by `root:dialout`. If the board is
visible but flashing cannot open the port, grant temporary access to the current
user:

```sh
sudo setfacl -m u:$USER:rw /dev/ttyACM0
```

That workaround is reset when the device node is recreated. For the persistent
fix, add the user to `dialout` and use a fresh login session:

```sh
sudo usermod -aG dialout $USER
```

The Super Mini firmware exposes a USB Serial/JTAG shell:

```text
help
info
INSTALL.APP <app-id> <len> <fnv32hex>
RUN.APP <app-id>
RUN.EVENT <app-id> <event>
APP.LIST
key SELECT
key BACK
STATE.GET
trace
errors
reset
```

It also accepts the v4 developer protocol commands documented in
`docs/developer_repl_protocol.md`, including `INSTALL.APP`, `RUN.APP`,
`RUN.EVENT`, `APP.LIST`, `STATE.GET`, `STATE.IMPORT`, `OUTPUT.GET`, and
`DRAWLOG.GET`.

The ESP32-C3 Super Mini firmware maps `hardware.gpio.*` to the target-defined
status LED aliases and raw `GPIO8`. Use these repository-root checks after
flashing:

```sh
cargo run -p squidc -- repl --port /dev/ttyACM0 --script tests/repl/hardware-gpio-status-led.session
cargo run -p squidc -- repl --port /dev/ttyACM0 examples/blinky-supermini/main.squid --script tests/repl/blinky-supermini.session
cargo run -p squidc -- repl --port /dev/ttyACM0 --script examples/blinky-supermini/main.squid
```

The GPIO session verifies serial readback. The blinky session should also be
confirmed visually on the physical onboard LED.

The timer armed-app reference exercise uses a real firmware timer:

```sh
scripts/c3-supermini-test-timer-armed-app.sh
scripts/c3-supermini-test-generic-triggered-apps.sh
```

It installs `main` and `timer-armed-app` SQBC apps, runs `main`, waits for
timer ticks, and checks debug output for app startup plus armed timer events.

The generic triggered-apps hardware test is the canonical lifecycle regression
check for the current app-stack model. It installs `main`, `reader-clock`, and
`break-reminder`, then verifies `app.start`, `app.arm`, `event.addSource`, a
session-local timer, an armed timer, and key-driven `app.exit`.

The installed app registry is a temporary six-slot RAM-only development
harness. It accepts arbitrary valid app IDs, but installed apps are cleared on
reset or power loss until a persistent app registry/storage model exists.

`INSTALL.APP` receives raw SQBC v2 bytes into RAM. State persistence is
in-memory only for this milestone; `state.load` and `state.save` are traced but
do not write flash yet.

The crate still contains `x4-hello`, a separate XTEINK X4 display bring-up
binary. Keep X4 display work separate from the reference VM milestone.
