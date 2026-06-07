# squidc CLI

`squidc` is the host compiler and Zephyr-firmware control CLI.

Normal SquidScript compilation and upload does not require a target definition.
Apps compile against the portable language/runtime API. Device aliases and
hardware capabilities are resolved by firmware/runtime on the target device.

Target definitions are opt-in metadata for target checks, simulator
configuration, firmware build metadata, docs, and autocomplete.

## Common Commands

```sh
cargo run -p squidc -- app build examples/blinky-supermini/main.squid --out target/blinky.sqbc
cargo run -p squidc -- app package examples/blinky-supermini
cargo run -p squidc -- app run examples/blinky-supermini/main.squid
cargo run -p squidc -- app test examples/app-tests/portable
cargo run -p squidc -- app test --negative tests/app-tests/negative
cargo run -p squidc -- fmt examples
cargo run -p squidc -- fmt --check examples/ble-install/main.squid
cargo run -p squidc -- repl --script tests/repl/default-dev.session
cargo run -p squidc -- doctor
```

`app run` compiles the input app, uploads it through the Zephyr temp-run
command, and launches it as a temporary foreground app. Firmware stages the
temp SQBC as a temporary app-store file instead of buffering the payload in
RAM. It does not publish an installed app, does not overwrite `main`, and keeps
temp app state volatile. The temp app participates in normal foreground
lifecycle routing: key events and foreground timers dispatch to it while it is
current, it can launch installed apps and return through `app.exit`, and a new
temp run replaces the prior temp route.

## Source Formatting

```sh
cargo run -p squidc -- fmt examples/ble-install/main.squid
cargo run -p squidc -- fmt examples docs/reference/binbook-reader-draft
cargo run -p squidc -- fmt --check examples
cargo run -p squidc -- fmt --stdin < examples/ble-install/main.squid
```

`fmt` canonicalizes `.squid` source files using the current parser. File and
directory paths are accepted; directories are scanned recursively for `.squid`
files. The default mode rewrites files in place. `--check` reports files that
would be reformatted and leaves them untouched. `--stdin` reads one source file
from standard input and writes formatted source to standard output.

The formatter requires parser-clean source. SquidScript comments are not a
recognized source syntax today, so there is no formatter comment preservation
policy until comments are added to the language.

## Scripted REPL Checks

`repl --script <file>` accepts SquidScript source interleaved with colon
commands for hardware and protocol assertions. The diagnostic commands flush
the current source snippet before reading firmware state:

```text
:output
:expect-output lifecycle ok
:trace
:expect-trace app.launch reader
:state
:expect-state count=1
:drawlog
:expect-draw draw=text
```

Use `:trace` and `:expect-trace` for lifecycle and service trace assertions in
scripted hardware checks.

## App Commands

```sh
cargo run -p squidc -- app build examples/blinky-supermini/main.squid --out target/blinky.sqbc
cargo run -p squidc -- app package examples/blinky-supermini
cargo run -p squidc -- app package examples/blinky-supermini --out target/blinky-supermini.squid.zip
cargo run -p squidc -- app run examples/blinky-supermini/main.squid
cargo run -p squidc -- app test examples/app-tests/portable
cargo run -p squidc -- app test --negative tests/app-tests/negative
cargo run -p squidc -- app test --list examples/app-tests/portable
cargo run -p squidc -- app install examples/blinky-supermini/main.squid
cargo run -p squidc -- app install blinky-supermini.squid.zip
cargo run -p squidc -- app push SquidScript target/installed-app.sqbc
cargo run -p squidc -- app install --as reader tests/hardware/c3-supermini/generic-events/reader-clock.squid
cargo run -p squidc -- app launch reader
cargo run -p squidc -- app list
```

`app build` compiles a SquidScript app or source file to SQBC.

`app package <app-dir>` expects `<app-dir>/main.squid`, compiles it to package
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

`app test <path>` discovers small example-backed tests. A positive app test is
a directory containing `main.squid` and a sibling `test.session`. The source is
compiled and installed through the existing scripted REPL runner, then the
session drives hardware assertions such as `:output`, `:state`, `:trace`, and
`:expect-*`. Passing a directory scans recursively, so
`examples/app-tests/portable` is the portable app regression suite. Use
`--port <serial-port>` when auto-detection is ambiguous. Use `--list` to inspect
the selected tests without touching hardware.

`app test --negative <path>` discovers compile-failure fixtures. A negative
fixture is a directory containing `main.squid` and `expected.txt`; the first
non-empty line of `expected.txt` must match a compiler diagnostic code or
message fragment. Negative tests are host-only and do not require connected
hardware.

