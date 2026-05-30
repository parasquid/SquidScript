# Hardware Target Tests

Hardware target tests exercise a connected physical board. They are not unit
tests. Never run hardware commands in parallel against the same serial device:
concurrent flash, monitor, REPL, hardware-test, or `squidc device` commands can
interleave serial bytes, reset the board, or leave hardware in a misleading
state.

Hardware target tests and serial/flashing commands must run outside the Codex
sandbox. Sandboxed sessions do not reliably expose `/dev/ttyACM*`,
`/dev/ttyUSB*`, or `/dev/serial/by-id`.

Scripts that invoke `squidc device ...` or `squidc app ...` source
`scripts/lib/hardware-command.sh` so each hardware-owning command is captured
under `target/hardware-tests/` or `target/hardware-benchmarks/` with a
command-level timeout. If a protocol command stalls, the script should fail with
the captured command output instead of hanging the full suite.

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
- Verify `service.display.info()` / `display.info()` returns the active display
  service descriptor without using `hardware.*` display APIs.
- Verify persistent app storage and app state through Zephyr storage.
- Verify `system.memory()` and `system.storage("apps")` through the Zephyr VM
  FFI host.
- Verify app-facing installed-app inspection through `app.registry()` and
  `app.registry.get(...)`.
- Verify app-facing indicator state reads and toggles through
  `service.indicator.read()` and `service.indicator.toggle()`.
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
increasing service scope. The Zephyr system heap is sized at 36864 bytes from
live `device resources` heap high-water data after representative app, display,
device binding, file, Wi-Fi status, scan, list, and AP workloads; remeasure
it before adding larger radio or networking workloads.

`targets/esp32c3-super-mini.target.json` should describe these verified Zephyr
runtime services and defaults, not only the ESP32-C3 silicon radio capability.
The Zephyr build generates SquidScript-facing defaults such as
`indicator.default` from `SQUID_ZEPHYR_TARGET_JSON` and validates those
defaults against `SQUID_ZEPHYR_TARGET_OVERLAY`, while devicetree remains
responsible for board driver nodes. Wi-Fi status, scan, redacted network
listing, AP start/stop/IP lookup, and volatile-profile station
connect/disconnect are exposed through the canonical firmware. Station connect
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
`app.armedStack.get(...)`. The reader fixture prints armed-stack diagnostics
only on the first timer tick that sees the armed app, so the bounded retained
output history keeps the root launch, reader launch, and armed-selection proof
lines together. The lifecycle fixtures use volatile
in-memory counters and intentionally avoid `state.load()` / `state.save()` so
the check distinguishes ordinary foreground event dispatch from fresh VM
sessions on launch, armed trigger activation, and app-exit return.
See `docs/app_lifecycle_state_machine.md` for the lifecycle state model and
the `device reset` versus `device storage-format` isolation rule used by
stateful hardware harnesses.

The Zephyr app registry API check is
`scripts/c3-supermini-test-app-registry-api.sh`. It formats app storage,
installs `tests/hardware/c3-supermini/app-registry-summary/main.squid`, verifies
the host app registry contains the installed app, launches it, and verifies the
app can inspect the same installed-app registry through `app.registry()` and
`app.registry.get(...)`. The check is currently standalone only and is excluded
from `scripts/c3-supermini-test-hardware.sh` until the clean-firmware baseline
where `app install` reports success but host `app list` is empty is diagnosed.
The ESP32-C3 Zephyr reference firmware keeps eight installed-app registry
entries resident in RAM; this check exercises registry visibility, not
full-capacity filling.

The Zephyr app state check is `scripts/c3-supermini-test-app-state.sh`. It
installs `tests/hardware/c3-supermini/state-counter/main.squid`, launches it,
sends `SELECT` key events, verifies explicit `state.load()` / `state.save()`
debug output and non-empty `device state` bytes, resets the runtime without
formatting storage, relaunches the app, and verifies `state.load()` restores
the saved count.

