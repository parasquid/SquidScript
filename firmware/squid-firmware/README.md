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
RUN.TEMP <app-id> <len> <fnv32hex>
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
`docs/developer_repl_protocol.md`, including `INSTALL.APP`, `RUN.TEMP`, `RUN.APP`,
`RUN.EVENT`, `APP.LIST`, `STATE.GET`, `STATE.IMPORT`, `OUTPUT.GET`, and
`DRAWLOG.GET`.

The ESP32-C3 Super Mini firmware maps `hardware.gpio.*` to the target-defined
status LED aliases and raw `GPIO8`. Use these repository-root checks after
flashing:

```sh
cargo run -p squidc -- repl --script tests/repl/hardware-gpio-status-led.session
cargo run -p squidc -- repl examples/blinky-supermini/main.squid --script tests/repl/blinky-supermini.session
cargo run -p squidc -- repl --script examples/blinky-supermini/main.squid
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
`break-reminder`, then verifies `app.start`, `app.arm`, `service.timer.*`, a
session-local timer, an armed timer, and key-driven `app.exit`.

The installed app registry is a six-slot runtime cache over persistent app
storage. On the ESP32-C3 Super Mini, installed SQBC payloads are stored in
LittleFS under `/apps/<app-id>.sqbc` on the `squidfs` flash partition and are
loaded back into the registry at firmware startup.

`INSTALL.APP` receives raw SQBC v2 bytes, validates the payload, writes app
storage, and then publishes the app in the registry cache. `RUN.TEMP` validates
SQBC bytes into RAM and pushes the temp app onto the foreground stack without
writing flash. `STORAGE.FORMAT` formats app storage and clears the cache. State
persistence remains a separate future milestone; `state.load` and `state.save`
are traced but do not write flash yet.

The crate still contains `x4-hello`, a separate XTEINK X4 display bring-up
binary. Keep X4 display work separate from the reference VM milestone.
