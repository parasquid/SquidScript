# Firmware & Script Tooling Reference

Command, script, path, and target reference for firmware/hardware work. This is
the *how* — the exact wrappers, ports, venv, and target names. The always-fire
*disciplines* that govern when to escalate, when to run hardware tests, and the
single-port safety rule live in `AGENTS.md` under "Script And Firmware Tooling
Discipline"; read both when doing firmware or hardware work.

## Firmware Source

- Firmware source for the canonical ESP32-C3 firmware lives under
  `firmware/zephyr`; the old Rust ESP32-C3 firmware tree has been removed.

## Build, Flash, Serial

- Before reporting that firmware build, flashing, serial, or hardware checks do
  not work in this environment, check the relevant repository docs and wrapper
  scripts first. Prefer the documented wrapper command over ad hoc direct tool
  invocations, and only call something blocked after the documented path fails.
- Use `cargo run -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd`
  to build or type-check the XIAO ESP32-C3 e-paper default dev firmware
  binary. `squidc target` resolves target JSON metadata, Zephyr board,
  overlay, fallback app, generated Kconfig path, and build directory.
  The XIAO is the default dev target — `scripts/zephyr-env.sh` defaults
  to it, `scripts/zephyr-test-radio-concurrency.sh` defaults to it, and
  `docs/hardware_target_tests.md` documents it as the default. The
  ESP32-C3 Super Mini (`esp32c3-super-mini`) remains a supported
  regression hardware target, with its own scripts under
  `scripts/c3-supermini-*.sh` and its full suite
  `scripts/c3-supermini-test-hardware.sh`.
- For hardware flashing, use `west flash -d build/zephyr/<target>` after
  building. The `target build` command compiles but does not flash; the
  wrapper convention is to run `west flash` explicitly. The serial port
  is auto-detected via `scripts/lib/serial-port.sh::resolve_esp.serial_port`
  and exported as `ESPFLASH_PORT`. `scripts/zephyr-test-ble-file-transfer.sh`
  is the reference for the full end-to-end flash + protocol + BLE flow.
- For firmware flashing scripts, avoid auto-monitoring by default when USB reset or re-enumeration can break the serial session. Prefer `squidc device monitor` for XIAO ESP32-C3 SquidScript output, and use explicit opt-in monitoring such as `MONITOR_AFTER_FLASH=1` only when needed.
- Do not filter or suppress flashing tool stderr in firmware scripts. Surface warnings and errors directly, and document known harmless tool warnings instead of hiding them.

## Zephyr Environment & Test Wrappers

- `scripts/zephyr-env.sh` prepends `target/zephyr/venv/bin` to `PATH`, so
  any `python3` invoked after sourcing it is the Zephyr venv Python, not
  the system Python. Python packages installed at the system level are NOT
  visible to wrappers that source `scripts/zephyr-env.sh`; install wrapper
  Python dependencies into the Zephyr venv when a wrapper needs them.
- Use `scripts/zephyr-test-protocol.sh` for the Zephyr native protocol ztests
  instead of invoking `west twister` directly. The wrapper sources
  `scripts/zephyr-env.sh`, which adds the repo-local `target/zephyr/venv/bin`
  `west` to `PATH` and sets the expected Zephyr environment.
- Run Zephyr Twister protocol tests outside the Codex sandbox. Twister uses a
  Python multiprocessing manager that opens a local socket; sandboxed runs can
  fail with `PermissionError: [Errno 1] Operation not permitted` or an
  `EOFError` before building or running tests. Treat that as an environment
  limitation, rerun the documented wrapper with escalated execution, and do not
  diagnose it as a source/test failure.
- Run ESP32-C3 Super Mini Zephyr build wrappers outside the Codex sandbox in
  this environment. Zephyr/ccache may write host cache files outside the
  workspace, so sandboxed firmware builds can fail with read-only filesystem
  errors unrelated to the source.
- Dry-run new scripts before calling them ready: run `bash -n`, verify required tools and Rust targets, check wrapped command help where practical, and confirm wrapper scripts forward user-supplied arguments.

## Reproduction And Visual Verification

- Hardware scripts that drive an end-to-end flow (for example
  `scripts/xteink-x4-test-binbook-reader.sh`) double as reproduction harnesses:
  they leave the device in a known screen state at the end of the run, which is
  suitable for asking the user to inspect the panel visually.
