# Native X4 Canonical Parity Execution Plan

> Keep this plan as the active checklist for the native XTEINK X4 parity slice.
> Detailed command transcripts and temporary evidence belong in `.current_agent_work`
> or `/tmp`, not in this tracked plan.

**Goal:** Make the Rust-only native X4 firmware the canonical SquidScript
firmware path after real hardware evidence proves VM/app execution, display,
input, storage/content, Wi-Fi, BLE file transfer, and resource cleanup.

## Acceptance Gates

- Native X4 builds and flashes through `squidc target build/flash --backend native`.
- Wi-Fi AP, station, scan, AP IP/status, and cleanup return real hardware state,
  not placeholders.
- BLE file transfer exposes the Zephyr-compatible GATT service and accepts
  uploads through the existing `device ble-put` CLI path.
- Native radio resources are acquired by active app demand and released after
  app exit, app replacement, reset, storage format, and runtime errors.
- Full BinBook reader flow runs on XTEINK with fresh webcam evidence before
  target docs describe native X4 as canonical.

## Tasks

- [ ] Add host tests for Wi-Fi scan/status/AP-IP and BLE profile lifecycle
  behavior using fake native radio backends.
- [ ] Extend native radio backend interfaces so `service.wifi.*` can expose
  scan results, STA connection details, AP IP/status, RSSI, BSSID, auth, and
  backend errors.
- [ ] Wire X4 Wi-Fi to real `esp-radio` operations: config/start, scan,
  station connect, channel, RSSI, AP info, and connection state.
- [x] Spike `trouble-host` over `esp-radio::ble::BleConnector`; `trouble-host`
  0.6.0 plus `bt_hci::controller::ExternalController<BleConnector, 4>`
  type-checks against the ESP HCI transport.
- [ ] Add a native async BLE runner integration point before GATT work:
  TrouBLE requires a continuously polled host runner plus GATT event loop, while
  the current native firmware loop is synchronous serial/VM polling with only
  one-shot `block_on` calls.
- [ ] After the async BLE runner exists, implement the Zephyr-compatible custom
  file-transfer service and route completion events to installed SquidScript
  receivers.
- [ ] Verify native X4 on hardware for serial identity/resources, Wi-Fi AP,
  Wi-Fi station, Wi-Fi scan, BLE upload, storage/content integrity, display,
  and BinBook reader webcam evidence.
- [ ] Update target metadata and docs only after the hardware gates pass.

## Verification Commands

- `cargo test -p squidscript-fw-core`
- `cargo test -p squidscript-fw-x4 --features x4-binbook`
- `cargo test -p squid-device-protocol`
- `cargo test -p squidc`
- `cargo test -p squidvm-core`
- `cargo test -p squidvm-ffi`
- `cargo run -p squidc -- target build --target xteink-x4 --backend native`
- `cargo run -p squidc -- target flash --target xteink-x4 --backend native`

## Assumptions

- Canonical X4 parity is the X4 user-facing firmware path, not every Zephyr
  surface.
- Station verification uses environment-provided credentials and redacted
  evidence.
- No pre-1.0 compatibility bridges are added unless explicitly requested.