The Zephyr GPIO input button check is
`scripts/c3-supermini-test-input-button.sh`. It installs
`tests/hardware/c3-supermini/input-button-summary/main.squid`, whose top-level
`device { input { use "gpio-button:GPIO9:key.SELECT:activeLow" } }` binding maps
the ESP32-C3 Super Mini BOOT/GPIO9 button to `key.SELECT`. The script verifies
launch output, waits for a physical BOOT/GPIO9 press to increment app state, and
the app starts a visible indicator blink when the press is handled.

`scripts/c3-supermini-probe-gpio9-raw.sh` is a targeted GPIO9 electrical
diagnostic, not part of the full suite. It installs
`tests/hardware/c3-supermini/gpio9-raw-probe/main.squid`, which applies the same
GPIO9 input binding to configure the pull-up and then prints
`hardware.gpio.read("GPIO9")` at `app.start`. The released BOOT/GPIO9 state
should print `output=gpio9 true`; the held BOOT/GPIO9 state should print
`output=gpio9 false`. The script repeatedly relaunches the tiny probe while
waiting for the held state, so a delayed human press gets re-sampled instead of
being hidden by the first launch result. If the tiny BOOT button cannot be held
reliably, short the GPIO9 header pin to GND during the held phase to force the
same active-low electrical condition. Use it to separate raw pin visibility from
input event dispatch when the input stack isolation harness times out before
`after-press-observed`.

`scripts/c3-supermini-probe-boot-button-pins.sh` is a broader physical BOOT
button diagnostic, also outside the full suite. It installs
`tests/hardware/c3-supermini/boot-button-pin-scan/main.squid`, captures a
released baseline for GPIO0 through GPIO10, then repeatedly re-launches the
probe while the BOOT button is held and waits for any sampled GPIO value to
change. It writes timeout diagnostics under
`target/hardware-tests/boot-button-pin-scan/` and captures `device errors` as
`errors-after-timeout` if no pin changes.

`scripts/c3-supermini-measure-stack-usage.sh` runs after the stateful app and
app lifecycle checks in the full ESP32-C3 Super Mini suite. It records
`device resources` output under `target/hardware-tests/stack-usage/` and
verifies `proto_stack_*` and `vm_stack_*` metrics are
internally consistent. The current firmware keeps the protocol/main stack budget
at 8,192 bytes and the VM worker stack budget at 22,016 bytes. The protocol
stack was previously reduced to 3,264 bytes from measured GPIO9 input rows, but
installed app lifecycle coverage later exposed a protocol/main-side fatal crash
while registering armed trigger metadata and launching installed foreground
apps. Treat the 8,192-byte budget as a reliability baseline until the trigger
metadata and launch paths are re-attributed and reduced with hardware evidence.
The harness uses a command-level timeout for its
`device resources`
request so serial stalls fail with captured output instead of hanging the full
suite. GPIO-button device-binding launch coverage previously measured
protocol/main stack use above the old 8 KiB budget before launch-time binding
setup moved to the VM worker stack. Current targeted clean-boot and skip-flash
GPIO9 input rows measure protocol/main stack use flat at 2476 bytes across
format, install, launch, release, and timeout rows. Do not lower the configured
main stack budget again until lifecycle, registry, GPIO input, and stack
resource checks all pass in the same firmware build.
Use `scripts/c3-supermini-measure-input-stack-isolation.sh` when the input path
needs clean high-water attribution. It builds/flashes by default, verifies the
diagnostic boot banner, then records `after-boot`, `after-format`,
`after-install`, `after-launch`, `after-release`, `after-press-observed`, and
`after-press` resource rows under
`target/hardware-tests/input-stack-isolation/summary.tsv`. Pass `--skip-flash`
only when preserving the current firmware session is more important than a
fresh high-water baseline. Override `INPUT_BUTTON_APP`, `INPUT_BUTTON_APP_ID`,
and `INPUT_BUTTON_LABEL` to run the same attribution flow against a candidate
binding such as `tests/hardware/c3-supermini/input-button-gpio5-summary`.
For the default ESP32-C3 Super Mini GPIO9 active-low path, short GPIO9 to GND
during the held phase if the tiny BOOT button cannot be held reliably.
The current clean-boot GPIO9 input isolation launch row measured protocol/main
stack flat at 2476 bytes and VM worker stack use at 17056 bytes, with 2400
bytes free and `input_button_state=1`. The low byte confirms one physical GPIO9
binding was installed; the next byte reports zero currently pressed inputs
after the BOOT/GPIO9 pull-up is configured through devicetree. The script waits
for release before asking for a held press and writes `after-release-timeout`
diagnostics if the line never reads released. If the held press is not observed
electrically, it writes `after-press-timeout` diagnostics; if the press is
observed but `output=count 1` is not produced, it writes
`after-dispatch-timeout` diagnostics. A GPIO5 active-high candidate run also
timed out before `after-press-observed`, with `input_button_state=1`,
protocol/main stack flat at 2476 bytes, and VM worker stack use at 17136 bytes.
The completed physical GPIO9 input isolation run observed
`after-press-observed` with `input_button_state=257`, proving one configured
input and one currently pressed input, and `device output` changed from
`output=count 0` to `output=count 1`. The current skip-flash press row kept
protocol/main stack flat at 2476 bytes and VM worker stack use at 17136 bytes,
with 2320 bytes free.

