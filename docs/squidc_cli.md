# squidc CLI

`squidc` is the host compiler and Zephyr-firmware control CLI.

Normal SquidScript compilation and upload does not require a target definition.
Apps compile against the portable language/runtime API. Device aliases and
hardware capabilities are resolved by firmware/runtime on the target device.

Target definitions are opt-in metadata for target checks, simulator
configuration, firmware build metadata, docs, and autocomplete.

## Common Commands

```sh
cargo run -p squidc -- build examples/blinky-supermini/main.squid --out target/blinky.sqbc
cargo run -p squidc -- package examples/binbook-reader
cargo run -p squidc -- run examples/blinky-supermini/main.squid
cargo run -p squidc -- repl --script tests/repl/default-dev.session
cargo run -p squidc -- doctor
```

`run` compiles the input app, uploads it through the Zephyr temp-run command,
and launches it as a temporary foreground app. Firmware stages the temp SQBC as
a temporary app-store file instead of buffering the payload in RAM. It does not
publish an installed app, does not overwrite `main`, and keeps temp app state
volatile.

## App Commands

```sh
cargo run -p squidc -- package examples/binbook-reader
cargo run -p squidc -- package examples/binbook-reader --out target/binbook-reader.squid.zip
cargo run -p squidc -- app install examples/blinky-supermini/main.squid
cargo run -p squidc -- app install binbook-reader.squid.zip
cargo run -p squidc -- app install --as reader tests/hardware/c3-supermini/generic-events/reader-clock.squid
cargo run -p squidc -- app launch reader
cargo run -p squidc -- app list
```

`package <app-dir>` expects `<app-dir>/main.squid`, compiles it to package
entry `main.sqbc`, and writes `<app-id>.squid.zip` in the current directory
unless `--out` is provided. Package output includes safe non-source runtime
files from the app directory. It excludes `.squid` source files, dot-files,
dot-directories, `source-map.json`, existing `.squid.zip` outputs, and generated
`main.sqbc`.

`app install` accepts `.squid` source, `.sqbc` bytecode, or `.squid.zip`
packages. Source is compiled before upload. `.sqbc` input must include app-id
metadata unless `--as` is provided. Package installs derive the app ID from
`main.sqbc`; `--as` is not supported for packages. Use `app install` plus
`app launch` for persistent apps.

## Device Commands

```sh
cargo run -p squidc -- device key SELECT
cargo run -p squidc -- device output
cargo run -p squidc -- device state
cargo run -p squidc -- device drawlog
cargo run -p squidc -- device trace
cargo run -p squidc -- device errors
cargo run -p squidc -- device resources
cargo run -p squidc -- device reset
cargo run -p squidc -- device storage-format
cargo run -p squidc -- device monitor --max-lines 4
```

`device key` sends a logical key event to Zephyr firmware. It does not
press a physical button; the firmware routes the event to the current app.

`device drawlog` returns the current Zephyr headless display draw log. Records
use the current firmware diagnostic text shape, such as
`draw=clear color=gray0`, `draw=text text="Hello" x=10 y=20`,
`draw=rect x=1 y=2 w=3 h=4`, and `draw=line x1=5 y1=6 x2=7 y2=8`.

`device resources` reads Zephyr firmware resource diagnostics and reports
raw target-specific RAM and app-storage byte counts. `ram_total_bytes` is
static board context; `ram_heap_*` fields are live allocator telemetry from the
running firmware. With `--json`, parsed values are returned under
`data.resources`. Firmware diagnostics should distinguish volatile temp-run
state from installed-app code cache and app-store usage.

`device reset` performs a firmware soft boot. It clears the current VM, temp
app, foreground stack, pending launches, trigger/timer registrations, and debug
buffers, then boots installed `main` when present. It does not erase installed
apps.

`device storage-format` erases Zephyr app storage, including installed apps,
resources, temp app staging, and app state files, then recreates the expected
storage directories.

`device monitor` polls the firmware output buffer by default. Use `--raw` only
when literal serial bytes are needed. JSON monitor output must be bounded with
`--max-lines`.

## Protocol Escape Hatch

```sh
cargo run -p squidc -- protocol raw hello --seq 1 --string 1=esp32c3-supermini
cargo run -p squidc -- protocol raw resources-get --u64 1=409600
```

Use `protocol raw` for low-level Zephyr protocol troubleshooting only. It sends
one binary framed request, not a text command. Field options are typed TLV
entries: `--string TAG=VALUE`, `--bytes TAG=HEX`, `--bool TAG=true|false`,
`--u64 TAG=VALUE`, and `--i64 TAG=VALUE`. Prefer grouped `app` and `device`
commands for normal workflows.

Framed command, app lifecycle, diagnostics, state, resources, and storage
operations are implemented in the host and Zephyr serial transport layers.

## JSON

All commands accept global `--json`:

```sh
cargo run -p squidc -- --json doctor
cargo run -p squidc -- --json app list
```

JSON output uses a stable envelope:

```json
{
  "ok": true,
  "command": "doctor",
  "data": {},
  "warnings": [],
  "errors": []
}
```

Failures use the same shape with `ok: false`, `data: null`, and one or more
entries in `errors`.

## Doctor

`doctor` is read-only. It checks host/toolchain/device readiness without
flashing firmware, installing apps, resetting the board, or running hardware
tests.

Checks include:

- `cargo`, `rustc`, and `rustup`
- `west` and Zephyr workspace readiness
- Zephyr SDK/toolchain support for the selected board
- visible serial ports
- optional Zephyr image/map size tooling
- firmware identity when exactly one candidate device is visible or `--port` is
  supplied
- hardware target test script presence

Hardware target tests and serial/flashing commands must run outside the Codex
sandbox.

## Target Checks

Normal compile/upload:

```sh
cargo run -p squidc -- run examples/blinky-supermini/main.squid
```

Explicit target check:

```sh
cargo run -p squidc -- run examples/blinky-supermini/main.squid \
  --target targets/esp32c3-super-mini.target.json \
  --check-target
```
