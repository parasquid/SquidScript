# Native VM Temp-Run Implementation Plan

> **For agentic workers:** implement task-by-task and keep the active todo
> tracker current. Run host tests before firmware builds, and flash/drive
> hardware before committing firmware behavior.

**Goal:** Host `squidvm-core` directly in native X4 firmware and run one
temporary SQBC app over the existing serial protocol.

**Architecture:** Add a host-testable native runtime in `squidscript-fw-core`
that owns a bounded temp-run SQBC buffer, output/trace/state diagnostics, and
direct `squidvm-core` dispatch. The X4 firmware keeps USB-JTAG framing local
and delegates protocol actions to that runtime. Installed persistence and
Wi-Fi/BLE service leases remain later Task 3 slices.

**Tech Stack:** `squid-device-protocol` no-alloc frame helpers,
`squidvm-core` no-std VM/chunk APIs, fixed buffers, ESP32-C3 USB-JTAG serial.

## Task 1: Runtime Core

- [x] Add host tests for temp-run begin/chunk/commit, output/trace/state
  responses, reset cleanup, and oversize rejection.
- [x] Implement `squidscript-fw-core::native_runtime` with fixed temp SQBC,
  VM dispatch, and diagnostic line encoding helpers.
- [x] Verify `cargo test -p squidscript-fw-core`.

## Task 2: Firmware Protocol Wiring

- [x] Route native X4 serial requests for temp-run, output, trace, state,
  resources, and lifecycle through the runtime.
- [x] Keep hello/reset behavior working and keep unsupported protocol requests
  honest with error responses.
- [x] Verify native no-radio firmware builds through
  `squidc target build --target xteink-x4 --backend native`.

## Task 3: End-To-End Hardware Check

- [x] Flash native X4 firmware with `squidc target flash --target xteink-x4
  --backend native`.
- [x] Run a small SquidScript app through the existing CLI temp-run path.
- [x] Read `device output`, `device trace`, `device state`, and
  `device resources` from hardware.
- [x] Update the migration checklist and commit only after hardware passes.
