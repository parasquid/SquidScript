# Native X4 Firmware Migration Plan

> Implement task-by-task and keep the active todo tracker current. Run hardware
> verification after each independent firmware slice when the XTEINK X4 is
> reachable. Detailed session notes, command transcripts, and intermediate
> measurements belong in `.current_agent_work`, not in this tracked plan.

**Goal:** Build a native `no_std`/Embassy XTEINK X4 firmware path that can
replace Zephyr after evidence gates pass, with lower RAM use and explicit
Wi-Fi/BLE lifecycle ownership.

**Branch:** `native-firmware-x4`

## Acceptance Gates

- Native X4 builds and flashes through `squidc target build/flash --backend native`.
- Native firmware hosts `squidvm-core` directly, without the Zephyr C runtime
  layer, for temp and installed apps.
- Wi-Fi and BLE service calls are app-driven, dynamically acquire/release native
  radio resources, and release on app replacement/reset/exit/error.
- Display calls render on the physical XTEINK panel without retaining a
  persistent full framebuffer in the radio-enabled image.
- Native firmware exposes SquidScript-owned file/storage services backed by X4
  SD, with BinBook/content services layered on top rather than defining the
  storage architecture.
- RAM is reported against total X4 RAM, with heap-pool details separated from
  total-RAM percentages.
- Zephyr remains reference-only until native serial, VM, display, storage,
  BinBook, Wi-Fi, and BLE gates are verified.

## Task 1: Radio Lifecycle Proof

- [x] Add a native firmware workspace under `firmware/native`.
- [x] Add host-testable radio lifecycle snapshots, reclaim gates, summaries,
  and redacted diagnostics.
- [x] Build no-radio, Wi-Fi-only, BLE-only, and Wi-Fi+BLE variants where the
  stack supports them.
- [x] Flash XTEINK X4 and record reusable Wi-Fi/BLE init/drop behavior.
- [x] Accept stable one-time Wi-Fi retained RAM after investigation showed it is
  the esp-radio RTOS timer queue, not an accumulating leak.

## Task 2: Native Firmware Runtime Foundation

- [x] Add native X4 target metadata while preserving Zephyr metadata.
- [x] Add explicit `--backend native` build/flash support with Zephyr still
  available as reference.
- [x] Port serial identity, reset, resources, output, trace, state, lifecycle,
  app install, app launch, app list, key, event dispatch, and state import.
- [x] Verify native serial identity/reset and installed-app lifecycle on XTEINK.

## Task 3: SQBC And Service Host Integration

- [x] Host `squidvm-core` directly in native firmware.
- [x] Run temp app and installed app launch checks through `squidc`.
- [x] Implement native Wi-Fi/BLE service lease model.
- [x] Wire VM service calls to the real ESP radio backend for handle ownership.
- [x] Infer capability demand from SQBC builtin/service usage.
- [x] Finish service cleanup on every app abort, app exit, storage format, and
  runtime error path.
- [ ] Implement actual Wi-Fi station/scan/AP details and BLE profile behavior
  beyond handle lifecycle.

## Task 4: BinBook Import Repair

- [x] Replace the removed `../binbook/rust` dependency with current sibling
  path crates needed for BinBook rendering/reference use: `binbook-core`,
  `binbook-decompress`, `embedded-sd-storage`, `gray2-render`,
  `ssd1677-driver`, and `xteink-x4-display`.
- [x] Remove native-firmware reliance on Zephyr-era C BinBook parser/decompress
  shims.
- [x] Keep compiler and VM tests focused on SquidScript contracts rather than
  BinBook internal source layout.
- [x] Run BinBook-related VM/FFI host tests and current reader example builds.

## Task 5: X4 Display, Storage, And Canonical Switch

- [x] Add native display drawlog and `display.info` support.
- [x] Add screen-render completion plumbing from `squidvm-core`.
- [x] Add X4 display sinks that avoid persistent framebuffer retention in the
  radio-enabled image.
- [x] Add cooperative/streaming SSD1677 flush path and verify a visible XTEINK
  panel update while Wi-Fi/BLE leases remain active.
