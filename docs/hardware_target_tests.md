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
the captured command output instead of hanging the full suite. If the protocol
diagnostic captures are empty, the helper also records bounded raw serial
diagnostics so a failure can distinguish a live firmware log stream from a
silent USB/protocol path.

## Current Targets

The default real firmware target is Zephyr-backed XIAO ESP32-C3 directly wired
to the Good Display DESPI-C02 connector board and GDEQ0426T82/SSD1677 panel.
The ESP32-C3 Super Mini remains a supported regression hardware target.

Default XIAO build and flash:

```sh
cargo run -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd
SQUID_ZEPHYR_TARGET_JSON=targets/xiao-esp32c3-gdeq0426t82-sd.target.json \
  ./scripts/zephyr-ram-audit.sh build/zephyr/xiao-esp32c3-gdeq0426t82-sd/zephyr/zephyr.elf
cargo run -p squidc -- target flash --target xiao-esp32c3-gdeq0426t82-sd
```

XIAO monitor:

```sh
cargo run -p squidc -- target monitor --target xiao-esp32c3-gdeq0426t82-sd
```

Super Mini regression build and flash:

```sh
cargo run -p squidc -- target build --target esp32c3-super-mini
SQUID_ZEPHYR_TARGET_JSON=targets/esp32c3-super-mini.target.json \
  ./scripts/zephyr-ram-audit.sh
cargo run -p squidc -- target flash --target esp32c3-super-mini
```

Super Mini monitor:

```sh
cargo run -p squidc -- target monitor --target esp32c3-super-mini
```

Pass `--target` explicitly in scripts and CI. `squidc target` resolves the
Zephyr board, overlay, fallback app, generated Kconfig path, and build
directory from target JSON.

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
- Verify ESP32-C3 BLE advertising when the target JSON declares
  `service.ble.file-transfer`.

For the XIAO e-paper target, missing e-paper or external SD hardware must not
block boot, fallback app launch, serial, or host protocol commands. Boot logs
should surface target-device availability failures, and retained diagnostics
should be queryable later with `device errors`. In the first XIAO slice, the
external SD reader is target metadata only: SD `MISO` and `CS` jumper wiring,
mounting, app storage, and content volume behavior remain unverified and are
not runtime-advertised features.

`scripts/xiao-esp32c3-test-epaper-hello.sh` is the XIAO physical-display smoke
test. It builds and flashes the diagnostic-only Zephyr app under
`tests/hardware/xiao-esp32c3/epaper-hello`, which bypasses SquidScript and the
product firmware runtime, drives the SSD1677/GDEQ0426T82 panel directly, and
prints `EPAPER_HELLO_READY` after the refresh command completes. The serial
marker proves that the app reached the refresh path and is the unattended
smoke-test pass criterion. The app leaves the image on the e-paper display.
This write-only display path does not provide pixel readback; without a camera
or another optical sensor, serial evidence can prove controller activity but
cannot prove the final visible pixels. Visual confirmation is optional for
smoke runs and required only when the task explicitly asks to verify rendered
pixels or a human is present to inspect the panel.

`scripts/xiao-esp32c3-test-epaper-gray2-smoke.sh` is the XIAO e-paper GRAY2 smoke
test for the product firmware display path. It packages, installs, and launches
`tests/hardware/xiao-esp32c3/epaper-gray2-smoke`, whose BinBook fixture
exercises `service.display.draw` with packed 2bpp content. The
unattended pass criteria are serial: `device output` contains `gray2 pages 1`,
`device drawlog` contains `draw=binbook`, `device errors` is empty, and
`device resources` responds after the refresh. The expected visible image is
native 800 x 480 panel content with black, dark gray, light gray, white bands.
The script can capture a USB webcam frame as optional evidence; pass
`--require-camera` only when the camera itself is part of the check.

