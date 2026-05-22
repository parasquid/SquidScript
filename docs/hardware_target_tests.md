# Hardware Target Tests

Hardware target tests exercise a connected physical board. They are not unit
tests. Never run them in parallel against the same serial device: concurrent
flash, install, monitor, REPL, hardware-test, or `squidc device` commands can
interleave serial bytes, reset the board, steal foreground app state, or leave
hardware in a misleading state. Run one hardware command at a time and wait for
it to exit before starting the next command.

Hardware target tests and serial/flashing commands must run outside the Codex
sandbox. Sandboxed sessions do not reliably expose `/dev/ttyACM*`,
`/dev/ttyUSB*`, or `/dev/serial/by-id`, even after host reboot. Use escalated
command execution for ESP32-C3 Super Mini serial visibility checks and hardware
target tests:

```sh
cargo run -p squidc -- doctor
~/.cargo/bin/espflash list-ports --list-all-ports
./scripts/c3-supermini-test-hardware.sh
```

`squidc doctor` is read-only. It reports toolchain, optional size tooling,
serial visibility, firmware handshake, and hardware-test script readiness.
See `docs/squidc_cli.md` for the grouped `squidc` command surface.

The suite is ordered so tests that reset, install, or replace apps run before
visual/interactive checks. Blinky runs last because it intentionally leaves the
board in an observable state with the onboard LED toggling. If a later test
installs another app, it will replace that visible state.

The current hardware target is the ESP32-C3 Super Mini reference firmware on
the first auto-detected SquidScript firmware serial target. Set
`ESPFLASH_PORT=/path/to/device` when multiple devices are connected or
auto-detection is not enough.

## ESP32-C3 Super Mini

### Full Sequence

```sh
./scripts/c3-supermini-test-hardware.sh
./scripts/c3-supermini-test-hardware.sh --skip-flash
```

Runs the hardware target checks sequentially, verifies blinky timer output, and
then deliberately launches the blinky app again as the final serial action.
This leaves the onboard LED visibly active after the automated serial checks
finish.

Current order:

1. Reference firmware protocol test.
2. GPIO REPL session.
3. Default dev REPL session.
4. Persistent app registry test.
5. Timer-armed app test.
6. Generic triggered-apps test.
7. Blinky REPL session.
8. Volatile blinky app run, timer-output assertion, app-list persistence check,
   and final blinky launch with no later serial command.

Add new hardware tests before the final blinky app unless the new test is also
intended to be the final visible board-state check.

### Firmware Flash

```sh
./scripts/c3-supermini-build.sh
./scripts/c3-supermini-flash.sh
```

Builds and flashes the reference firmware. Normal flashing writes the
bootloader, partition table, and factory app partition. It does not erase the
`squidfs` app-storage partition. Use the persistent registry test or
`STORAGE.FORMAT` when a clean app store is needed.

### Reference Firmware Protocol

```sh
./scripts/c3-supermini-test-reference-firmware.sh
./scripts/c3-supermini-test-reference-firmware.sh --skip-flash
```

Builds/flashes the reference firmware unless `--skip-flash` is used, installs
the headless counter fixture as `main`, and verifies run, key, state, and trace
behavior over USB serial.

### Persistent App Registry

```sh
./scripts/c3-supermini-test-persistent-app-registry.sh
```

Formats app storage, installs the headless counter fixture as `main`, runs it,
presses `SELECT` twice to save `count=2`, resets the chip, verifies `APP.LIST`
still reports `main`, then reads state to prove boot-time `state.load()`
restored `count=2` and dispatched `app.start` after reset. This test proves
installed SQBC and installed-app state survive a real firmware restart.

### Blinky App

```sh
cargo run -p squidc -- run examples/blinky-supermini/main.squid
cargo run -p squidc -- device output
cargo run -p squidc -- device monitor --max-lines 6
```

Compiles, uploads, and runs the blinky SquidScript app as a temporary
foreground app through `RUN.TEMP`. It must not persist to app storage or
overwrite `main`. The serial output should include `blinky ready` followed by
alternating `blink` values. The onboard LED should visibly toggle.

`squidc device monitor` polls the firmware debug output buffer and prints newly
observed `output=...` lines to the terminal. A real blinky check must observe
both `output="blink" true` and `output="blink" false`; startup output alone is
not enough. Use `--raw` only when literal serial bytes are needed.

