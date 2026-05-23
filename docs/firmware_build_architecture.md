# SquidScript Firmware Build Architecture

Status: Current direction
Purpose: Define the real-firmware build model for SquidScript targets.

## Current Backend

Zephyr is the only supported real-firmware backend. The primary firmware app is
`firmware/zephyr`, built with Zephyr CMake/Kconfig through `west`.

Rust remains part of the firmware architecture only at the VM boundary:

- `squidc-core`, `squidvm-core`, and SQBC tooling remain Rust.
- `squidvm-ffi` exposes VM behavior through a small C ABI/staticlib boundary.
- Zephyr owns the real firmware host layer, scheduler integration, drivers,
  storage, serial/shell, logging, networking, Wi-Fi, power, and diagnostics.

There is no fallback Rust ESP firmware path and no compatibility shim for the
old serial protocol or flash layout. If behavior fails on ESP32-C3 under
Zephyr, treat it as a Zephyr implementation, driver, configuration, or
workaround task.

## Repository Interfaces

From the repository root:

```sh
scripts/zephyr-setup.sh
scripts/c3-supermini-build.sh
scripts/c3-supermini-flash.sh
scripts/c3-supermini-zephyr-monitor.sh
```

The build and flash wrappers delegate to Zephyr-specific scripts and source
`scripts/zephyr-env.sh`. By default, `SQUID_ZEPHYR_HOME` is
`target/zephyr`, with `west` installed in `target/zephyr/venv` and the Zephyr
workspace in `target/zephyr/workspace`.

`scripts/zephyr-setup.sh` prepares that local tooling area. It may install
generic host tools with Homebrew (`cmake`, `ninja`, `dtc`, `wget`, and `xz`),
creates the Python venv, installs `west`, initializes and updates the Zephyr
workspace from `firmware/zephyr/west.yml`, and runs
`west blobs fetch hal_espressif` for Espressif RF blob support. If no SDK is
detected, it runs Zephyr's supported `west sdk install` flow for the
`riscv64-zephyr-elf` GNU toolchain under `target/zephyr/sdk`; pass
`--skip-sdk` to leave SDK installation manual. The setup path does not use
`rpm-ostree`.

`ZEPHYR_BOARD` selects the board. The default wrapper value is an unverified
ESP32-C3 Super Mini clone-board default and must be corrected when hardware
bring-up identifies the exact Zephyr board target.

## Rust VM Static Library

`firmware/zephyr/cmake/squidvm_ffi.cmake` builds and links `squidvm-ffi` into
Zephyr targets. Native simulator builds use the host Rust target so ztests can
exercise the C header and linker path. ESP32-C3 builds use
`riscv32imc-unknown-none-elf` with the `squidvm-ffi` `zephyr` feature, which
compiles the FFI crate as `no_std` with a firmware panic handler.

The helper resolves `cargo` and `rustc` through the stable `rustup` toolchain
instead of relying on whatever Rust binaries appear first on `PATH`. This is
required on hosts where Homebrew Rust is present but the rustup target library
for `riscv32imc-unknown-none-elf` is installed under the rustup toolchain.

Current Zephyr ztest coverage proves the firmware can include
`src/squidvm_ffi.h`, link the Rust static library, call context metadata ABI
functions, and link the resumable dispatch/storage request entry points. The
Rust FFI tests exercise resumable dispatch through the C ABI for SQBC chunk
reads, state load, state save, completion, and trace ordering. Full VM hosting
now has a Zephyr-side storage adapter that maps FFI storage requests to
backend callbacks for SQBC reads and app state load/save/reset. Native Zephyr
ztests initialize the linked Rust VM through `squidvm_ffi.h`, run a real SQBC
fixture with resumable dispatch, complete its storage requests through the
adapter, and verify trace callback ordering. Zephyr also has a file-backed
storage backend that uses `fs_*` APIs for SQBC byte-range reads and app-state
load/save/reset paths; native ztests cover it through a host-mounted filesystem.
Display draw-log, GPIO, indicator, timer, app lifecycle, and Wi-Fi VM service
calls now cross the Rust FFI boundary into Zephyr callbacks. Zephyr now
performs installed-app foreground handoff for `app.launch` and `app.exit` with
a bounded return stack and `device lifecycle` diagnostics. `app.arm` and
`app.disarm` now manage bounded timer-triggered armed app registrations and
dispatch armed timer events as foreground app starts. The current Zephyr Wi-Fi
callbacks for status, scan, AP start/stop, station connect/disconnect, and AP
IP return truthful fallback records that report `unsupported` without
credentials or RF scan data; real Zephyr Wi-Fi management scan/AP/station work
remains a runtime-service task.
The app-store layer now derives bounded file paths for `main.sqbc` and app
state from a mount point plus validated app ID, and ESP32-C3 firmware attempts
to mount the `storage_partition` LittleFS volume at `/sq` during boot without
blocking framed serial command transport if storage is unavailable. The next
storage step is to add install-time app directory creation, boot-time registry
scanning, and package-resource lookup paths, then expand callbacks for
diagnostics, resources, real app lifecycle orchestration, real Wi-Fi service
records, and explicit error mapping.

## Target Definitions

Target JSON describes portable capabilities and board metadata. It must not
make Zephyr Kconfig symbols, devicetree nodes, thread handles, or driver state
part of the SquidScript language contract.

Backend-specific generated artifacts may include:

- Zephyr devicetree overlays.
- Kconfig fragments.
- C target configuration headers.
- Flash-map and storage layout metadata.
- Firmware manifest/provenance data.
- Generated app capability tables.

## Runtime Boundary

The Zephyr host calls the Rust VM through C ABI functions for:

- context allocation sizing and initialization
- SQBC byte-range reads
- event dispatch
- storage suspend/resume
- state inspection and mutation
- diagnostics and error mapping
- service result conversion

The browser simulator remains separate. It shares the SquidScript service
contract but does not use Zephyr or the firmware C ABI.

## Non-Goals

- Do not maintain the old Rust ESP firmware as a fallback.
- Do not reimplement the VM in C/C++.
- Do not expose Zephyr objects in SquidScript source, compiler core, SQBC, or
  portable app APIs.
- Do not add SQBC or firmware compatibility modes before 1.0.
