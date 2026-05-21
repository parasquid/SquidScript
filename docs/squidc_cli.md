# squidc CLI

`squidc` is the host compiler and reference-firmware control CLI.

Normal SquidScript compilation and upload does not require a target definition.
Apps compile against the portable language/runtime API. Device aliases and
hardware capabilities are resolved by firmware/runtime on the target device.

Target definitions are opt-in metadata for compatibility checks, simulator
configuration, firmware build metadata, docs, and autocomplete.

## Common Commands

```sh
cargo run -p squidc -- build examples/blinky-supermini/main.squid --out target/blinky.sqbc
cargo run -p squidc -- package examples/binbook-reader
cargo run -p squidc -- run examples/blinky-supermini/main.squid
cargo run -p squidc -- repl --script tests/repl/default-dev.session
cargo run -p squidc -- doctor
```

`run` compiles the input app, uploads it with `RUN.TEMP`, and launches it as a
temporary foreground app. It does not write flash, does not overwrite `main`,
and is intended for quick hardware checks and rapid iteration. Before 1.0,
`RUN.TEMP` stays RAM-backed by design so repeated `squidc run` loops do not
wear flash.

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
cargo run -p squidc -- device monitor --max-lines 4
```

`device key` sends a logical key event to the reference firmware. It does not
press a physical button; the firmware routes the event to the current app.

`device resources` reads the firmware `RESOURCES.GET` diagnostics and reports
raw target-specific RAM and app-storage byte counts. With `--json`, parsed
values are returned under `data.resources`. Firmware reports `RUN.TEMP`
buffer usage separately from installed-app code cache usage, so comparisons can
distinguish volatile developer runs from persistent chunk execution.

`device reset` performs a firmware soft boot. It clears the current VM, temp
app, foreground stack, pending launches, trigger/timer registrations, and debug
buffers, then boots installed `main` when present. It does not erase installed
apps; use `STORAGE.FORMAT` through the protocol or the storage-format helper
when a clean app store is needed.

`device monitor` polls the firmware output buffer by default. Use `--raw` only
when literal serial bytes are needed. JSON monitor output must be bounded with
`--max-lines`.

## Protocol Escape Hatch

```sh
cargo run -p squidc -- protocol raw APP.LIST
```

Use `protocol raw` for low-level protocol troubleshooting only. Prefer grouped
`app` and `device` commands for normal workflows.

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
- the `riscv32imc-unknown-none-elf` Rust target
- the RISC-V ELF GCC toolchain required by the LittleFS firmware build
- `espflash`, including `~/.cargo/bin/espflash`
- visible serial ports
- optional `riscv64-elf-size`
- firmware `HELLO` when exactly one candidate device is visible or `--port` is
  supplied
- hardware target test script presence

Hardware target tests and serial/flashing commands must run outside the Codex
sandbox.

## Target Checks

Normal compile/upload:

```sh
cargo run -p squidc -- run examples/blinky-supermini/main.squid
```

Explicit compatibility check:

```sh
cargo run -p squidc -- run examples/blinky-supermini/main.squid \
  --target targets/esp32c3-super-mini.target.json \
  --check-target
```