`scripts/xiao-esp32c3-test-epaper-fast-redraw-smoke.sh` is the retained
XIAO e-paper fast redraw smoke test for repeated product-firmware BinBook redraws.
It packages, installs, and launches
`tests/hardware/xiao-esp32c3/epaper-fast-redraw-smoke`, then injects
`key.RIGHT` events to cycle the three-page GRAY2 fixture through
`service.display.draw`. The unattended pass criteria are serial: `device output`
contains `fast redraw page 1`, `fast redraw page 2`, and `fast redraw page 0`,
`device drawlog` contains `draw=binbook`, `device errors` is empty, and
`device resources` responds after the repeated redraws. The expected visible
sequence is gray bands -> chimp/image -> sharp geometry. The visual acceptance
question is no full flash-style refresh between page changes. The SSD1677
backend renders the first and cadence cleanup refreshes with true GRAY2. Fast
intermediate page turns use a full-window ordered-dither black/white
differential partial pass: firmware re-decodes the remembered previous BinBook
page and streams its complete 1bpp dithered plane, including white and black
pixels, to RED/previous RAM (`0x26`), then streams the requested page's complete
1bpp dithered plane to BW/current RAM (`0x24`) before activation. This gives the
SSD1677 both old and new pixels for black-to-white and white-to-black
transitions without keeping a full-screen framebuffer, while preserving some
GRAY2 edge texture on fast turns. A render may call
`service.display.refreshMode("fast1bpp")` to force the same differential partial
path with flat 1bpp thresholding, or `service.display.refreshMode("full")` to
force the GRAY2 full refresh path for that render. Human or camera evidence is
needed for the optical judgment; the serial checks prove app/runtime/display-path
activity and report the selected firmware refresh mode, not final visible
quality.

`scripts/xteink-x4-test-http-binbook-upload.sh` is the XTEINK X4 HTTP BinBook
upload hardware check. It flashes the X4 firmware, installs
`tests/hardware/xteink-x4/http-binbook-upload`, launches a SquidScript app that
starts the `SquidScript-X4` AP and registers
`service.http.start("file-upload", ...)`, associates the host Wi-Fi to the
device AP, and uploads a real `.binbook` with `curl -T` to
`http://192.168.4.1/upload/<name>.binbook`. The wrapper prints curl progress,
records curl byte/speed totals, and resumes interrupted transfers by querying
`HEAD /upload/<name>.binbook` for the device-reported upload offset before the
next `PUT`. Set `INTERRUPT_UPLOAD=1` with a large fixture to run the recovery
variant: the script starts a rate-limited upload, interrupts it, requires the
HTTP route to keep answering `HEAD` with a non-zero partial offset, then resumes
and completes the upload. The unattended pass criteria are serial plus HTTP:
curl receives `ok`, `device output` contains `http upload complete`,
`upload copy true null`, and the uploaded book name, `device drawlog` contains
`draw=binbook`, and `device errors` is empty. This proves the app-owned HTTP
route, firmware staging file, resumable SD-backed upload, `file.copy`,
`content.binbook.list("books")`, and BinBook display path work together on the
device-owned X4 SD card.

`scripts/xteink-x4-test-binbook-reader.sh` is the XTEINK BinBook reader
selection and interrupted-resume hardware check. It flashes the XTEINK firmware
unless `--skip-flash` is passed, formats app/content storage for an isolated
test run, installs two `.binbook` files into the `books` library, installs
`examples/binbook-reader`, drives the library, reader, menu, and relaunch flows
with serial `device key` events, and verifies `device drawlog` contains
`draw=binbook` with `mode=full` for the reader page, `mode=fast1bpp` for
selection screens, `device errors` is empty, and resource metrics are
available. This proves the promoted reader app can select uploaded
content-library books and resumes only when the saved foreground view was the
reader.

`scripts/xteink-x4-test-transfer-regression.sh` is the XTEINK X4 transfer regression
suite for upload speed and data-integrity work. It runs the serial, HTTP, and
BLE transfer checks sequentially against the same physical target. Each check
uses a validator-compatible generated BinBook payload larger than the small
firmware scratch buffers, stores it under a transport-specific safe file name,
and verifies the final device-owned file with `device content-check <name>
--size <bytes> --crc32 <hex>`. The direct scripts are
`scripts/xteink-x4-test-serial-transfer.sh`,
`scripts/xteink-x4-test-http-transfer.sh`, and
`scripts/xteink-x4-test-ble-transfer.sh`; the HTTP and BLE checks install
`tests/hardware/xteink-x4/file-transfer-regression`, which copies completed
uploads into the `books` library before verification. Use `--payload
<file.binbook>` to test a specific existing book, `--host-wifi-iface <iface>`
for the HTTP AP association, and `--device <name-or-address>` for BLE matching.
The scripts write command output, curl progress, and failure diagnostics under
`target/hardware-tests/xteink-x4-transfer-*`.