`app push <device-name-or-address> <file.sqbc>` uploads SQBC over the custom
BLE GATT file-transfer service. The target app must already be running a
`service.ble.start("file-transfer", ...)` profile that accepts `.sqbc`; the
receiving app decides whether to call `app.install(ev.upload)` or handle the
file another way. The CLI matches the BLE peripheral by advertised name or
address, writes `.sqbc` as the transfer file name, uses write-without-response
for data chunks when the characteristic supports it, and waits for the firmware
completion notification.

## Device Commands

```sh
cargo run -p squidc -- device key SELECT
cargo run -p squidc -- device output
cargo run -p squidc -- device state
cargo run -p squidc -- device drawlog
cargo run -p squidc -- device trace
cargo run -p squidc -- device lifecycle
cargo run -p squidc -- device errors
cargo run -p squidc -- device resources
cargo run -p squidc -- device runtime-cap get
cargo run -p squidc -- device runtime-cap set vm_runtime.timer_max 2
cargo run -p squidc -- device reset
cargo run -p squidc -- device storage-format
cargo run -p squidc -- device wifi-profile dev --ssid-env SQUID_WIFI_STATION_SSID --password-env SQUID_WIFI_STATION_PASSWORD
cargo run -p squidc -- device monitor --max-lines 4
```

`device key` sends a logical key event to Zephyr firmware. It does not
press a physical button; the firmware routes the event to the current app.

`device drawlog` returns the current Zephyr headless display draw log. Records
use the current firmware diagnostic text shape, such as
`draw=clear color=gray0`, `draw=text text="Hello" x=10 y=20`,
`draw=rect x=1 y=2 w=3 h=4`, and `draw=line x1=5 y1=6 x2=7 y2=8`.

`device lifecycle` returns current Zephyr app lifecycle diagnostics as lines
such as `active=reader`, `process_stack[0]=launcher`, and `armed_stack=`.
When armed timers are registered, additional lines use
`armed_stack[0]=break-reminder timer.break`.
With `--json`, the raw lines are preserved and the same information is also
parsed into `data.active`, `data.processStack`, and `data.armedStack`. Armed
stack entries are objects with `appId` and `event` fields.

`device errors` is empty when no firmware runtime error is active. If a VM
dispatch fails, Zephyr reports a line such as
`runtime=vm_error code=-5 (EIO)` or
`runtime=invalid_argument code=-22 (EINVAL)`, preserving both the FFI status
class and the mapped errno. When the retained diagnostic ring is larger than
the response budget, firmware returns the newest lines that fit and includes
`errors_truncated=N`.

`device resources` reads Zephyr firmware resource diagnostics and reports
raw target-specific RAM and app-storage byte counts. `ram_total_bytes` is
static board context; `vm_stack_*` fields expose the configured VM work
queue stack size and Zephyr stack high-water usage when stack initialization is
enabled; `heap_*` fields are live allocator telemetry from the running
firmware; `runtime_static_bytes` is the resident VM runtime object after
internal buffer sharing; `vm_sqbc_chunk_bytes` is the bounded SQBC read/code
window used for file-backed installed app dispatch.
Use `device resources --reset-heap-max` at a workload boundary to reset
Zephyr's heap allocation high-water statistic to the current allocated bytes
before measuring later work.
`last_dispatch_seq`, `last_dispatch_us`,
`last_sqbc_reads`, and `last_sqbc_bytes` report
firmware-owned metrics for the most recent VM dispatch and are intended for
hardware benchmarks that must exclude host serial latency. `runtime_status`,
`runtime_dispatch_started`, `runtime_dispatch_age_us`,
`runtime_work_submitted`, `runtime_current_app_present`,
`runtime_lifecycle_phase`, and `runtime_arm_phase` are lockup triage metrics:
if serial remains responsive while app launch, input dispatch, or a service
call appears stuck, compare these fields with the stack and heap fields before
treating GPIO, flashing, or serial as the primary failure. With `--json`,
parsed values are returned under `data.resources`. Firmware diagnostics should
distinguish volatile temp-run state from installed-app code cache and app-store
usage.

`device runtime-cap` reads and writes active runtime cap overrides without
rebuilding firmware. `get` prints every active cap, or one key when a key is
provided. `set <key> <value>` persists a non-default active cap to
firmware-owned runtime config; values must be greater than zero, no larger than
the build-time hard cap, and not below currently active entries. `clear <key>`
restores one key to its hard cap, and `clear` with no key restores all tunable
active caps to their hard caps. Supported keys are listed in
`docs/runtime_limits.md`.

