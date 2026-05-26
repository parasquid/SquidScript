# Portable RTOS Kernel Architecture

Status: Current direction
Purpose: Define the Zephyr-backed firmware runtime model for SquidScript.

## Goals

- Make Zephyr the real firmware scheduler, driver, storage, networking, and
  diagnostics host.
- Keep SquidScript app lifecycle, VM dispatch, service contracts, timers, and
  diagnostics behind a Squid-owned runtime boundary.
- Keep RTOS concepts out of SquidScript source, compiler core, SQBC, and
  portable app APIs.
- Require non-blocking service behavior: no service should monopolize the main
  loop, starve serial input, delay timers, or hide long busy waits.
- Use bounded memory for queues, service state, transfer buffers, and resource
  pools.

## Architecture

```text
SquidScript VM semantics in Rust (`squidvm-core`)
        |
C ABI/staticlib boundary (`squidvm-ffi`)
        |
Zephyr Squid host: app lifecycle, services, command surface, diagnostics
        |
Zephyr kernel subsystems: timers, work queues, message queues, shell/logging,
flash-map, NVS, LittleFS, networking, Wi-Fi management, GPIO/PWM
        |
Target HAL and board-specific devicetree/Kconfig
```

The Squid host owns portable runtime behavior:

- app install, launch, temp run, exit, app stack, and app list policy
- key and generic event dispatch
- output, draw log, resources, errors, reset, and diagnostics
- service routing and result conversion
- storage format, SQBC byte reads, app state, and package resources
- fairness rules across VM, serial, timers, display, storage, radio, and HTTP

The current Zephyr build links `squidvm-ffi` as a Rust static library and
tests the C-header link path through native simulator ztests. The linked ABI
includes context allocation metadata, init, blocking dispatch, and resumable
dispatch entry points that report pending SQBC reads and state load/save/reset
requests through C structs. The Zephyr host layer includes a bounded storage
adapter that completes those requests through backend callbacks. Native Zephyr
ztests now run a real SQBC fixture through the linked Rust VM and complete its
storage flow through that adapter. A file-backed backend now uses Zephyr
`fs_*` APIs for SQBC byte-range reads and app-state load/save/reset paths, and
native ztests cover it through a host-mounted filesystem. The app-store layer
derives bounded VM storage paths from a mount point and app ID, prepares the
top-level app/state directories, and ESP32-C3 firmware attempts to mount the
target LittleFS `storage_partition` at `/sq` during boot. Installed-app
registry scanning, install-time directory creation, package-resource lookup
paths, service records, diagnostics, and lifecycle callbacks are connected
through the current Zephyr runtime boundary.

Zephyr owns backend integration:

- threads, work queues, timers, and synchronization
- shell or dedicated serial command ownership
- flash partitions, NVS records, and LittleFS volumes
- GPIO, PWM, display, input, radio, networking, and power drivers
- target-specific Kconfig/devicetree and hardware errata

## Service Model

Services are bounded actors or equivalent Zephyr-native state machines. A
SquidScript service call may enqueue a command, read cached state, or return an
immediate bounded error. It must not run radio loops, wait for display busy,
poll flash until complete, delay for animation, or block serial input.

Use Zephyr timers, work queues, message queues, and subsystem event callbacks
for progress. When a queue or pool is full, return a bounded service error or
use a documented coalesce/drop policy.

## Firmware Command Surface

The Zephyr firmware command surface is the current device protocol. Host tools
such as `squidc run`, `squidc app`, `squidc repl`, and `squidc device` should
speak that protocol directly as it is implemented. Do not preserve old command
names or response shapes unless they are also the current Zephyr protocol.

## Storage Model

Model logical app storage separately from physical volumes:

- SQBC app bytes and package resources use Zephyr flash-map plus LittleFS where
  a file layout is needed.
- App state uses NVS or LittleFS records, selected by implementation tests.
- Compatibility with old Rust firmware storage is not supported before 1.0.

## Browser Simulator

The browser simulator shares the same SquidScript service contract but remains
a separate TypeScript/WASM host. Browser IR JSON is a development artifact, not
a Zephyr firmware format.