### Wi-Fi AP Diagnostics App

```sh
cargo run -p squidc -- app install examples/wifi-ap-diagnostics/main.squid
cargo run -p squidc -- app launch wifi-ap-diagnostics
cargo run -p squidc -- device output
cargo run -p squidc -- device key SELECT
cargo run -p squidc -- device output
```

Use the separate Wi-Fi state script when Wi-Fi behavior is under test:

```sh
scripts/c3-supermini-test-wifi-state.sh
```

This script installs the AP diagnostics example as a persistent app, starts
`service.wifi.startAP("SquidScript")`, checks firmware-reported AP state through
`WIFI.STATUS`, then sends `SELECT` to stop the AP and checks the inactive state.
It is intentionally not part of `scripts/c3-supermini-test-hardware.sh`, which
keeps blinky as the final visible board-state check.

The diagnostics app prints the start result, expanded status record, AP IP
record, and periodic client-count changes. `status.state == "started"`,
`status.driverStarted == true`, `status.configured == true`, `status.mode ==
"ap"`, and `status.ipAddress == "192.168.4.1"` prove internal firmware/driver
state only. They do not prove that nearby devices can see or join the AP.

The example owns the indicator behavior in SquidScript:

- waiting for clients: `service.indicator.breathe()`
- first observed client: fast `service.indicator.toggle()` blink burst
- connected: mostly lit with short off blinks
- all clients disconnected: returns to `service.indicator.breathe()`

While the AP is active, optionally verify from a phone or laptop that the
`SquidScript` network is visible, attempt to join it, confirm serial output
reports `status.clients > 0`, then disconnect and confirm `status.clients`
returns to zero. Use `STRICT_PROBE=1 scripts/c3-supermini-test-wifi-state.sh`
only when deliberately treating probe/client events as a required hardware
check. Until Rust AP beacon visibility is proven on hardware, treat successful
firmware status records as internal radio-driver state only; IP/DHCP and HTTP
serving are separate follow-up work.

For public station API coverage, use:

```sh
scripts/c3-supermini-test-wifi-station-api.sh
```

The script reads `HWTEST_STA_SSID` and `HWTEST_STA_PASSWORD` from the
environment or `HWTEST_ENV_FILE`, provisions volatile profile `dev` over serial
without echoing credential values, installs `examples/wifi-station-diagnostics`,
and checks `service.wifi.connect("dev")` plus `service.wifi.status()` output.
The default pass requires the Rust backend to enter real station mode and prove
`connected=true` with the provisioned network. Use `STRICT_STA_CONNECT=0`
only when deliberately collecting scan, auth, or association failure
diagnostics from a local network environment expected not to complete.

For public Wi-Fi scan API coverage, use:

```sh
scripts/c3-supermini-test-wifi-scan-api.sh
```

The script installs `examples/wifi-scan-diagnostics`, launches it, and requires
either a real AP list or a concrete driver scan failure without printing
credential-related fields. The old ESP32-C3 `unsupported` scan stub is a
failure.

The attached ESP32-C3 Super Mini can detect nearby Wi-Fi networks. ESP-IDF
scan-only hardware isolation reports multiple AP records on this board. If the
Rust `esp-radio` or SquidScript firmware path reports zero scan records, treat
that as a Rust firmware or driver-integration issue until a fresh ESP-IDF
scan-only run says otherwise. Start debugging at the Rust Wi-Fi stack, scan
configuration, driver lifecycle, scheduler/runtime integration, or result
extraction.

### ESP-IDF Wi-Fi Hardware Isolation

When `service.wifi.startAP(...)` reports success but phones or laptops cannot
see the AP, or when station-mode Wi-Fi fails in the SquidScript firmware,
isolate hardware before continuing SquidScript firmware changes. Use the
ESP-IDF Wi-Fi experiment:

```sh
cd experiments/esp32c3-supermini/firmware/esp-idf-softap-hwtest
./build.sh
./flash-monitor.sh /dev/ttyACM0
```

This is intentionally outside the SquidScript firmware stack. It uses
Espressif's ESP-IDF C Wi-Fi driver, FreeRTOS, `esp_netif`, and DHCP path so it
can rule out board/RF issues independently of Rust, `esp-radio`, Embassy, or the
SquidScript runtime.