The default XIAO firmware also drives the SSD1677/GDEQ0426T82 display through
the Zephyr SPI backend for `service.display.clear` and `service.display.text`.
The target JSON is the source of truth for the default portrait logical
orientation: physical `800 x 480`, logical `480 x 800`, rotation `270`. The
backend translates logical display ops into physical panel coordinates. On
boot, fallback `main` renders the target label, `system.memory()`,
`system.storage("apps")`, and BLE installer status. The unattended firmware
display proof is: the serial monitor logs `display refresh complete
busy_observed=1`, `device drawlog` contains fallback text commands, and
`device errors` is empty. The drawlog is bounded, so early commands such as
`clear` can roll out after the full fallback screen renders. `rect`, `line`,
`image`, `draw`, partial refresh, and grayscale dithering are not
physical-display features in this slice.

Keep the suite ordered so stateful reset/install tests run before Wi-Fi and
late physical-input checks, and keep the final visible board-state check last.
Pass `--skip-physical-input` to the full suite for unattended runs where the
BOOT/GPIO9 press cannot be confirmed; that run does not validate physical input
dispatch.

`scripts/c3-supermini-test-hardware-non-scan.sh` is the RAM-confidence subset
for same-build stack and heap validation when Wi-Fi scan/list is verified
separately. It runs the same ordered stateful checks but excludes Wi-Fi
scan/list, so it does not validate Wi-Fi scan/list. It still runs Wi-Fi status,
Wi-Fi AP, stack usage, and the final visible blinky check. Pass
`--skip-physical-input` only for unattended runs where the BOOT/GPIO9 press
cannot be confirmed; that run does not validate physical input dispatch.

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
uses bounded 6/6 native-network packet pools and 16/16 network buffer pools
sized for current low-throughput control-plane Wi-Fi behavior and measured
socket-service,
network-management event, ESP timer task, and network RX stack budgets; TCP,
HTTP, AP client throughput, or other bulk traffic must be remeasured before
increasing service scope. The Zephyr system heap is sized at 65536 bytes for
the current reference workload, including ESP32-C3 Wi-Fi and BLE advertising
support selected from target metadata. Keep
`CONFIG_HEAP_MEM_POOL_IGNORE_MIN` enabled because reset-bounded workload
telemetry, not the ESP Wi-Fi minimum alone, is the sizing authority for this
target. Remeasure it with live `device resources` data after representative
app, display, device binding, file, Wi-Fi status, scan, AP, BLE advertising,
and future BLE transfer workloads before changing the heap budget again.
`device resources`
also reports `heap_largest_free_supported` and `heap_largest_free_bytes`.
Current ESP32-C3 Zephyr builds set the supported flag to `0` because the
public Zephyr heap stats API exposes free/allocated/high-water bytes but no
safe non-mutating largest-free-block query.
The deferred logger process stack stays at Zephyr's 768-byte default for this
target class. Do not reduce it below that without rerunning Wi-Fi scan while
capturing raw serial output, because scan-time driver and filesystem logs share
the USB serial path used by protocol diagnostics.

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
`app.registry.get(...)`. The check runs in
`scripts/c3-supermini-test-hardware.sh` after app lifecycle coverage and before
the stack measurement checkpoint. Its fixture keeps selected registry fields on
separate debug output lines so assertions stay within the bounded firmware
output slot.
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
The full hardware suite runs this physical prompt near the end, immediately
before the final blinky check, so unattended or missed-button failures do not
hide earlier app, storage, lifecycle, display, resource, GPIO, and Wi-Fi
coverage. Use `scripts/c3-supermini-test-hardware.sh --skip-physical-input`
when no one is present to press BOOT/GPIO9.
For RAM and stack-budget validation, host-injected `device key SELECT` events
exercise the logical input dispatch and app handler path after an input event is
queued. Physical GPIO9 checks are still required when validating the electrical
pin, pull configuration, debounce, and binding path that turns the BOOT/GPIO9
press into that logical event.

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
at 4,864 bytes and the VM worker stack budget at 24,576 bytes. Treat the current
budgets as the reliability baseline until lifecycle, registry, GPIO input, and
stack resource checks pass with fresh hardware evidence for a smaller setting.
The harness uses a command-level timeout for its
`device resources`
request so serial stalls fail with captured output instead of hanging the full
suite. The repeatable non-scan wrapper is
`scripts/c3-supermini-test-hardware-non-scan.sh`; a run with
`--skip-physical-input` is useful for unattended same-build coverage but does
not replace the GPIO9 physical press row. Do not lower the configured main stack
budget again until lifecycle, registry, GPIO input, and stack resource checks
all pass in the same firmware build.
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
For the ESP32-C3 Super Mini regression GPIO9 active-low path, short GPIO9 to
GND during the held phase if the tiny BOOT button cannot be held reliably.
Current same-build hardware coverage reached the BOOT/GPIO9 prompt in
`scripts/c3-supermini-test-input-button.sh` but timed out with `output=count 0`,
so it proves the app launched and the line was not pressed during that run; it
does not prove physical dispatch. Current GPIO9 input isolation coverage with
`--skip-flash` refreshed the resource summary through `after-press-timeout`.
Re-run input isolation after stack-budget changes when the exact free-byte
margin on the physical press path matters. The low byte of
`input_button_state` confirms installed physical GPIO bindings; the next byte
reports currently pressed inputs after the BOOT/GPIO9 pull-up is configured
through devicetree. The script waits
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