- A script's serial assertions (`draw=...` lines, `device output`, `device
  errors`, `device resources`) are a *serial gate*. A passing serial gate does
  not imply a clean panel: optical bugs such as ghosting, half-screen splits, or
  stale regions only surface under visual inspection.
- When reproducing an optical bug, run the relevant script, confirm the serial
  gate still passes, then ask the user present at the device to describe what is
  on the panel. Treat the visual report as a separate gate from the serial
  assertions.
- Capture `device drawlog` immediately after the user reports the visual state
  and before driving further input. The drawlog shows the ops that produced the
  visible frame and is the primary evidence for compositor and refresh bugs.
- After the run, the captured `.out` files under
  `target/hardware-tests/<script-name>/` are durable diagnostic artifacts;
  read them instead of re-running the script when investigating a reported
  optical regression from the same session.

## Hardware Target Tests

- Hardware target tests are listed in `docs/hardware_target_tests.md`; use that inventory to identify real-device tests before running them.
- Use `cargo run -p squidc -- hardware test --target <target-id>` for the
  target-aware regression wrapper when it covers the changed path. The wrapper
  selects checks from target metadata features, flashes once unless
  `--skip-flash` is passed, runs portable example app tests, and delegates
  BLE/Wi-Fi checks to the existing hardware scripts.
- Clearly report host visibility limits, such as Codex sandbox sessions that cannot see `/dev/ttyACM*`, `/dev/ttyUSB*`, or `/dev/bus/usb`.
- When running the ESP32-C3 Super Mini hardware target suite, use
  `scripts/c3-supermini-test-hardware.sh` so stateful checks run first and the
  blinky app runs last. Blinky is the final visible board-state check and
  should be left running unless the user asks otherwise. Do not run any serial
  command after the final blinky launch unless you are deliberately debugging
  the final board state. The XIAO target uses the `scripts/zephyr-test-*.sh`
  family of scripts (each script is a single target-aware check); there is no
  single XIAO full-suite wrapper, so prefer running the individual scripts that
  cover the firmware path under test.
- Hardware target tests and serial/flashing commands must run outside the Codex sandbox. Sandboxed sessions do not reliably expose `/dev/ttyACM*`, `/dev/ttyUSB*`, or `/dev/serial/by-id`, even after host reboot. Use escalated command execution for ESP32-C3 serial visibility checks and hardware target tests on either the XIAO default dev target or the Super Mini regression target.
- For ESP-IDF hardware-isolation experiments under
  `experiments/esp32c3-supermini/firmware/esp-idf-softap-hwtest`, the user has
  approved the repository's documented containerized ESP-IDF build path when no
  local `idf.py` is installed. Do not re-ask for approval just because Podman or
  Docker will run the official Espressif IDF image with the experiment mounted;
  still avoid passing Wi-Fi credentials to containers unless the specific test
  requires station credentials.
- When troubleshooting ESP32-C3 Super Mini flashing access, check
  `firmware/README.md` and the Zephyr wrapper scripts before suggesting broader
  sudo changes.
- For `hardware.gpio.*` work on the ESP32-C3 Super Mini, run the serial GPIO REPL session and the blinky upload session when hardware is available; the blinky check requires both serial assertions and physical onboard LED observation. The XIAO ESP32-C3 e-paper target has **no onboard LED** wired into the firmware (`targets/xiao-esp32c3-gdeq0426t82-sd.target.json` declares no `pwm-led` indicator) — its "visible board state" is the GDEQ0426T82 e-paper display, not an LED. LED-observation-based tests such as `scripts/c3-supermini-test-blinky.sh` and `scripts/c3-supermini-test-blink.sh` are Super-Mini-only and will not work on the XIAO. For XIAO GPIO/input work, rely on serial output and e-paper drawlog evidence, not LED observation.
- When analyzing hardware benchmark results, inspect the full distribution and
  explain outliers before summarizing. Do not assume convenient causes such as
  caching, timing noise, or hardware quirks when a pattern aligns with app
  logic, wraparound boundaries, state changes, or event counts. Correlate
  anomalous rows with trace, drawlog, state, resource metrics, or fixture
  source before calling the benchmark valid.

## REPL & CLI

- For REPL work, default app and firmware profiles are `dev`. Hardware target tests should include `tests/repl/default-dev.session`, which intentionally does not set `:profile dev`.
- Do not require `--target` for normal `squidc repl` upload/run flows. SquidScript apps compile against the portable language/runtime API; target definitions are opt-in for explicit target checks, simulator config, firmware metadata, docs, and autocomplete.
- When changing the `squidc` CLI surface, update `docs/squidc_cli.md`, scripts, and command examples in docs in the same change.