ESP-IDF is an external developer dependency. Do not vendor ESP-IDF, generated
SDK files, container layers, or `build/` outputs into this repository. The
experiment scripts use a local `idf.py` when available, or the official
Espressif IDF container image via Podman/Docker.

The test AP is:

- SSID: `ESP32C3-HWTEST`
- password: open network
- channel: `6`
- expected AP IP: `192.168.4.1`

For SoftAP, test channels `1`, `6`, and `11` by editing `HWTEST_WIFI_CHANNEL` in the
experiment source and rebuilding. A reliable hardware pass means the SSID is
visible from a phone/laptop, the device can join, and serial logs show station
join/leave plus connected station counts.

For scan-only hardware isolation, build the scan variant. It does not need or
use Wi-Fi credentials:

```sh
cd experiments/esp32c3-supermini/firmware/esp-idf-softap-hwtest
./build-scan.sh
./flash-with-espflash.sh /dev/ttyACM0
timeout 28s ~/.cargo/bin/espflash monitor --chip esp32c3 --port /dev/ttyACM0 \
  --non-interactive --after hard-reset --monitor-baud 115200 --log-format serial
```

Representative scan-only ESP-IDF output, with network identifiers redacted:

```text
I (245) scan_hwtest: ESP32-C3 scan-only hardware test using ESP-IDF
I (245) scan_hwtest: starting unfiltered scan
I (2745) scan_hwtest: scan found 4 AP record(s)
I (2745) scan_hwtest: scan[0] ssid_len:22 bssid:<redacted> channel:2 rssi:-36 auth:WPA_WPA2_PSK
I (2745) scan_hwtest: scan[1] ssid_len:10 bssid:<redacted> channel:8 rssi:-65 auth:WPA2_PSK
I (2745) scan_hwtest: scan[2] ssid_len:0 bssid:<redacted> channel:8 rssi:-65 auth:WPA2_PSK
I (2745) scan_hwtest: scan[3] ssid_len:23 bssid:<redacted> channel:5 rssi:-70 auth:WPA_WPA2_PSK
I (15245) scan_hwtest: scan found 5 AP record(s)
```

This proves the attached ESP32-C3 Super Mini can detect nearby networks with
Espressif's ESP-IDF Wi-Fi stack.

For station mode, put the target network SSID and password in an untracked env
file such as `~/.env` as `HWTEST_STA_SSID` and `HWTEST_STA_PASSWORD`, then
build the station variant:

```sh
cd experiments/esp32c3-supermini/firmware/esp-idf-softap-hwtest
./build-station.sh
./flash-monitor.sh /dev/ttyACM0
```

The station test does not print the configured SSID or password values; it logs
their lengths, scan matches, BSSID, channel, RSSI, auth mode, disconnect reason
labels, and IP configuration if the join succeeds. It disables Wi-Fi power save
and uses a relaxed WPA/WPA2 auth threshold to avoid making mixed-mode routers
look like hardware failures.

Interpretation:

- ESP-IDF AP visible and joinable: hardware/RF is probably fine; continue
  debugging the SquidScript Rust firmware, `esp-radio`, and scheduler/network
  runner architecture.
- ESP-IDF scan-only sees AP records while SquidScript/Rust scan returns zero:
  hardware/RF is proven capable of scanning in this environment; debug the Rust
  `esp-radio` scan lifecycle and result handling before blaming the board.
- ESP-IDF AP also invisible: suspect board/RF/antenna, USB power, physical
  placement, or local RF environment before blaming SquidScript.
- AP visible only at very short range or when the board is moved/touched:
  suspect ESP32-C3 SuperMini antenna/layout sensitivity on the specific board.
- AP visible but DHCP/join fails: RF beaconing works; isolate ESP-IDF AP config,
  DHCP, channel, or client compatibility separately.
- Station scan sees the AP but disconnects with `AUTH_EXPIRE` or
  `CONNECTION_FAIL`: the board can receive the router beacon, but auth did not
  complete. Check password handling, router mixed WPA/WPA2 settings, PMF/router
  compatibility, and transmit/RF strength before blaming SquidScript.