### Lockup Triage

Hardware scripts that use `scripts/lib/hardware-command.sh` capture
best-effort diagnostics on command failure or timeout:

- `<label>-failure-resources.out`
- `<label>-failure-errors.out`
- `<label>-failure-lifecycle.out`

Polling loops that wait for app output use the same helper with a
`<label>-timeout-*` prefix. The diagnostics are bounded by
`COMMAND_TIMEOUT_SECONDS` and are captured sequentially on the same serial
target.

When flashing succeeds but serial commands stall, app launch hangs, or input
dispatch stops responding, inspect `device resources` first. Compare
`proto_stack_*` and `vm_stack_*` used/unused values, then inspect
`runtime_status`, `runtime_dispatch_started`, `runtime_dispatch_age_us`,
`runtime_work_submitted`, `runtime_current_app_present`,
`runtime_lifecycle_phase`, and `runtime_arm_phase`. If serial remains
responsive and `runtime_dispatch_age_us` keeps growing, treat the VM worker or
the active service path as the first suspect. If stack unused values are close
to their guardrails, inspect recent FFI, metadata parsing, storage, and service
changes for hidden stack temporaries before treating GPIO, flashing, or serial
as the root cause.

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
The ESP32-C3 reference firmware uses a 4,864-byte protocol/main stack and a
24,576-byte VM worker stack. Keep the stack harness in the validation path
before lowering either budget again. The harness fails with the captured
resource frame when protocol/main unused stack drops below 768 bytes or VM
worker unused stack drops below 384 bytes.

`scripts/lib/ram-workload-harness.sh` provides the shared targeted RAM and
stack attribution helpers for ESP32-C3 hardware scripts. Target wrappers use it
to reset heap high-water attribution, record `device resources` snapshots, check
protocol/main and VM worker stack accounting, and write a stable `summary.tsv`
with stack, heap, runtime-static, and dispatch metrics. The resources response
also reports `proto_stack_pre_*` so the harness can distinguish stack already
consumed before resource-response encoding from stack pressure caused by the
diagnostic command itself. Stack values are Zephyr high-water readings for the
current boot, so unchanged stack values across rows mean the peak happened
before or during the earliest matching snapshot, not that every workload used
the same stack depth. Before each workload boundary, the harness runs
`device resources --reset-heap-max`; each later `heap_max_alloc_bytes` row is
therefore the peak since that reset, while `heap_alloc_bytes` remains the live
allocation count at sample time.

