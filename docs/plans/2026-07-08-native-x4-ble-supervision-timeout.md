# Native X4 BLE Supervision Timeout

## Goal

Eliminate controller reason `0x08` during active native X4 BLE file transfers
without weakening the bounded producer/consumer pipeline or serial/runtime
responsiveness.

## Tasks

- [x] Enable and handle Trouble Host connection-parameter update requests.
- [x] Add debug-build diagnostics for request receipt and response outcome.
- [x] Build, flash, and test the isolated request-response fix with 26 bytes. Reason `0x08` remained and no connection-parameter request was reported.
- [x] Test Trouble Host 0.6 Connecting-state ATT handling. Reason `0x08` remained, so the local vendor patch is not retained.
- [x] Test callback-driven esp-radio HCI wakeups. They did not improve the link and are not retained.
- [x] Reproduce against the official Trouble Host 0.7/current esp-radio minimal ESP32-C3 peripheral. Service discovery disconnects on the same X4 and host after a cold daemon/controller reset, isolating the remaining blocker below SquidScript.
- [x] Verify 26-, 1,024-, and 8,982-byte transfers and stored CRCs.
- [ ] Repeat the 8,982-byte transfer three times from clean sessions.
- [x] Verify interrupted-transfer cleanup, reconnect, watchdog recovery, memory budgets, and runtime responsiveness.
- [x] Run automated debug/release checks and update current-state documentation.

## Acceptance Gates

- Every connection-parameter request is accepted or rejected explicitly.
- No active transfer ends with controller reason `0x08`.
- BLE queue depth remains at or below four fixed chunks; no whole-payload RAM buffer is introduced.
- Stored size and CRC match for all required payload sizes.
- A killed client leaves no staged partial file and advertising recovers within 35 seconds.
- Debug instrumentation remains enabled in normal builds and compiles out in release builds.

## Current Status

The required transfer sizes, interrupted-transfer cleanup, watchdog recovery,
memory budget, runtime responsiveness, and readvertising gates pass on the
native X4 firmware. The remaining stability gate is three consecutive
8,982-byte transfers from clean sessions.
