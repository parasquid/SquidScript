# Native X4 Firmware Design

## Purpose

SquidScript's XTEINK X4 firmware moves from Zephyr to a native `no_std` Rust
firmware built around Embassy-style async tasks and explicit service ownership.
The first reason for the migration is RAM: Wi-Fi and BLE must be available on
targets that have radios, but their RAM must be claimed only while the
corresponding SquidScript service is active and released afterwards.

Zephyr remains reference code during the migration. The native X4 path becomes
canonical only after it passes the radio lifecycle, serial protocol, VM,
display, storage, and BinBook gates described below.

## Architecture

Native firmware is split into reusable firmware/runtime crates and a board
crate:

- `squidscript-fw-core`: `no_std` service traits, capability demand analysis,
  VM host glue, diagnostic records, and protocol-facing types.
- `squidscript-fw-x4`: ESP32-C3/XTEINK X4 Embassy firmware, board pins,
  allocator setup, task wiring, radio owners, serial diagnostics, display,
  input, and SD integration.

The native runtime uses existing SquidScript service calls and SQBC builtin IDs
as the source of capability demand. No new `use service.wifi` or capability
declaration syntax is introduced for this migration. If an installed app has no
`service.wifi.*` or `service.ble.*` calls, the runtime must not keep Wi-Fi or BLE
RAM resident on that app's behalf.

Each scarce service is represented by a service owner and a short-lived lease:

- Wi-Fi lease: initializes the ESP32-C3 Wi-Fi stack on first Wi-Fi operation,
  performs the requested station/AP/scan operation, and deinitializes when the
  operation and app-owned service state are complete.
- BLE lease: initializes BLE for foreground profiles such as file transfer,
  advertises while the profile is active, and deinitializes when the profile is
  stopped or the owning app exits.
- Failure cleanup: app abort, app replacement, storage format, and device
  protocol reset release all active leases before the runtime accepts more app
  work.

## Radio Stack

The native branch targets the current Rust ESP stack first: `esp-radio`
`1.0.0-beta.0` with the matching `esp-hal ~1.1` generation. The stack choice is
evidence-gated because the Espressif Rust HAL and radio crates are unstable.
`esp-radio` requires Rust `1.95.0` and the local stable toolchain satisfies that
requirement. The first firmware slice must measure whether Wi-Fi and BLE can be
initialized, stopped, deinitialized, and initialized again without reboot.

The current `esp-radio` Wi-Fi seam is RAII-based: `WifiController::new(...)`
initializes Wi-Fi and `WifiController::drop` stops and deinitializes it. The
crate also keeps a global radio reference guard that deinitializes the common
radio layer when the final radio user is dropped. Native SquidScript services
should model leases around those ownership boundaries rather than keeping a
global controller alive for the boot lifetime.

The reusable dynamic-service gate is stricter than one-way memory release. A
BT-controller style permanent memory release is useful evidence, but it does
not satisfy SquidScript's runtime requirement because an app may later request
the same radio service again.

The radio lifecycle diagnostic logs:

- free heap bytes,
- largest free block when available,
- radio state,
- operation status,
- cycle count,
- named failure code.

Human-facing diagnostics must redact environment-identifying radio data such as
SSIDs, BSSIDs, MAC addresses, credentials, and local IPs unless raw identifiers
are explicitly requested.

## BinBook And Display

Native firmware consumes current BinBook sibling crates directly instead of the
old Zephyr-era C/Rust parser shim:

- `binbook-core` for validation, metadata, and page index parsing.
- `binbook-decompress` for allocation-free page decompression.
- `gray2-render` for GRAY2 plane conversion and dithering.
- `ssd1677-driver` for SSD1677 command sequencing.
- `xteink-x4-display` for the X4 display pipeline.
- `binbook-storage` and `embedded-sd-storage` for SD-backed document access.

The board crate owns X4 pins, ADC button mapping, SPI device sharing, task
priorities, and storage mounting. The reusable BinBook and display crates must
not depend on SquidScript app state, target JSON, serial protocol, Wi-Fi, BLE,
or board-specific aliases.

## Acceptance Gates

Native X4 firmware is not canonical until all gates pass:

- Radio lifecycle: Wi-Fi and BLE each initialize, perform a minimal operation,
  stop, deinitialize, and repeat for five cycles. The first cycle may establish
  a reported warmed baseline for one-time shared radio/RTOS support allocations.
  Later cycles must reclaim heap to within `max(4 KiB, 10%)` of their measured
  pre-init service delta, with no monotonic largest-free-block loss across
  cycles. Cold retained RAM remains a diagnostic value and must not grow across
  repeated service use.
- Serial protocol: identity, app install, temp run, launch, output, and reset
  interoperate with `squidc`.
- VM host: current SQBC runs through `squidvm-core` without a Zephyr FFI shim.
- Display: `service.display.*` renders on X4 and remains responsive while
  serial input is active.
- Storage: SD-backed app and resource storage supports installed apps and
  content references.
- BinBook: the reader opens, lists, and displays current `.binbook` files
  through direct Rust crate integration.
- Services: Wi-Fi and BLE service calls activate/deactivate through leases and
  leave non-radio apps with radio RAM unclaimed.