`scripts/c3-supermini-measure-ram-workloads.sh` is the Super Mini RAM workload
wrapper. It formats app storage, installs the GPIO9 input summary app, launches
it, dispatches a serial `SELECT`, and records snapshots after format, install,
launch, and dispatch under `target/hardware-tests/ram-workloads/summary.tsv`.
The input app launch and `SELECT` dispatch stay in one foreground session,
while later independent display, system-resource, and Wi-Fi workload groups
reset runtime lifecycle state before launch. Current hardware coverage measured
the protocol/main stack at 3,904 bytes used with 960 bytes free and the VM
worker stack at 16,112 bytes used with 528 bytes free. The current targeted RAM
workload measured Wi-Fi AP start at `heap_max_alloc_bytes=59724`, and Wi-Fi AP
stop at `heap_max_alloc_bytes=59752`, leaving at least 5,784 bytes below the
configured heap ceiling in those reset-bounded rows.

`scripts/xiao-esp32c3-measure-ram-workloads.sh` is the XIAO ESP32-C3 e-paper
RAM workload wrapper. It builds and flashes the selected XIAO target unless
`--skip-flash` is passed, then records storage-format, e-paper GRAY2 display,
system-resource, and Wi-Fi AP start/stop rows under
`target/hardware-tests/xiao-ram-workloads/summary.tsv`. Use the XIAO wrapper
for the default dev target RAM evidence pass before changing shared ESP32-C3
budgets. Re-run the relevant workload harness after stack-budget changes when
exact free-byte margins are needed. `heap_max_headroom_bytes` is computed from
the configured 65,536-byte Zephyr system heap and each row's allocation
high-water mark, so AP/Wi-Fi pressure can be compared across workload rows
without adding another firmware response metric.
Current XIAO workload evidence measured the protocol/main stack at 2,356 bytes
used with 2,508 bytes free, the VM worker stack at 16,192 bytes used with
8,384 bytes free, Wi-Fi AP start at `heap_max_alloc_bytes=59748`, and Wi-Fi AP
stop at `heap_max_alloc_bytes=59776`, leaving at least 5,760 bytes below the
configured heap ceiling in those reset-bounded rows.
`heap_largest_free_supported=0` and `heap_largest_free_bytes=0` mean the
current Zephyr public heap API does not expose a safe non-mutating
largest-free-block query. `sys_heap_runtime_stats_get()` returns free,
allocated, and max-allocated byte counts; `sys_heap_print_info()` prints bucket
details but does not provide a bounded numeric telemetry value; heap listeners
report allocation/free events rather than the current largest free block.

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
target metadata. Host `app launch` accepts the lifecycle request, `device output`
remains empty, and `device errors` reports the retained runtime failure as
`runtime=host_error`, proving target validation rejects the binding before app
code runs while the protocol remains responsive.

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
polls `service.wifi.result()` from a timer and prints only `ok`, `error`, and
`count`; the script rejects raw BSSID, MAC, or local IP patterns in captured
output. In the default full hardware suite it runs with `--require-real-wifi`,
which requires a successful real Zephyr Wi-Fi scan.
On output timeout, the script writes best-effort `output-timeout-resources.out`
and `output-timeout-errors.out` captures under
`target/hardware-tests/wifi-scan/`; these files may contain the last
stack/heap/error evidence if the protocol path is still responsive.

`scripts/c3-supermini-test-wifi-list-api.sh` runs after Wi-Fi scan coverage and
before Wi-Fi AP coverage. It installs
`tests/hardware/c3-supermini/wifi-list-summary` and launches a SquidScript app
that starts `service.wifi.scan()`, polls `service.wifi.result()`, and reads
bounded rows through `service.wifi.scanNetwork(index)`. The app prints only
redacted per-network structure: SSID length, channel, RSSI, auth, and hidden
flag. It does not print SSIDs or BSSIDs. The script rejects raw BSSID, MAC, or
local IP patterns in captured output, rejects `null` auth values in AP rows,
and `--require-real-wifi` requires at least one redacted `wifi ap` record from
the real Zephyr Wi-Fi backend. On output timeout, the script writes best-effort
`output-timeout-resources.out` and `output-timeout-errors.out` captures under
`target/hardware-tests/wifi-list/`.

