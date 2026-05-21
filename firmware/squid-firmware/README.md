# squid-firmware

Rust reference firmware for exercising SquidScript language semantics.

The host-testable core is intentionally separated from ESP32-C3 hardware
entrypoints. Run TDD checks from this directory:

```sh
cargo test --target x86_64-unknown-linux-gnu
```

The ESP32-C3 serial binary keeps hardware boot wiring in
`src/bin/c3_supermini_serial_hello.rs`. The serial firmware module root is
`src/serial.rs`, with implementation split under `src/serial/`:

- `command.rs`: shell command parsing, install/run/key/state command handling
- `lifecycle.rs`: boot, app launch/exit, pending actions, timer dispatch
- `runtime.rs`: VM host callbacks, indicator/GPIO/timer behavior, trace output
- `vm.rs`: active VM wrappers, SQBC loading, persistent app host/reader glue
- `state.rs`: resource reporting and state import/printing helpers
- `line.rs` and `log.rs`: line buffering and bounded log formatting

Build the ESP32-C3 Super Mini reference firmware:

```sh
cargo build --release --features hardware --bin c3-supermini-serial-hello
```

When asking for "memory", use RAM numbers by default. Flash image size,
partition usage, and LittleFS app storage are flash storage numbers and should
be requested or reported separately.

From the repository root, collect RAM and storage diagnostics with:

```sh
scripts/c3-supermini-build.sh
riscv64-elf-size firmware/squid-firmware/target/riscv32imc-unknown-none-elf/release/c3-supermini-serial-hello
scripts/c3-supermini-flash.sh
cargo run -p squidc -- device resources
```

`riscv64-elf-size` gives ELF `text`, `data`, and `bss`; for RAM-focused
comparisons, use `data` and `bss`. The flash script prints `espflash`
app-partition usage. `device resources` reads the firmware
`RESOURCES.GET` report for app/runtime diagnostics. Current
`memory_available_bytes` is a static estimate; it is not live heap telemetry and
does not yet show ESP radio heap free/used values.

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

The ESP32-C3 Super Mini firmware maps `service.indicator.*` to the default
logical indicator and keeps `hardware.gpio.*` for raw GPIO names such as
`GPIO8`. `service.indicator.breathe()` returns the default indicator to the
firmware breathing pattern after app-driven writes or toggles. Use these
repository-root checks after flashing:

```sh
cargo run -p squidc -- repl --script tests/repl/hardware-gpio-indicator.session
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
LittleFS under `/apps/<app-id>/main.sqbc` on the `squidfs` flash partition and
are loaded back into the registry at firmware startup. Package resources are
stored under the same app directory.

`INSTALL.APP` receives raw SQBC v3 bytes, validates the payload, writes app
storage, and then publishes the app in the registry cache. `RUN.TEMP` validates
SQBC bytes into RAM and pushes the temp app onto the foreground stack without
writing flash. Keeping temp runs RAM-backed is intentional before 1.0 because
`squidc run` is the rapid iteration path. `STORAGE.FORMAT` formats app storage
and clears the cache. Installed apps persist declared primitive state through
firmware-owned binary records. `RUN.TEMP` remains RAM-backed: `state.load`,
`state.save`, and `state.reset` do not write flash for temp apps.

The crate still contains `x4-hello`, a separate XTEINK X4 display bring-up
binary. Keep X4 display work separate from the reference VM milestone.
