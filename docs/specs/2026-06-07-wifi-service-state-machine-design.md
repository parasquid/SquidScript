# Wi-Fi Service State Machine Design

## Scope

SquidScript Wi-Fi firmware exposes one foreground-owned service with one
foreground operation at a time. The operation record reports command progress;
`service.wifi.status().state` reports the service lifecycle. These are separate
surfaces and must not be conflated.

This design covers the Zephyr runtime's scan, station connect/disconnect, and
AP start/stop lifecycle. It does not add new SquidScript syntax, credentials,
AP security options, station profile storage, HTTP, captive portal, or cursor
pagination behavior.

## Runtime States

The runtime tracks an internal Wi-Fi service state:

- `idle`
- `scanning`
- `connecting`
- `connected`
- `disconnecting`
- `apStarting`
- `apStarted`
- `apStopping`
- `error`

The public `service.wifi.status().state` field maps those internal states to
the portable service set:

- `idle`
- `configuring`
- `starting`
- `started`
- `stopping`
- `error`

Targets without Wi-Fi support keep reporting `unavailable` or `stopped` through
their existing unsupported path. Firmware must not expose raw Zephyr Wi-Fi
driver state strings as the public service state.

## Operation Rules

`service.wifi.startAP`, `stopAP`, `connect`, `disconnect`, and `scan` begin the
single foreground Wi-Fi operation and set the service state before calling the
driver:

- `scan` enters `scanning`; success returns the service to `idle`; failure or
  timeout enters `error`.
- `connect` enters `connecting`; success enters `connected`; failure or
  timeout enters `error`.
- `disconnect` enters `disconnecting`; success enters `idle`; failure or
  timeout enters `error`.
- `startAP` enters `apStarting`; success enters `apStarted`; failure enters
  `error`.
- `stopAP` enters `apStopping`; success enters `idle`; failure enters `error`.

Starting a second foreground Wi-Fi operation while one is active and unfinished
returns `error == "wifi busy"` without replacing the current service state.
Scan cannot interrupt active AP or station radio state.

The operation record keeps its current command-facing states: `idle`,
`running`, `done`, `cancelled`, or `error`. A completed command can therefore
coexist with a different service state; for example, a successful
`service.wifi.connect("dev")` has an operation result of `done` and a service
status of `started`.

## Implementation Notes

The generic service-state and operation bookkeeping lives in `struct
sq_vm_runtime` outside the Zephyr Wi-Fi management compile guard so native ztests
can exercise the state helpers without a radio driver. Driver-specific scan,
station, AP event, and IP scratch state remains behind the Wi-Fi management
guard.

The state helpers are:

- `sq_vm_runtime_wifi_service_begin`
- `sq_vm_runtime_wifi_service_finish`
- `sq_vm_runtime_wifi_service_cancel`
- `sq_vm_runtime_wifi_service_busy`
- `sq_vm_runtime_wifi_service_state_text`

Zephyr event callbacks may confirm AP enable/disable state and update AP client
counters, but app-facing behavior should not require exposing raw event codes
or raw driver state names.

## Acceptance Checks

- Native protocol ztests cover the service helper transitions in a no-radio
  build.
- Hardware Wi-Fi scan/list/AP/station checks exercise the state machine through
  real Zephyr driver calls on an attached target when hardware is available.
- Documentation describes `operation().state` and `status().state` as distinct
  surfaces.