`scripts/zephyr-test-wifi-station-api.sh` is explicit-credentials-only
and is not part of the default full hardware suite. It skips successfully unless
`SQUID_WIFI_STATION_SSID` and `SQUID_WIFI_STATION_PASSWORD` are set. When those
variables are present, it provisions profile `dev` through
`squidc device wifi-profile --ssid-env SQUID_WIFI_STATION_SSID --password-env
SQUID_WIFI_STATION_PASSWORD`, installs
`tests/hardware/c3-supermini/wifi-station-summary`, and launches a summary-only
app that calls `service.wifi.connect("dev")`, polls `service.wifi.operation()`,
and then reads `service.wifi.status()`. The script requires the start request
to be accepted and `status.connected == true`, prints command names and lengths
only, and rejects raw SSIDs, passwords, BSSIDs, MACs, or local IP patterns in
captured output. It rejects unexpected `device errors` output, but allows the
known `error=display=unavailable code=-19 (ENODEV)` diagnostic so the Wi-Fi
station check can run on target setups where configured display hardware is
intentionally not connected.

`scripts/zephyr-test-radio-concurrency.sh` is an opt-in radio concurrency check
for Zephyr ESP32-C3 targets and is not part of the default full hardware suite.
It defaults to `--target xiao-esp32c3-gdeq0426t82-sd` and accepts
`--target <id>`, `--skip-flash`, `--require-ble-reconnect`,
`--device <name-or-address>`, and `--host-wifi-iface <iface>`. The script may temporarily take over the host
Wi-Fi and Bluetooth controllers. It builds or flashes the selected target,
formats app storage, launches fallback `main` so `service.ble.start` registers
an active file-transfer profile, discovers the target with host Bluetooth,
connects to it, and keeps that BLE connection active while normal SquidScript
Wi-Fi apps exercise scan/list, target AP
start/client-association/DHCP-lease/stop, and station connect/disconnect
through a generated temporary host AP. These operations exercise the firmware
Wi-Fi service-state machine through scan, AP, and station transitions rather
than only probing individual driver calls. It finishes by checking Wi-Fi status. Pass
`--require-ble-reconnect` only when validating BLE re-advertising after host
disconnect; the default matrix proves active BLE coexistence during Wi-Fi work
without making reconnectability a pass requirement.

ESP32-C3 targets can enable Wi-Fi and BLE in the same firmware image. The
ESP32-C3 datasheet lists 2.4 GHz Wi-Fi, Bluetooth LE, and an internal
coexistence mechanism that lets Wi-Fi and Bluetooth share the same antenna:
<https://documentation.espressif.com/esp32-c3_datasheet_en.html>. Espressif's
ESP32-C3 RF coexistence guide describes the implementation as shared RF
time-division multiplexing rather than independent simultaneous Wi-Fi/BLE
radios, and its coexistence table marks STA scan/connect/connected activity
with BLE advertising/connecting/connected as supported. The same table marks
SoftAP beaconing with BLE as stable, while SoftAP with connected client traffic
is supported but performance-unstable:
<https://docs.espressif.com/projects/esp-idf/en/v5.2/esp32c3/api-guides/coexist.html>.
The Zephyr XIAO ESP32-C3 board documentation identifies the board as ESP32-C3
based with Wi-Fi and BLE support:
<https://docs.zephyrproject.org/latest/boards/seeed/xiao_esp32c3/doc/index.html>.

For SquidScript ESP32-C3 firmware, treat Wi-Fi/BLE coexistence as a target
capability that must be verified on the actual board and antenna arrangement.
The XIAO target with the external antenna has hardware evidence for an active
BLE connection staying connected while SquidScript apps perform Wi-Fi scan/list,
target AP start/client-association/DHCP-lease/stop, station connect/disconnect
through a temporary host AP, and Wi-Fi status. The ESP32-C3 Super Mini uses the same SoC
radio class, but clone-board antenna and RF layout quality are variant-dependent;
run the radio concurrency script against the specific Super Mini board before
claiming the same coexistence quality. When both radios are enabled, the
generated Zephyr `.config` for the image under test should contain
`CONFIG_ESP32_SW_COEXIST_ENABLE=y`.