`device reset` performs a firmware soft boot. It clears the current VM, temp
app, foreground stack, pending launches, trigger/timer registrations, and debug
buffers, then boots installed `main` when present. It does not erase installed
apps.

`device storage-format` erases Zephyr app storage, including installed apps,
resources, temp app staging, and app state files, then recreates the expected
storage directories. Firmware may report bounded pending progress internally;
the CLI repeats the framed command until the final success response so callers
still see one completed command.

`device wifi-profile` provisions a volatile Wi-Fi station profile through the
current framed Zephyr command surface. It reads the SSID and password from
environment variable names passed with `--ssid-env` and `--password-env` so
normal command output can report only the profile name and byte lengths. Do not
use `protocol raw` for credentials unless raw request hex is explicitly needed
and safe for the current environment.

`device monitor` polls the firmware output buffer by default. Use `--raw` only
when literal serial bytes are needed. JSON monitor output must be bounded with
`--max-lines`.

## Protocol Escape Hatch

```sh
cargo run -p squidc -- protocol raw hello --seq 1 --string 1=esp32c3-supermini
cargo run -p squidc -- protocol raw resources-get --u32 1=409600
```

Use `protocol raw` for low-level Zephyr protocol troubleshooting only. It sends
one binary framed request, not a text command. Field options are typed TLV
entries: `--string TAG=VALUE`, `--bytes TAG=HEX`, `--bool TAG=true|false`,
`--u32 TAG=VALUE`, `--u64 TAG=VALUE`, and `--i64 TAG=VALUE`. Prefer grouped `app` and `device`
commands for normal workflows.

Framed command, app lifecycle, diagnostics, state, resources, and storage
operations are implemented in the host and Zephyr serial transport layers.

## Target Commands

`target` commands are the canonical firmware-target workflow. They read
repository target JSON metadata and resolve Zephyr board, build directory,
overlay, fallback app, and generated Kconfig paths from that metadata.

```sh
cargo run -p squidc -- target list
cargo run -p squidc -- target inspect --target xiao-esp32c3-gdeq0426t82-sd
cargo run -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd
cargo run -p squidc -- target flash --target xiao-esp32c3-gdeq0426t82-sd
cargo run -p squidc -- target monitor --target xiao-esp32c3-gdeq0426t82-sd
cargo run -p squidc -- target doctor --target xiao-esp32c3-gdeq0426t82-sd
cargo run -p squidc -- hardware test --target xiao-esp32c3-gdeq0426t82-sd
cargo run -p squidc -- hardware test --target xiao-esp32c3-gdeq0426t82-sd --list
```

Use `target inspect` or `--print-plan` before side-effectful operations when
automation needs to verify the resolved command without invoking Zephyr:

```sh
cargo run -p squidc -- target build --target esp32c3-super-mini --print-plan
cargo run -p squidc -- target flash --target esp32c3-super-mini --print-plan -- --runner esp32
```

When `--target` is omitted, interactive terminals show a target picker.
Noninteractive sessions fail and should pass `--target <target-id>` explicitly.

`target flash` builds first, then flashes. It monitors only when
`--monitor-after-flash` is passed. `target monitor` is a streaming hardware
command; with `--json`, use `--print-plan` instead of starting the stream.

`hardware test --target <target-id>` runs the target-aware hardware regression
checks selected from target metadata features. The XIAO ESP32-C3 default dev
target currently selects portable app tests, BLE file-transfer install, BLE
reconnect, radio concurrency, and AP-after-station checks. It excludes display
drawlog and SD-card checks until those capabilities are ready for this target.
Use `--skip-flash` when the correct firmware is already flashed. Use
`--ble-device <name-or-address>` to override BLE matching and
`--host-wifi-iface <iface>` when Wi-Fi tests should use a specific host
interface.

## JSON

All commands accept global `--json`:

```sh
cargo run -p squidc -- --json doctor
cargo run -p squidc -- --json app list
cargo run -p squidc -- --json target inspect --target esp32c3-super-mini
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

Automation and AI agents should use `--json` for machine-readable output and
must not parse human output. Commands that invoke child tools keep JSON stdout
structured; child-tool logs are written to stderr.

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
cargo run -p squidc -- app run examples/blinky-supermini/main.squid
```

Explicit target check:

```sh
cargo run -p squidc -- app run examples/blinky-supermini/main.squid \
  --target targets/esp32c3-super-mini.target.json \
  --check-target
```