- [x] Add injectable native BinBook/content backend boundary with unsupported
  default.
- [x] Add injectable native file backend boundary for existing `file.*` VM
  calls with unsupported default behavior on unbound targets.
- [x] Remove native X4 dependence on BinBook storage/listing traits for the
  general firmware storage boundary.
- [x] Add bounded SquidScript-owned read buffers and streaming line result
  materialization for native `file.readText`/`file.readLines`.
- [x] Wire X4 SD read storage through SquidScript-owned filesystem traits backed
  by `embedded-sd-storage` on the shared SPI bus.
- [x] Add bounded `file.pickFile(extension)` support over generic storage
  enumeration, returning firmware-owned refs such as `books/<name>`.
- [x] Add bounded same-storage `file.copy(source, { library, name })` over
  generic file storage, returning destination refs such as `books/<name>`.
- [x] Layer `content.binbook.list("books")` over generic file enumeration,
  filtering `.binbook` refs without making BinBook storage the firmware
  filesystem model.
- [x] Layer `binbook.open` and `binbook.info` over the native file backend with
  bounded path handles and transient `binbook-core` opens; the X4 backend
  stores file refs, not persistent `Book` objects or BinBook-owned storage.
- [x] Add serial `device content-put` support in native firmware over the
  SquidScript-owned generic file backend, publishing simple names as
  `books/<name>` through bounded begin/chunk/commit writes.
- [x] Transfer a current-format BinBook onto XTEINK SD storage and verify
  `binbook.open` plus `binbook.info` succeeds from the uploaded
  `books/<name>` ref on hardware.
- [x] Add generic directory/list result buffers for dynamic SD-backed file names
  and refs beyond the existing picker API.
- [x] Implement `binbook.readPage` and drawable display handoff against
  SD-backed content through SquidScript-owned file refs and drawable handles.
- [x] Implement `binbook.chapters` and `binbook.chapter` against SD/package
  content.
- [x] Add content check/delete support in the native firmware for
  already-published files.
- [ ] Verify full BinBook reader hardware flow on XTEINK with webcam evidence.
- [ ] Update firmware docs and target reference docs to describe native X4 as
  canonical only after all gates pass.

## Current RAM Evidence

- Native VM plus Wi-Fi/BLE active and command-display state has been measured on
  XTEINK at roughly 128 KiB known measured RAM use out of 409,600 bytes total
  RAM, with the 102,400-byte radio heap pool reported separately.
- Native VM plus Wi-Fi/BLE-capable command-display and SD file-read backend
  currently reports 49,104 bytes known static/allocated use at idle out of
  409,600 bytes total RAM, with the 102,400-byte radio heap pool free and
  reported separately.
- Adding SD-backed `file.pickFile` keeps the current XTEINK native idle report
  at 49,104 bytes known static/allocated use out of 409,600 bytes total RAM,
  with the 102,400-byte radio heap pool free and reported separately.
- Adding bounded SD-backed `file.copy` keeps the current XTEINK native report
  at 49,104 bytes known static/allocated use out of 409,600 bytes total RAM,
  with the 102,400-byte radio heap pool free and reported separately.
- Adding SD-backed `content.binbook.list("books")` keeps the current XTEINK
  native report at 49,104 bytes known static/allocated use out of 409,600 bytes
  total RAM, with the 102,400-byte radio heap pool free and reported
  separately. Hardware smoke currently sees 15 `.binbook` files on the SD card.
- Adding bounded file-backed `binbook.open`/`binbook.info` state raises the
  current XTEINK native reset report to 52,384 bytes known static/allocated use
  out of 409,600 bytes total RAM, with the 102,400-byte radio heap pool still
  free.
- Adding serial content publish support keeps the current XTEINK native reset
  report at 52,384 bytes known static/allocated use out of 409,600 bytes total
  RAM, with the 102,400-byte radio heap pool still free.
- XTEINK hardware returns generic `books/<name>` refs from
  `content.binbook.list`, and `binbook.open` handles those refs through the
  native file backend. A current-format BinBook fixture uploaded through
  `device content-put` opens successfully and reports `pageCount=1` and
  `chapterCount=0` from `binbook.info`.
