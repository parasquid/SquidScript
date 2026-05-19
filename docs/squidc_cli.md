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
cargo run -p squidc -- run examples/blinky-supermini/main.squid
cargo run -p squidc -- repl --script tests/repl/default-dev.session
cargo run -p squidc -- doctor
```

`run` compiles the input app, uploads it with `RUN.TEMP`, and launches it as a
temporary foreground app. It does not write flash, does not overwrite `main`,
and is intended for quick hardware checks.

## App Commands

```sh
cargo run -p squidc -- app install examples/blinky-supermini/main.squid
cargo run -p squidc -- app install --as reader tests/hardware/c3-supermini/generic-events/reader-clock.squid
cargo run -p squidc -- app launch reader
cargo run -p squidc -- app list
```

`app install` accepts `.squid` source or `.sqbc` bytecode. Source is compiled
before upload. `.sqbc` input must include app-id metadata unless `--as` is
provided. Use `app install` plus `app launch` for persistent apps.

## Device Commands

```sh
cargo run -p squidc -- device key SELECT
cargo run -p squidc -- device output
cargo run -p squidc -- device state
cargo run -p squidc -- device drawlog
cargo run -p squidc -- device trace
cargo run -p squidc -- device errors
cargo run -p squidc -- device reset
cargo run -p squidc -- device monitor --max-lines 4
```

`device key` sends a logical key event to the reference firmware. It does not
press a physical button; the firmware routes the event to the current app.

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
