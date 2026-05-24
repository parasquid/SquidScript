# Hardware Target Tests

Hardware target tests exercise a connected physical board. They are not unit
tests. Never run hardware commands in parallel against the same serial device:
concurrent flash, monitor, REPL, hardware-test, or `squidc device` commands can
interleave serial bytes, reset the board, or leave hardware in a misleading
state.

Hardware target tests and serial/flashing commands must run outside the Codex
sandbox. Sandboxed sessions do not reliably expose `/dev/ttyACM*`,
`/dev/ttyUSB*`, or `/dev/serial/by-id`.

## Current Target

The current real firmware target is Zephyr-backed ESP32-C3 work under
`firmware/zephyr`.

Build and flash:

```sh
./scripts/c3-supermini-build.sh
SQUID_ZEPHYR_TARGET_JSON=targets/esp32c3-super-mini.target.json \
  ./scripts/zephyr-ram-audit.sh
./scripts/c3-supermini-flash.sh
```

Monitor:

```sh
./scripts/c3-supermini-zephyr-monitor.sh
```

The ESP32-C3 Super Mini wrappers default to Zephyr's `esp32c3_supermini` board
target. Set `ZEPHYR_BOARD` when testing a different ESP32-C3 board variant.

## Test Inventory

The default Zephyr-only hardware suite covers the current required inventory:

- Build and flash the Zephyr diagnostic firmware.
- Check Zephyr RAM budget output before flashing.
- Verify the diagnostic boot banner over the serial monitor.
- Install and launch real SquidScript apps through the Zephyr command surface.
- Verify `app.launch`, `app.exit`, `app.arm`, timer-triggered armed dispatch,
  `app.processStack()`, `app.armedStack()`, `app.armedStack.get(...)`, and
  `device lifecycle` process/armed stack diagnostics.
- Dispatch key events and verify state/output traces.
- Verify headless display draw-log records for `service.display.clear`,
  `service.display.select`, `service.display.image`, and
  `service.display.draw`.
- Verify persistent app storage and app state through Zephyr storage.
- Verify `system.memory()` and `system.storage("apps")` through the Zephyr VM
  FFI host.
- Verify app-facing installed-app inspection through `app.registry()` and
  `app.registry.get(...)`.
- Verify `device.config.load`, `device.config.set`, `device.config.rebind`,
  and `device.config.save` reach the Zephyr VM FFI host and save active SQDC
  config through Zephyr storage.
- Verify GPIO/indicator behavior, including a final visible board-state check.
- Verify Wi-Fi scan without credentials.
- Verify Wi-Fi station behavior only when credentials are explicitly provided
  through the separate station script.

Keep the suite ordered so stateful reset/install tests run before the final
visible board-state check.

`scripts/c3-supermini-zephyr-test-diagnostic.sh` builds and flashes the Zephyr
diagnostic image, then runs a bounded serial monitor check for the diagnostic
boot banner. It writes the captured monitor output to
`target/hardware-tests/diagnostic/boot-banner.log` and exits after the bounded
check instead of leaving a monitor attached.

RAM guards should be interpreted as target-profile limits, not universal
ESP32-C3 limits. The current no-target fallback guard is `266240` bytes. When
`SQUID_ZEPHYR_TARGET_JSON=targets/esp32c3-super-mini.target.json` is supplied,
the default 65% profile uses the target definition's 400 KiB internal SRAM and
sets a 266240-byte limit. The default Zephyr firmware now enables the real
Zephyr ESP32 Wi-Fi driver, Wi-Fi management events, scan/AP/station Wi-Fi
usage, AP DHCPv4 server support, and station DHCP/IP status reporting without
TCP. Its
measured `dram0_0_seg` must be read from the latest `scripts/zephyr-ram-audit.sh`
output for the firmware image under test. The ESP32-C3 reference configuration
uses bounded native-network packet and buffer pools sized for current
low-throughput control-plane Wi-Fi behavior and measured socket-service,
network-management event, ESP timer task, and network RX stack budgets; TCP,
HTTP, AP client throughput, or other bulk traffic must be remeasured before
increasing service scope. The Zephyr system heap is also measured from live
`device resources` heap high-water data after representative Wi-Fi status,
scan, list, and AP workloads; remeasure it before adding larger radio or
networking workloads.