For the ESP32-C3 Super Mini reference board, treat GPIO9 as the confirmed BOOT
button input path. Board pinout references identify GPIO9 as the BOOT button,
and local hardware evidence confirms GPIO9 reads released as `true`, held as
`false`, and dispatches the configured `key.SELECT` input event. The
BOOT-button pin scan is diagnostic only: it samples GPIO0 through GPIO10 and
now requires repeated stable changed samples, but floating or unconfigured pins
can still move with a pen-held tiny button. ESP32-C3 GPIOs can be configured
with weak internal pull-up or pull-down bias in software, so future diagnostic
scans can reduce floating-pin noise by applying a known pull before sampling
candidate pins. Keep the confirmed Super Mini BOOT path active-low with pull-up
bias on GPIO9; do not switch the BOOT binding to pull-down. Do not treat GPIO3,
GPIO4, GPIO7, GPIO10, or GPIO5 scan changes as ESP32-C3 Super Mini buttons
without a targeted raw probe and input-stack run. GPIO3 is also part of
ESP32-C3 boot strapping, while GPIO4 and GPIO7 have alternate JTAG/FSPI-related
functions, so broad unconfigured scans are not authoritative for button mapping.
After flattening the resumable screen-render interpreter path, a headless
draw-log isolation run showed that `screen.open(...)` into a screen with only
`service.display.clear("gray0")` uses `vm_stack_used_bytes=17056` of
the prior 24576-byte budget, down from the previous 24020-byte display-only
spike. After moving
function calls onto the same VM-owned continuation stack, full suite coverage
measured `vm_stack_used_bytes=17620` before lowering the worker stack to
20480. Targeted GPIO9 input summary coverage after app-start binding setup moved
to the VM worker measured `vm_stack_used_bytes=17296`, and narrowing the
FFI app process/armed stack scratch reduced the input launch row to 17056. The
worker stack is now 22,016 bytes. This replaces the previous 18,016-byte budget
after installed-app lifecycle launch testing exposed a fatal crash consistent
with worker-stack exhaustion. Remeasure the physical press row and full
hardware suite before lowering that budget again. The stack harness fails
with the captured resource frame when protocol/main unused stack drops below
768 bytes or VM worker unused stack drops below 384 bytes.