- MicroPython station scan may see the target AP with strong RSSI while the
  connection still fails to reach `STAT_GOT_IP`. Record the numeric status code
  and the published MicroPython status constants from the same firmware before
  assigning a meaning to the code; observed firmware reported status `2`, which
  did not match its exported `STAT_*` constants.
- Station gets an IP address: ESP-IDF station RX/TX/auth/DHCP works on this
  board and network; focus back on SquidScript firmware, `esp-radio`, and the
  scheduler/network runner.

Known flashing caveat: containerized `idf.py flash` may fail to open
`/dev/ttyACM0` even when the host user is in `dialout`. Use a local ESP-IDF
install for flashing when possible. The experiment also includes an
`espflash`-based helper for host-side flashing, but ESP-IDF's own `idf.py flash`
is the reference path for this hardware-isolation test.

### Rust Wi-Fi AP Probe Experiments

After hardware isolation, use the Rust AP probes to compare esp-rs paths before
changing the SquidScript firmware:

```sh
cd experiments/esp32c3-supermini/firmware/wifi-ap-probe
./build.sh
ESPFLASH_PORT=/dev/ttyACM0 ./flash.sh
```

The blocking probe follows the release-matched esp-rs blocking `access_point`
example with `esp-radio` 0.17 and a smoltcp-backed blocking network stack.

```sh
cd experiments/esp32c3-supermini/firmware/embassy-wifi-ap-probe
./build.sh
ESPFLASH_PORT=/dev/ttyACM0 ./flash.sh
```

The Embassy probe follows the current upstream `embassy_access_point` shape
with `esp-radio` 0.18, `esp-rtos`, `embassy-net`, DHCP, and an HTTP listener.

Both Rust probes currently use the SSID `esp-radio` so host scans can compare
them against the upstream examples. If serial reports a successful AP start but
host scans still do not show the SSID, keep the result as an esp-rs/Rust
firmware investigation item rather than changing SquidScript language or app
semantics.

Current AP visibility caveat: repeated host scans have also failed to see a
MicroPython AP on the same board even while MicroPython reports AP active and
configured. A previous MicroPython AP run was visible, so treat host scan
results as part of the hardware/RF/client matrix until an alternate client
such as a phone confirms whether the AP beacon is visible.

### GPIO REPL Session

```sh
cargo run -p squidc -- repl --script tests/repl/hardware-gpio-indicator.session
```

Exercises `service.indicator.write`, `service.indicator.read`,
`service.indicator.toggle`, `service.indicator.breathe`, and raw `GPIO8`
readback.

### Blinky REPL Session

```sh
cargo run -p squidc -- repl examples/blinky-supermini/main.squid --script tests/repl/blinky-supermini.session
```

Runs the blinky app through the REPL workflow, sends key events, and verifies
serial output/state.

### Default Dev REPL Session

```sh
cargo run -p squidc -- repl --script tests/repl/default-dev.session
```

Verifies the default dev profile REPL behavior without requiring an explicit
`:profile dev` command.

### Timer-Armed App

```sh
./scripts/c3-supermini-test-timer-armed-app.sh
```

Installs the timer armed-app example and verifies `app.arm`,
`service.timer.*`, and timer-dispatched output on real firmware. The test polls
device output until the expected timer lines arrive instead of relying on a
fixed sleep.

### Generic Triggered Apps

```sh
./scripts/c3-supermini-test-generic-triggered-apps.sh
```

Installs the generic-events hardware fixture set from
`tests/hardware/c3-supermini/generic-events` and verifies app start/arm flow,
timer events, triggered app behavior, and key-event handling on real firmware.
The timer portion polls for expected output with a timeout instead of relying on
a fixed sleep.

## Rules

- Run these checks sequentially against a given board.
- Do not leave `squidc device monitor` open while running an install, flash,
  REPL, or hardware test script.
- Do not run a second serial command while any monitor, flash, install, REPL,
  or hardware script is still running.
- Prefer auto-detected ports for normal `squidc` flows. Pass `--port` only when
  the host has multiple candidate serial devices or auto-detection fails.
- Use `squidc device reset` to clear VM state, timers, trace, output, and temp
  apps between independent tests. Avoid full chip resets except for persistence
  and boot behavior checks because USB re-enumeration adds timing noise.
- If the device is visible but access fails, use the documented ACL workaround:
  `sudo setfacl -m u:$USER:rw /dev/ttyACM0`.
