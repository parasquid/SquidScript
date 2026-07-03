# Native X4 Firmware Migration Plan

> **For agentic workers:** implement task-by-task and keep the active todo
> tracker current. Run hardware verification after each independent firmware
> slice when the X4 is reachable.

**Goal:** Build a native `no_std`/Embassy XTEINK X4 firmware path that can
replace Zephyr after evidence gates pass, starting with reusable Wi-Fi/BLE RAM
reclaim.

**Branch:** `native-firmware-x4`

## Task 1: Radio Lifecycle Proof

- [x] Add a native firmware workspace skeleton under `firmware/native`.
- [x] Add `squidscript-fw-core` host-testable types for radio lifecycle
  snapshots, cycle summaries, pass/fail thresholds, and redacted diagnostics.
- [x] Add failing host tests for reusable reclaim threshold behavior before
  implementation.
- [x] Add the X4 firmware crate with pinned ESP Rust dependencies, allocator
  setup, Embassy executor wiring, serial diagnostic output, and a minimal radio
  lifecycle command.
- [x] Build no-radio, Wi-Fi-only, BLE-only, and Wi-Fi+BLE variants where the
  stack supports them.
- [x] Flash X4 and record five-cycle Wi-Fi and BLE init/operation/deinit
  results. Stop the migration and report evidence if reusable reclaim fails.

Current hardware result: the combined `radio-probe` image flashes and runs on
ESP32-C3/X4 hardware. BLE completed five init/drop cycles and passed the reclaim
gate. Wi-Fi completed five init/drop cycles. The cold first cycle retained 9020
bytes in the combined image and 8856 bytes in a Wi-Fi-only diagnostic image,
then cycles 2-5 returned to the warmed baseline. The accepted gate reports cold
retained RAM separately and passes when post-warmup cycles reclaim to baseline.

Follow-up investigation: a Wi-Fi-only `alloc-trace` run showed the retained
memory is stable after the first Wi-Fi drop: 16 live allocations totaling 8836
bytes, dominated by one 8272-byte allocation. The size and source scan match
`esp-radio-rtos-driver`'s lazily initialized compat timer queue, which creates a
timer task with an 8192-byte stack and documents that the timer queue cannot be
stopped. This is not an accumulating per-cycle leak, but it is still persistent
RAM retained after first radio use.

## Task 2: Native Firmware Runtime Foundation

- [ ] Add target metadata for the native X4 backend while keeping Zephyr
  metadata available as reference.
- [ ] Add `squidc target build/flash --backend native` support without changing
  the default backend until native gates pass.
- [ ] Port the serial identity and reset surfaces to the native firmware.
- [ ] Run `squidc` serial identity/reset checks against flashed hardware.

## Task 3: SQBC And Service Host Integration

- [ ] Host `squidvm-core` directly in native firmware without the Zephyr C FFI
  runtime layer.
- [ ] Infer capability demand from SQBC builtin/service usage.
- [ ] Implement service leases for Wi-Fi and BLE and release them on app abort,
  app exit, app replacement, storage format, and device reset.
- [ ] Run temp app and installed app launch checks through `squidc`.

## Task 4: BinBook Import Repair

- [ ] Replace the removed `../binbook/rust` dependency with sibling path crates
  from current `../binbook`: `binbook-core`, `binbook-decompress`,
  `binbook-storage`, `embedded-sd-storage`, `gray2-render`, `ssd1677-driver`,
  and `xteink-x4-display`.
- [ ] Remove native-firmware reliance on Zephyr-era C BinBook parser/decompress
  shims.
- [ ] Keep compiler and VM tests focused on SquidScript contracts, not BinBook
  internal source layout.
- [ ] Run BinBook-related VM/FFI host tests and current reader examples.

## Task 5: X4 Display, Storage, And Canonical Switch

- [ ] Wire X4 display and SD storage through the current reusable BinBook/display
  crates.
- [ ] Verify `service.display.*`, app resource storage, `content.binbook.list`,
  `binbook.open`, and `binbook.readPage` on hardware.
- [ ] Update firmware docs and target reference docs to describe native X4 as
  canonical only after the gates pass.
- [ ] Leave Zephyr code as reference until native serial, VM, display, storage,
  BinBook, Wi-Fi, and BLE gates are all verified.

## Verification Commands

- `cargo test -p squidscript-fw-core`
- `cargo test -p squid-device-protocol --no-default-features`
- `cargo test -p squidvm-core`
- `cargo test -p squidvm-ffi`
- `cargo run -p squidc -- target build --target xteink-x4 --backend native`
- `cargo run -p squidc -- target flash --target xteink-x4 --backend native`