`targets/esp32c3-super-mini.target.json` should describe these verified Zephyr
runtime services and defaults, not only the ESP32-C3 silicon radio capability.
The Zephyr build generates SquidScript-facing defaults such as
`indicator.default` from `SQUID_ZEPHYR_TARGET_JSON` and validates those
defaults against `SQUID_ZEPHYR_TARGET_OVERLAY`, while devicetree remains
responsible for board driver nodes. Wi-Fi status, scan, redacted network
listing, AP start/stop/IP lookup, and volatile-profile station
connect/disconnect are exposed through the reference firmware. Station connect
proof remains explicit-credentials-only, and AP client association/DHCP lease
proof is separate future work.

The Zephyr app lifecycle check is
`scripts/c3-supermini-test-app-lifecycle.sh`. It installs the real SquidScript
fixtures under `tests/hardware/c3-supermini/generic-events`, launches `main`,
verifies `reader-clock` starts via `app.launch`, verifies `break-reminder` is
registered on the armed stack through `app.arm`, waits for the armed timer to
start `break-reminder`, then sends `SELECT` so `app.exit` returns to the
previous app on the process stack. It also verifies app-facing lifecycle
inspection through `app.processStack()`, `app.armedStack()`, and
`app.armedStack.get(...)`. The lifecycle fixtures use volatile
in-memory counters and intentionally avoid `state.load()` / `state.save()` so
the check distinguishes ordinary foreground event dispatch from fresh VM
sessions on launch, armed trigger activation, and app-exit return.

The Zephyr app registry API check is
`scripts/c3-supermini-test-app-registry-api.sh`. It formats app storage,
installs `tests/hardware/c3-supermini/app-registry-summary/main.squid`, verifies
the host app registry contains the installed app, launches it, and verifies the
app can inspect the same installed-app registry through `app.registry()` and
`app.registry.get(...)`.

The Zephyr app state check is `scripts/c3-supermini-test-app-state.sh`. It
installs `tests/hardware/c3-supermini/state-counter/main.squid`, launches it,
sends `SELECT` key events, verifies explicit `state.load()` / `state.save()`
debug output and non-empty `device state` bytes, resets the runtime without
formatting storage, relaunches the app, and verifies `state.load()` restores
the saved count.

`scripts/c3-supermini-measure-stack-usage.sh` runs after the stateful app and
app lifecycle checks in the full ESP32-C3 Super Mini suite. It records
`device resources` output under `target/hardware-tests/stack-usage/` and
verifies `protocol_thread_stack_*` and `vm_worker_stack_*` metrics are
internally consistent. The current firmware keeps the protocol/main stack budget
at 8 KiB and the VM worker stack budget at 24 KiB while this measurement data is
used to decide whether later reductions are safe.

`scripts/c3-supermini-test-system-resources.sh` runs after lifecycle coverage
and before stack measurement. It installs
`tests/hardware/c3-supermini/system-resources`, launches it, and verifies that
`system.memory()` returns a Zephyr RAM/heap diagnostic string and
`system.storage("apps")` returns an app-storage string through the real VM FFI
host callbacks.

`scripts/c3-supermini-test-device-config.sh` runs after system resource coverage
and before stack measurement. It installs
`tests/hardware/c3-supermini/device-config-summary`, launches it, and verifies
that `device.config.load`, `device.config.set`, `device.config.rebind`, and
`device.config.save` all return result records through the real Zephyr VM FFI
host. The app is installed as a package with a `.sqdevice` resource; the
current reference firmware returns `ok=true` for package resource load and
draft set, `ok=true` for `indicator.default` rebind, then `ok=true` for SQDC
flash save. Native Zephyr ztests additionally verify that the saved SQDC is
loaded on later app starts before app-local `device {}` bindings.

