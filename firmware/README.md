# SquidScript Firmware

SquidScript real firmware targets are Zephyr-backed. The current firmware tree
is `firmware/zephyr`; it is the only forward firmware path for ESP32-C3 and
future real boards.

Rust remains authoritative for the compiler, SQBC tooling, and VM semantics.
Real firmware hosts call the Rust VM through the `squidvm-ffi` C ABI, while
Zephyr owns platform services such as GPIO, timers, storage, serial/shell,
logging, Wi-Fi, networking, power, and hardware diagnostics.

## Current Zephyr Firmware

The Zephyr app is the canonical firmware implementation:

- `firmware/zephyr/CMakeLists.txt`: Zephyr app entrypoint.
- `firmware/zephyr/prj.conf`: serial, shell, logging, flash-map, NVS, and
  LittleFS configuration for the host runtime.
- `firmware/zephyr/src/main.c`: Zephyr boot and runtime entrypoint.
- `firmware/zephyr/src/squidvm_ffi.h`: C header for the Rust VM FFI.
- `compiler/rust/crates/squidvm-ffi`: Rust staticlib/rlib exposing the VM C
  ABI.

The default firmware target is `xiao-esp32c3-gdeq0426t82-sd`, built on
Zephyr's upstream `xiao_esp32c3` board with a SquidScript-owned overlay for
the Seeed XIAO ePaper Driver Board and 4.26 inch GDEQ0426T82/SSD1677 panel.
The overlay describes target-environment wiring; it does not fork the upstream
board definition. The ESP32-C3 Super Mini remains an explicit regression
target through the `c3-supermini-*` wrappers.

## Commands

From the repository root:

```sh
scripts/zephyr-setup.sh
scripts/zephyr-build.sh
scripts/zephyr-flash.sh
scripts/xiao-esp32c3-epaper-zephyr-monitor.sh
scripts/c3-supermini-build.sh
scripts/c3-supermini-flash.sh
scripts/c3-supermini-zephyr-monitor.sh
```

`scripts/zephyr-build.sh` and `scripts/zephyr-flash.sh` use the current
default XIAO target. The XIAO-specific wrappers expose the same target
explicitly. The `c3-supermini-build.sh` and `c3-supermini-flash.sh` wrappers
select the Super Mini target JSON, overlay, fallback app, Zephyr board, and
build directory before sourcing the shared environment. Flashing does not
auto-monitor unless `MONITOR_AFTER_FLASH=1` is set.

Zephyr setup is host-specific but repository-local by default:

- `scripts/zephyr-setup.sh` installs missing generic host tools with Homebrew,
  creates `target/zephyr/venv`, installs `west` into that venv, initializes and
  updates a Zephyr workspace under `target/zephyr/workspace`, installs Zephyr's
  base/build-test Python requirements plus the repo-local requirements for
  Twister and optional runner discovery, and fetches `hal_espressif` blobs.
  When no SDK is detected, it uses Zephyr's
  supported `west sdk install` flow to install the RISC-V Zephyr GNU toolchain
  under `target/zephyr/sdk`; pass `--skip-sdk` to leave SDK installation
  manual.
- `scripts/zephyr-env.sh` exports the local `west` path, `ZEPHYR_BASE` when the
  workspace exists, the default XIAO `ZEPHYR_BUILD_DIR`, `ZEPHYR_BOARD`,
  `SQUID_ZEPHYR_TARGET_JSON`, `SQUID_ZEPHYR_TARGET_OVERLAY`, and fallback app
  source.
- `SQUID_ZEPHYR_HOME` overrides the default `target/zephyr` tool/workspace
  root.

Target-generated Kconfig enables device drivers for declared target devices.
For example, an ESP32-C3 target may expose PWM-capable GPIOs without enabling
Zephyr's PWM driver until the target declares a PWM-backed device such as an
onboard indicator.

The setup script does not use `rpm-ostree`. Set `ZEPHYR_SDK_INSTALL_DIR` if an
existing SDK is installed somewhere the env script cannot find.

## Memory Numbers

When asking for "memory", report RAM numbers by default. Zephyr image size,
flash partition usage, LittleFS usage, and installed app storage are flash
storage numbers and should be requested or reported separately.

RAM checks should use Zephyr build output, Zephyr map/size tooling, and the
Zephyr diagnostic command surface.
`scripts/zephyr-ram-audit.sh` checks the `dram0_0_seg` guard and prints
structured top static RAM symbols. Use `SQUID_ZEPHYR_RAM_SYMBOL_COUNT` to show
more or fewer symbol rows during optimization work.