The radio concurrency script uses the portable fixtures under
`tests/hardware/zephyr/radio-concurrency/`. These fixtures print only redacted
Wi-Fi structure and command status: counts, SSID lengths, channels, RSSI, auth,
hidden flags, operation status, and error strings. The script stores raw host
tool output under `target/hardware-tests/radio-concurrency/` for local
diagnosis but does not print SSIDs, BSSIDs, MAC addresses, local IP addresses,
or generated credentials. It deletes the generated temporary station password
and host NetworkManager profiles on exit. Like the station check, it rejects
unexpected `device errors` output but allows the known
`error=display=unavailable code=-19 (ENODEV)` diagnostic for target setups where
the configured display is intentionally disconnected.

The target-aware XIAO hardware wrapper intentionally runs `radio-concurrency`
before `ap-after-station`. Treat this as reset-boundary recovery coverage: after
Wi-Fi scan/list, AP client association, station connect/disconnect, and BLE
disconnect, `device reset` must still answer and leave the follow-on
`ap-after-station` check able to install and launch its fixture. If this
boundary fails, inspect the captured protocol diagnostics first; when they are
empty, inspect the raw serial diagnostics before classifying the failure as
host timing, firmware recovery, USB re-enumeration, or target radio state.

The target AP row in `scripts/zephyr-test-radio-concurrency.sh` proves AP
connectability with the host Wi-Fi interface: the host associates to the target
AP, receives an IPv4 DHCP lease in the target AP subnet, and the SquidScript AP
fixture reports a positive `service.wifi.status().clients` count. The script
prints only boolean/count proof and keeps raw host NetworkManager output in the
gitignored hardware-test work directory for local diagnosis.

`scripts/c3-supermini-test-wifi-ap-api.sh` runs after Wi-Fi list coverage and
before the final visible LED check in the default full hardware suite. It
installs `tests/hardware/c3-supermini/wifi-ap-summary`, launches a summary-only
app that calls `service.wifi.startAP("SquidScript")` and
`service.wifi.getAPIP()`, sends `SELECT` to call `service.wifi.stopAP()`, and
requires start, AP IP lookup, and stop to report success without printing the
raw AP SSID, BSSIDs, MACs, or local IP patterns in captured output. AP start
also starts a bounded DHCPv4 server on the AP interface. This Super Mini script
is an AP API check only; use `scripts/zephyr-test-radio-concurrency.sh` when the
acceptance criterion includes an external AP client association and DHCP lease
proof. When using a host Wi-Fi interface for AP proof, do not print nearby
SSIDs, BSSIDs, MACs, or assigned IP addresses in logs; report only whether the
test AP was found, whether the client connected, and whether an IPv4 lease was
assigned.

`scripts/c3-supermini-test-ble-smoke.sh` runs after Wi-Fi coverage and before
physical-input/final-visible checks. It builds and flashes the selected
ESP32-C3 Super Mini target firmware, verifies the serial log line
`BLE advertising started: ESP32-C3 Super Mini`, and uses `bluetoothctl` to scan
for the advertised name when the host has a usable Bluetooth controller. If host
Bluetooth tooling or a controller is unavailable, or the host does not discover
the device within the bounded scan window, the script reports that host
discovery was not confirmed after the serial advertising proof. Use
`--require-host-scan` when the host-side BLE path must be proven. This check
validates that the target radio backend is enabled by target metadata; it does
not validate BLE file-transfer chunking, staging, or app install.