- Native X4 `binbook.readPage` resolves an SD-backed `books/<name>` ref to a
  drawable handle, and `service.display.draw(page.drawable)` can render that
  drawable to the physical XTEINK panel using flush-scoped buffers. The current
  reset report is 56,040 bytes known static/allocated use out of 409,600 bytes
  total RAM, with the 102,400-byte radio heap pool still free. This known total
  includes the runtime, serial buffers, and display flush task scratch.
- Native X4 content check/delete resolves simple uploaded names through the
  SquidScript-owned `books/<name>` storage boundary. Hardware verification
  confirms a 40-byte uploaded content file reports the expected size and CRC,
  deletes successfully, and returns `not-found` on the same check after
  deletion. The current post-delete resource report remains 56,040 bytes known
  static/allocated use out of 409,600 bytes total RAM, with the 102,400-byte
  radio heap pool still free.
- Native X4 `binbook.chapters` streams chapter entries from an SD-backed
  `books/<name>` ref through caller-owned result materialization, and
  `binbook.chapter` reads a single chapter with bounded title storage. Hardware
  verification confirms an uploaded current-format two-chapter BinBook reports
  `pageCount=2`, `chapterCount=2`, chapter titles `Opening` and `Second`, and
  the current resource report remains 56,040 bytes known static/allocated use
  out of 409,600 bytes total RAM, with the 102,400-byte radio heap pool still
  free.
- Native X4 `file.list("books", { offset, limit })` lists generic SD-backed
  file refs through the SquidScript-owned file backend. Hardware verification
  confirms a full 8-entry page runs without VM record-arena wraparound, and a
  non-BinBook uploaded proof file appears as `books/000-native-list-proof.txt`
  with size 24. The current resource report is 56,352 bytes known
  static/allocated use out of 409,600 bytes total RAM, with the 102,400-byte
  radio heap pool still free.
- Native X4 resource reporting includes SQBC-derived active-app demand bits for
  Wi-Fi, BLE, HTTP, display, storage, and BinBook. Hardware verification
  confirms an app can report `demand_wifi=1` and `demand_ble=1` while
  `radio_wifi_active=0`, `radio_ble_active=0`, and `radio_active_leases=0`,
  separating static app capability demand from actual radio acquisition. The
  current resource report is 56,360 bytes known static/allocated use out of
  409,600 bytes total RAM, with the 102,400-byte radio heap pool still free.
- Native X4 BLE profile lifecycle state is tracked separately from raw BLE
  radio leases. Hardware verification confirms `service.ble.start` reports
  `radio_ble_active=1`, `ble_profile_active=1`, and `ble_profile_id_len=2`.
  The active profile report is 92,480 bytes known used out of 409,600 bytes
  total RAM; after `service.ble.stop`, `radio_ble_active=0`,
  `ble_profile_active=0`, and known used RAM returns to 65,404 bytes. The
  102,400-byte radio heap pool remains reported separately.
- A persistent 96,000-byte framebuffer inside the radio-enabled runtime is not
  viable with the current heap/static layout. Display work must stay
  command-retained, flush-scoped, or streaming.

## Verification Commands

- `cargo test -p squidscript-fw-core`
- `cargo test -p squidscript-fw-x4 --features x4-binbook`
- `cargo test -p squid-device-protocol --no-default-features`
- `cargo test -p squid-device-protocol`
- `cargo test -p squidc`
- `cargo test -p squidvm-core`
- `cargo test -p squidvm-ffi`
- `RUSTUP_TOOLCHAIN=nightly cargo fmt --all --check` from `firmware/native`
- `cargo run -p squidc -- target build --target xteink-x4 --backend native`
- `cargo run -p squidc -- target flash --target xteink-x4 --backend native`
- `cargo run -p squidc -- device content-put --port <port>
  <current-format.binbook> --name <safe-name.binbook>`
- From `firmware/native`:
  `RUSTUP_TOOLCHAIN=nightly cargo build -Zbuild-std=core,alloc -p squidscript-fw-x4 --features firmware-bin,x4-binbook,native-radio-services --target riscv32imc-unknown-none-elf --release`