`scripts/c3-supermini-test-content-pick.sh` runs after device config coverage
and before stack measurement. It installs
`tests/hardware/c3-supermini/content-pick-summary`, launches it, and verifies
that `content.pickFile(".binbook")`, `content.readText("notes.txt")`, and
`content.readLines("notes.txt", 4)` flow through compiler lowering, SQBC, Rust
VM hosting, FFI, and Zephyr runtime callbacks as current unsupported result
records. The Zephyr reference firmware returns `ok=false`,
`error="unsupported"`, `path=null`, `text=null`, and an empty `lines` list until
real external content picking and reads are implemented.

`scripts/c3-supermini-test-device-binding.sh` runs after system resource
coverage and before the explicit device config API check. It packages
`tests/hardware/c3-supermini/device-binding-summary`, installs it with its
`.sqdevice` resource, launches it, and verifies that a top-level
`device { indicator { use ... } }` binding can be applied before
`event.on("app.start")` without explicit app-side `device.config.*` calls.

`scripts/c3-supermini-test-inline-gpio-binding.sh` runs after packaged device
binding coverage and before the explicit device config API check. It installs
`tests/hardware/c3-supermini/inline-gpio-binding-summary`, launches it, and
verifies that a top-level `device { indicator { use "gpio:GPIO8" } }` binding
can be normalized and applied before `event.on("app.start")` without a package
`.sqdevice` resource. `GPIO8` is the ESP32-C3 Super Mini fixture value and is
accepted because the generated Zephyr target header marks it GPIO-capable; use
target metadata or a `.sqdevice` resource for other boards and polarity needs.

`scripts/c3-supermini-test-inline-gpio10-binding.sh` runs after the GPIO8
inline binding check and before the reserved-pin rejection check. It installs
`tests/hardware/c3-supermini/inline-gpio10-binding-summary`, launches it, and
verifies that the Super Mini target metadata accepts
`device { indicator { use "gpio:GPIO10" } }` through the same Zephyr binding
path. The check proves target validation and app-start behavior only; it does
not claim a visible indicator is connected to GPIO10.

`scripts/c3-supermini-test-unsupported-inline-gpio-binding.sh` runs after the
supported inline binding check and before the explicit device config API check.
It installs `tests/hardware/c3-supermini/unsupported-inline-gpio-binding`,
whose top-level `device { indicator { use "gpio:GPIO18" } }` binding is
syntactically valid but reserved for native USB in the ESP32-C3 Super Mini
target metadata. `app launch` must fail with `unsupported (-95)`, `device output`
remains empty, and `device errors` remains empty, proving target validation
rejects the binding before VM start while the protocol remains responsive.

`scripts/c3-supermini-test-blink.sh` is an explicit visible indicator parity
check. It installs `examples/blink-supermini`, launches it, verifies
`output=blink ready`, checks that `device errors` is empty, and leaves the
non-blocking `service.indicator.blink(120, 80)` pattern running for physical LED
confirmation. It is not part of the full hardware suite because the suite keeps
the blinky app as the final visible board-state check.

`scripts/c3-supermini-test-display-drawlog.sh` runs after lifecycle coverage
and before system resource coverage. It installs
`tests/hardware/c3-supermini/display-drawlog`, launches it, and verifies that
the Zephyr VM host records `service.display.select`, `service.display.image`,
and `service.display.draw` in the same headless `device drawlog` surface as
the older clear/text/rect/line display commands.

`scripts/c3-supermini-test-wifi-state.sh` runs before Wi-Fi scan coverage. It
installs `tests/hardware/c3-supermini/wifi-status-summary` and launches a
summary-only SquidScript app that calls `service.wifi.status()`. The app prints
only `state`, `backend`, `driverStarted`, and `error`; the script rejects raw
BSSID, MAC, or local IP patterns in captured output. In the default full
hardware suite it runs with `--require-real-wifi`, which rejects the unsupported
fallback and requires the real Zephyr Wi-Fi backend to be active.

