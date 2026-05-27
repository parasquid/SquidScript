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
scripts/zephyr-test-protocol.sh
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
workspace from `firmware/zephyr/west.yml`, installs Zephyr's base/build-test
Python requirements plus `firmware/zephyr/requirements-twister.txt`, and runs
`west blobs fetch hal_espressif` for Espressif RF blob support. If no SDK is
detected, it runs Zephyr's supported `west sdk install` flow for the
`riscv64-zephyr-elf` GNU toolchain under `target/zephyr/sdk`; pass
`--skip-sdk` to leave SDK installation manual. The setup path does not use
`rpm-ostree`.

`ZEPHYR_BOARD` selects the board. The ESP32-C3 Super Mini wrappers default to
Zephyr's `esp32c3_supermini` board target. Override `ZEPHYR_BOARD` when testing
a different ESP32-C3 board variant.

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
The Zephyr VM runtime scratch buffer is sized to the FFI storage transfer
capacity so the firmware does not reserve a full max-app buffer when the VM
only needs one bounded code/storage chunk for resumable dispatch.
The repo-local Zephyr setup installs the lightweight Twister dependencies
needed to run `firmware/zephyr/tests/protocol` through Zephyr's test runner.
Use `scripts/zephyr-test-protocol.sh` for that protocol suite; it selects
`native_sim/native/64`, which avoids requiring host 32-bit libc headers.
Temp-run volatile app state is sized to the VM saved-state capacity, not the
larger SQBC transfer chunk capacity.
The ESP32-C3 linked Rust VM context reservation is capped at 11,776 bytes and
checked against the FFI-reported context size in Zephyr ztests. Native simulator
ztests use a larger host-only context reservation because the host Rust ABI has
larger pointer-sized VM structures; this does not change the ESP32-C3 runtime
RAM budget.
Protocol transfer sessions keep full resource-path storage for package paths
but use smaller internal staging-path buffers for fixed firmware staging
filenames.
Resource diagnostics are encoded directly into the caller-owned 992-byte
protocol response buffer and do not keep a resident metric staging array.
The protocol/main thread stack is currently 5 KiB and the VM worker stack is
18 KiB. Resource diagnostics expose each stack's high-water use separately so
budget reductions can be tied to measured workloads instead of inferred from
static allocation alone.
Display draw-log, GPIO, indicator, timer, app lifecycle, and Wi-Fi VM service
calls now cross the Rust FFI boundary into Zephyr callbacks. Zephyr now
performs installed-app foreground handoff for `app.launch` and `app.exit` with
a bounded return stack, foreground timer cleanup, and `device lifecycle`
diagnostics. `app.arm` and `app.disarm` now manage bounded timer-triggered
armed app registrations and dispatch armed timer events as foreground app
starts. The current Zephyr Wi-Fi callbacks use Zephyr Wi-Fi management for
status, scan, AP start/stop, AP IP reporting, station connect/disconnect, and
station DHCP/IP status reporting. `device wifi-profile` stores one volatile
bounded station profile in runtime memory without echoing SSIDs or passwords.
AP start also starts Zephyr's bounded DHCPv4 server on the AP interface; an
external-client association and lease proof remains separate hardware coverage.
The ESP32-C3 reference configuration intentionally uses smaller native
networking packet/buffer pools than Zephyr's Ethernet-oriented defaults because
current firmware networking is low-throughput control-plane traffic, not a TCP
or bulk-transfer workload. It also uses measured socket-service,
network-management event, ESP timer task, and network RX stack budgets for the
same current scope.
The app-store layer now derives bounded file paths for `main.sqbc` and app
state from a mount point plus validated app ID, and ESP32-C3 firmware attempts
to mount the `storage_partition` LittleFS volume at `/sq` during boot without
blocking framed serial command transport if storage is unavailable. Install-time
app directory creation, boot-time registry scanning, package-resource lookup
paths, diagnostics, resources, app lifecycle orchestration, and Wi-Fi service
records are connected through the current Zephyr command and VM host surface.

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

The Zephyr build generates `squidscript_target_defaults.h` from
`SQUID_ZEPHYR_TARGET_JSON`. The ESP32-C3 Super Mini wrappers default that
variable to `targets/esp32c3-super-mini.target.json`. The generated header is
used for SquidScript-facing target defaults such as `indicator.default` and
for the GPIO-capability mask used by device-binding validation. The generator
also validates those defaults against `SQUID_ZEPHYR_TARGET_OVERLAY` so the
target JSON indicator GPIO, polarity, and PWM frequency cannot silently drift
from the Zephyr overlay. Zephyr devicetree still owns driver nodes, PWM
channels, and pinctrl setup.

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