`scripts/zephyr-test-ble-reconnect.sh` is a focused re-advertising check. It
builds or flashes the selected target, launches fallback `main` to start the
BLE file-transfer profile, discovers the initial advertisement from a host
Bluetooth controller, connects to the device, requests a host disconnect,
watches the firmware log `BLE advertising stopped before restart` followed by
`BLE advertising restarted after disconnect`, and rescans the host controller to
confirm a fresh advertisement is observed. Use it to verify the
`bt_le_adv_stop` → `bt_le_adv_start` restart sequence in
`firmware/zephyr/src/ble_smoke.c` actually puts bytes back on the air. The
companion host-side ztests in `firmware/zephyr/tests/ble-smoke` drive the
state machine on `native_sim` with function-pointer stubs, and the
`scripts/zephyr-test-ble-smoke.sh` wrapper runs them through Twister. Pass
`--skip-flash` to either script when preserving the current firmware session
is more important than a fresh baseline; the reconnect script still requires a
host Bluetooth controller.

For non-Super-Mini ESP32-C3 targets such as
`xiao-esp32c3-gdeq0426t82-sd`, run the same checks against the selected target:

- build/flash the selected target
- launch fallback `main` so `service.ble.start("file-transfer", ...)` registers
  an active profile
- use a host Bluetooth controller to scan for the advertised name
- connect to the discovered peripheral
- disconnect after verification and require the restart log sequence
- remove the device from the BlueZ cache and require a fresh `[NEW]`/`[CHG]`
  rediscovery event after rescan

Do not print BLE MAC addresses in logs. Redact addresses and report only
whether advertising, host discovery, connection, and disconnection succeeded.

### BLE File Transfer Service

The BLE file-transfer GATT service is registered in-process through
`firmware/zephyr/src/ble_file_transfer.c` with `BT_GATT_SERVICE_DEFINE`.
There is no explicit init call. The service becomes discoverable when the BLE
radio starts advertising and includes the custom 128-bit service UUID in
advertising data.
Legacy BLE advertising and scan-response payloads are length-limited. If the
configured target name is too long for the current advertising data shape, host
scans may show a truncated name even though the serial log prints the full
`CONFIG_BT_DEVICE_NAME`. Treat this as a naming/payload issue, not as proof
that BLE failed, and shorten the target BLE name or move to an extended
advertising/scan-response design before relying on exact full-name discovery.

### BLE File Transfer end-to-end

`scripts/zephyr-test-ble-file-transfer.sh` is the end-to-end hardware
check for the BLE File Transfer work. It builds the XIAO ESP32-C3
e-paper dev target firmware, flashes it via `west flash -d
build/zephyr/xiao-esp32c3-gdeq0426t82-sd`, boots the default fallback
installer, and then runs `squidc app push` to push a payload over the custom
BLE GATT transfer service. The fallback starts `service.ble.start` on
`app.start`, receives `.sqbc`, calls `app.install(ev.upload)`, and launches
the installed app.

`scripts/zephyr-test-ble-installed-receiver.sh` verifies the installed-app
route path. It installs `tests/hardware/zephyr/ble-installed-receiver` as a
normal app registry entry, launches it, pushes an exiting SQBC payload to that
installed foreground receiver, verifies the completion event is delivered to
that installed app, waits for the launched app to exit back to the receiver,
then pushes the same payload again. This covers registry-slot BLE routing and
foreground-return BLE reactivation separately from the fallback-slot installer.
If either BLE transfer script fails during route selection, inspect
`squidc device errors`: route-table ambiguity or stale registry-slot state is
reported as an `invariant.ble.*` diagnostic rather than left as only a raw GATT
write failure.

The host-side driver is the Rust `squidc app push` client and requires a host
Bluetooth adapter. The `squidc` BLE push tests exercise the GATT call order with
a fake client, independent of host BLE capability. The native ztests under
`firmware/zephyr/tests/ble-file-transfer-{parse,staging,dispatch}`,
`firmware/zephyr/tests/ble-trigger-table`, and
`firmware/zephyr/tests/ble-app-install` exercise the firmware sides (file-name
parsing, staging lifecycle, dispatch handoff, profile table, and the
`app.install` validation path) without BLE.

Required arguments: `--device <name-or-address>` (the BLE device the host will
connect to). Optional: `--port <serial-port>` (auto-detected via
`scripts/lib/serial-port.sh::resolve_esp.serial_port`), `--source <file.squid>`,
and `--skip-flash` (build only; assume the firmware is already on the device).

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