`scripts/c3-supermini-test-wifi-scan-api.sh` runs after Wi-Fi status coverage
and before Wi-Fi list coverage. It installs
`tests/hardware/c3-supermini/wifi-scan-summary` and launches a summary-only
SquidScript app that calls `service.wifi.scan()` without credentials. The app
prints only `ok`, `error`, and `count`; the script rejects raw BSSID, MAC, or
local IP patterns in captured output. In the default full hardware suite it runs
with `--require-real-wifi`, which rejects the unsupported fallback and requires
a successful real Zephyr Wi-Fi scan.

`scripts/c3-supermini-test-wifi-list-api.sh` runs after Wi-Fi scan coverage and
before Wi-Fi AP coverage. It installs
`tests/hardware/c3-supermini/wifi-list-summary` and launches a SquidScript app
that iterates `service.wifi.scan().networks`. The app prints only redacted
per-network structure: SSID length, channel, RSSI, auth, and hidden flag. It
does not print SSIDs or BSSIDs. The script rejects raw BSSID, MAC, or local IP
patterns in captured output, and `--require-real-wifi` requires at least one
redacted `wifi ap` record from the real Zephyr Wi-Fi backend.

`scripts/c3-supermini-test-wifi-station-api.sh` is explicit-credentials-only
and is not part of the default full hardware suite. It skips successfully unless
`SQUID_WIFI_STATION_SSID` and `SQUID_WIFI_STATION_PASSWORD` are set. When those
variables are present, it provisions profile `dev` through
`squidc device wifi-profile --ssid-env SQUID_WIFI_STATION_SSID --password-env
SQUID_WIFI_STATION_PASSWORD`, installs
`tests/hardware/c3-supermini/wifi-station-summary`, and launches a summary-only
app that calls `service.wifi.connect("dev")` and `service.wifi.status()`. The
script requires `connect.ok == true` and `status.connected == true`, prints
command names and lengths only, and rejects raw SSIDs, passwords, BSSIDs, MACs,
or local IP patterns in captured output.

`scripts/c3-supermini-test-wifi-ap-api.sh` runs after Wi-Fi list coverage and
before the final visible LED check in the default full hardware suite. It
installs `tests/hardware/c3-supermini/wifi-ap-summary`, launches a summary-only
app that calls `service.wifi.startAP("SquidScript")` and
`service.wifi.getAPIP()`, sends `SELECT` to call `service.wifi.stopAP()`, and
requires start, AP IP lookup, and stop to report success without printing the
raw AP SSID, BSSIDs, MACs, or local IP patterns in captured output. AP start
also starts a bounded DHCPv4 server on the AP interface. This script does not
prove that an external client associated with the AP or received a lease.

For the current ESP32-C3 Super Mini Zephyr target,
`scripts/c3-supermini-test-blinky.sh` is the final full-suite check. It
installs and launches `examples/blinky-supermini/main.squid`. Serial
`device output` should show repeated `blink false` / `blink true` lines and
`device errors` should be empty before the final visible check. After that
final check starts, do not run another serial command unless debugging the
final board state.

The indicator breathe check installs and launches
`examples/breathe-supermini/main.squid`. Serial `device output` should include
`breathe ready`, `breathe peak marker`, and `breathe resume`, `device errors`
should be empty, and the final visible board state should be a smooth repeating
onboard LED breathe pattern. The example briefly double-blinks near every third
peak, then resumes the smooth breathe cycle so the visible check can confirm
both the marker and the PWM smoothness. For the current ESP32-C3 Super Mini
target, that visible check exercises the logical `indicator0` device, which
the Zephyr overlay maps to the common-clone GPIO8 onboard LED through ESP32-C3
LEDC PWM.
