# Developer REPL Protocol

Status: v4 reference protocol for ESP32-C3 Super Mini dev firmware.

The developer REPL protocol is a line-oriented control protocol with explicit
binary payload phases for SQBC and state snapshot bytes. It is enabled by
default for v4 dev firmware.

Normal host workflows should use grouped `squidc` commands documented in
`docs/squidc_cli.md`. Use `squidc protocol raw` only for low-level protocol
troubleshooting.

## Commands

```text
HELLO
INSTALL.APP <app-id> <len> <fnv32hex>
RUN.TEMP <app-id> <len> <fnv32hex>
RUN.APP <app-id>
RUN.EVENT <app-id> <event>
APP.LIST
KEY <logical-key>
STATE.GET
STATE.IMPORT <len> <fnv32hex>
TRACE.GET
OUTPUT.GET
DRAWLOG.GET
ERRORS.GET
RESOURCES.GET
RESET
STORAGE.FORMAT
```

`INSTALL.APP` is followed by exactly `<len>` raw SQBC v3 bytes and stores them
in firmware-owned app storage under `<app-id>`. `RUN.TEMP` uses the same binary
payload framing, but stores the SQBC in RAM only and launches it as a temporary
foreground app. This is the pre-1.0 developer iteration path and must not write
flash. Temp apps are current-only and volatile: they are not installed, do not
appear in `APP.LIST`, and cannot be return targets after launching or being
replaced by an installed app. `STATE.IMPORT` is followed by exactly `<len>`
state snapshot bytes. The hash is FNV-1a over the payload bytes.

Success responses start with `OK`. Error responses start with `ERR`.

Multi-line payloads use begin/end markers:

```text
BEGIN STATE
count=1
exited=false
END STATE
OK STATE.GET
```

Resource responses are read-only diagnostics:

```text
BEGIN RESOURCES
memory_available_bytes=299136
app_storage_total_bytes=2031616
app_storage_used_bytes=0
app_storage_available_bytes=2031616
END RESOURCES
OK RESOURCES.GET
```

These values are target-firmware metrics. `memory_available_bytes` is the
ESP32-C3 reference firmware's raw runtime RAM budget, not portable heap
introspection.

Installed apps are persistent in the ESP32-C3 reference firmware. On startup,
firmware scans the app store and rebuilds the in-memory registry cache.
`RESET` resets the development runtime state and clears the temp app but does
not erase installed apps. `STORAGE.FORMAT` formats the firmware app store and
clears the registry cache.

See `docs/firmware_app_storage.md` for the current ESP32-C3 storage layout.

## State Snapshots

State snapshots are newline-separated `name=value` records using the VM
primitive value syntax:

```text
count=2
label="Hello"
enabled=true
missing=null
```

Import restores matching state names with compatible primitive values. Unknown
or incompatible values are ignored for this milestone.

## Debug Console

`debug.print(...)` writes to the active debug console. On the Super Mini v4
reference firmware, the active debug console is exposed through `OUTPUT.GET`.
Output is bounded and line-oriented:

```text
BEGIN OUTPUT
output="count" 2
END OUTPUT
OK OUTPUT.GET
```

## Draw Log

Headless render mode records display commands rather than drawing pixels:

```text
BEGIN DRAWLOG
draw=clear color=gray0
draw=text text="Hello" x=10 y=20
END DRAWLOG
OK DRAWLOG.GET
```

This is a development observation surface for the ESP32-C3 Super Mini. It is
not a replacement for target display drivers.

## Hardware GPIO

Reference firmware accepts `hardware.gpio.*` SQBC builtins through uploaded
programs and REPL snippets. These builtins compile as part of the portable
SquidScript runtime API. Device alias resolution happens in firmware/runtime;
missing aliases or capabilities fail at runtime on the device.

The ESP32-C3 Super Mini target currently exposes:

- `indicator.status_led`, with aliases `status_led` and `status`, as the
  logical onboard LED. It is active-low on typical Super Mini boards.
- `GPIO8` as the raw pin name for the same line.

Normal quick upload/run uses volatile `squidc run` and does not require a target:

```sh
cargo run -p squidc -- run examples/blinky-supermini/main.squid
cargo run -p squidc -- device key SELECT
cargo run -p squidc -- device output
cargo run -p squidc -- device monitor --max-lines 4
```

Scripted REPL checks can still use session files:

```sh
cargo run -p squidc -- repl --script tests/repl/hardware-gpio-status-led.session
cargo run -p squidc -- repl examples/blinky-supermini/main.squid --script tests/repl/blinky-supermini.session
```

Target definitions are optional in host tooling. Use them only for explicit
compatibility checks and related metadata workflows:

```sh
cargo run -p squidc -- repl --target targets/esp32c3-super-mini.target.json --check-target --script examples/blinky-supermini/main.squid
```

The first check verifies serial-observable GPIO readback. The blinky example
also needs physical LED observation: `SELECT` toggles the onboard LED and
`BACK` turns it off and exits.