`scripts/c3-supermini-measure-ram-workloads.sh` is the targeted RAM and stack
attribution harness. It formats app storage, installs the GPIO9 input summary
app, launches it, dispatches a serial `SELECT`, and records `device resources`
snapshots after format, install, launch, and dispatch under
`target/hardware-tests/ram-workloads/summary.tsv`. The input app launch and
`SELECT` dispatch stay in one foreground session, while later independent
display, system-resource, and Wi-Fi workload groups reset runtime lifecycle
state before launch. This keeps host `app launch` lifecycle semantics intact
without filling the bounded foreground return stack during unrelated RAM
measurements. Use it before reducing stack budgets so changes can be tied to a
specific workload boundary. The resources response also reports
`proto_stack_pre_*` so the harness can
distinguish stack already consumed before resource-response encoding from stack
pressure caused by the diagnostic command itself. Stack values are Zephyr
high-water readings for the current boot, so unchanged stack values across rows
mean the peak happened before or during the earliest matching snapshot, not that
every workload used the same stack depth. It is separate from the full hardware
suite because it intentionally resets app storage. A representative GPIO9 input
summary run after moving app-start binding setup to the VM worker stack measured
the protocol/main stack flat at 2476 bytes, while the VM worker stack rose from
264 bytes to 17296 bytes during launch and kept 2160 bytes free; the serial
`SELECT` dispatch did not increase those high-water marks.

`scripts/c3-supermini-test-system-resources.sh` runs after lifecycle coverage
and before stack measurement. It installs
`tests/hardware/c3-supermini/system-resources`, launches it, and verifies that
`system.memory()` returns a Zephyr RAM/heap diagnostic string and
`system.storage("apps")` returns an app-storage string through the real VM FFI
host callbacks.

`scripts/c3-supermini-test-planned-sleep.sh` installs
`tests/hardware/c3-supermini/planned-sleep`, launches it, verifies
`system.startReason() == "launch"`, dispatches `key.SELECT` to request
`service.power.sleep({ wakeAfterMs: 1500 })`, waits for ESP32-C3 timer wake,
and verifies that the restored foreground app starts with
`system.startReason() == "wake"` and app state restored from `state.save()`.
This test owns the serial target across a USB reset/re-enumeration window, so
run it as a standalone hardware check rather than in parallel with monitors or
other hardware scripts.

`scripts/c3-supermini-test-indicator-state.sh` runs after system resource and
app registry coverage and before binding-specific indicator checks. It installs
`tests/hardware/c3-supermini/indicator-state-summary`, launches it, and
verifies serial output from `service.indicator.write(false)`,
`service.indicator.read()`, and `service.indicator.toggle()` so indicator state
semantics are proven without relying on visible LED observation.

`scripts/c3-supermini-test-device-config.sh` runs after system resource coverage
and before stack measurement. It installs
`tests/hardware/c3-supermini/device-config-summary`, launches it, and verifies
that `device.config.load`, `device.config.set`, `device.config.rebind`, and
`device.config.save` all return result records through the real Zephyr VM FFI
host. The app is installed as a package with a `.sqdevice` resource; the
current canonical firmware returns `ok=true` for package resource load and
draft set, `ok=true` for `indicator.default` rebind, then `ok=true` for SQDC
flash save. Native Zephyr ztests additionally verify that the saved SQDC is
loaded on later app starts before app-local `device {}` bindings.

`scripts/c3-supermini-test-file-pick.sh` runs after device config coverage
and before stack measurement. It installs
`tests/hardware/c3-supermini/file-pick-summary`, launches it, and verifies
that `file.pickFile(".binbook")`, `file.readText("notes.txt")`, and
`file.readLines("notes.txt", 4)` flow through compiler lowering, SQBC, Rust
VM hosting, FFI, and Zephyr runtime callbacks as current unsupported result
records. The Zephyr canonical firmware returns `ok=false`,
`error="unsupported"`, `path=null`, `text=null`, and an empty `lines` list until
real external file picking and reads are implemented.

`scripts/c3-supermini-benchmark-lazy-load-screen.sh` is a benchmark runner, not
a required pass/fail hardware suite check. It installs
`tests/hardware/c3-supermini/lazy-load-screen-benchmark` by default or
`tests/hardware/c3-supermini/lazy-load-screen-worst-case` with `MODE=worst`,
launches a timer-driven 10-screen app from LittleFS, waits for firmware
dispatch sequence increments, and reports the portable fields defined in
`docs/hardware_benchmarks.md`. The timing values come from firmware
`device resources` metrics for the most recent VM dispatch, so host serial
latency and physical display refresh are outside the measured window.

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

`scripts/c3-supermini-test-blink.sh` is an explicit visible indicator check. It
installs `examples/blink-supermini`, launches it, verifies
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
