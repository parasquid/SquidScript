# BLE HID Input Source Design

## Goal

Add a design path for BLE HID input sources that keeps apps portable, keeps BLE
protocol details out of SquidScript app code, and preserves the energy
principle that radio work should run only when a foreground workflow needs it.
Page turners and keyboards are both BLE HID input sources; page turning is a
reader use case, not the core input type.

This design is documentation-only. The binding examples below are proposed
syntax and are not implemented by the compiler, VM, or firmware yet.

## Namespace Split

`service.ble.*` and `device.ble.*` intentionally describe different ownership
models.

`service.ble.*` is for app-owned runtime BLE services. The foreground app starts
and stops the service, owns the configured event routing, and receives the
resulting app events. `service.ble.start("file-transfer", ...)` is the model
case.

`device.ble.*` is for device-level BLE management. It owns durable
pairing/bonding state, trusted-device records, pairing status, and pairing
cancellation for firmware-supported BLE roles. It is not raw BLE access, does
not expose arbitrary GATT operations, and does not make app code responsible for
HID report parsing or Bluetooth controller policy.

This mirrors the broader SquidScript boundary: `service.*` calls use active
runtime services, while `device.*` calls manage device configuration or
device-level state.

## Rationale

BLE file transfer and BLE HID input have different ownership models.

`service.ble.start("file-transfer", ...)` is an app-owned foreground receive
service. The SquidScript device acts as a BLE peripheral, advertises a transfer
service, receives uploaded objects, and dispatches the configured completion
event to the app that started the service. Foreground ownership is the right
energy model because the device should not advertise an installer or transfer
endpoint when no active workflow needs it.

A BLE HID page turner, keyboard, or similar controller is normally a BLE HID
peripheral. The SquidScript device acts as a BLE central/client: it scans for or
reconnects to a bonded device, subscribes to HID reports, parses those reports,
and turns them into input. Pairing, bonding, reconnect, HID parsing, and HID
usage mapping are device-level concerns. Apps should receive logical key events,
the same way they receive events from GPIO buttons or other target input
sources.

The energy model remains foreground-gated. The system owns durable pairing
state, but firmware should enable BLE HID scanning and connection maintenance
only while a foreground app declares matching input demand, or while a system
pairing/settings flow is active.

## App-Facing Model

Reader apps handle ordinary logical keys:

```squid
event.on("key.RIGHT") {
  // Next page.
}

event.on("key.LEFT") {
  // Previous page.
}
```

The source of those keys is not app-visible. The same handler should work for a
physical button, simulator key, BLE HID page turner, BLE HID keyboard, or a
future input device.

Foreground app demand is expressed declaratively through an input binding. The
initial inline shape can be:

```squid
device {
  input {
    use "ble-hid:key.LEFT,key.RIGHT"
  }
}
```

This declaration means the app can use a BLE HID input source that produces the
listed logical keys. Runtime applies input bindings before `event.on("app.start")`,
so firmware can enable the BLE HID input service before the app begins waiting
for key events.

A richer `.sqdevice` form may replace or complement the inline string when the
binding needs device identity, pairing profile, allowed usage pages, reconnect
policy, or diagnostic mode.

## Pairing Workflow

Pairing is a device capability, not a reader-app lifecycle concern. The
declarative input binding says an app wants BLE HID input from an already paired
or pairable source; it should not itself open-ended scan, bond, or expose raw
BLE details.

A foreground settings or diagnostic app still needs a SquidScript-visible way to
request the system pairing workflow. That operation should live in a future
`device.ble.*` family because it manages durable BLE pairing/bonding state, not
an app-owned BLE service and not generic input configuration. This keeps normal
input bindings transport-neutral while avoiding the implication that ESP32-C3
supports Bluetooth Classic.

Proposed shape:

```squid
event.on("app.start") {
  let started = device.ble.pair("hid", {
    binding: "input.default",
    keys: ["LEFT", "RIGHT", "SELECT", "BACK"],
    timeoutMs: 30000
  })
  debug.print("pair start", started.ok, started.error)
  service.timer.every("pair.status", 500)
}

event.on("pair.status") {
  let status = device.ble.pairing("input.default")
  debug.print("pair status", status.state, status.error)
  if (status.state == "paired") {
    device.ble.cancelPairing("input.default")
  }
}
```

`device.ble.pair(kind, config)` starts a bounded foreground pairing operation.
For BLE HID, `kind` is `"hid"`. The config names the input binding being
paired, the desired logical keys, and a timeout. The operation should return a
small result record with `ok`, `error`, and `state`.

`device.ble.pairing(binding)` reports the current pairing operation state for
display and serial diagnostics. Suggested public states are `idle`, `scanning`,
`connecting`, `paired`, `timeout`, `cancelled`, `error`, and `unsupported`.

`device.ble.cancelPairing(binding)` stops a pairing operation and closes the
pairing scan window. Cancelling pairing should not remove an existing bond.

Successful pairing stores system-owned bond/configuration state for the target
binding. Firmware should persist a bounded list of trusted BLE HID devices, not
only the most recent connection. Each record should contain the minimum data
needed to recognize and reconnect to the device later, plus the logical-key
mapping/profile it can satisfy. The exact BLE identity and bonding material are
firmware-private; app-visible records should not expose raw addresses or keys.

When a foreground app declares BLE HID input demand, firmware checks the
persisted trusted-device list for a compatible record. If one exists, firmware
may scan for a bounded window and reconnect to that known device when it is
seen, without asking the user to pair again. If no compatible trusted device is
available, the app still launches; a settings or diagnostic app can explicitly
open the pairing workflow.

The paired device can later satisfy ordinary foreground input demand from any
app whose active input binding accepts the mapped logical keys.

Pairing operations must be bounded and foreground-owned:

- No indefinite scanning.
- No raw BLE MACs, keys, or HID reports in app-visible records.
- Persisted trusted-device records are firmware-owned device config, not app
  state.
- If a device name is exposed for UI, it should be a display label only, not a
  stable identifier for app logic.
- Only one pairing operation per target binding should run at a time in the
  first implementation.
- Normal reader apps should declare input demand and handle `key.*`; settings
  or diagnostic apps should own pairing UI.

## System Behavior

The system input service owns BLE HID state:

- Pairing and bonding are system-owned and durable across app launches.
- Firmware persists a bounded trusted-device list so compatible BLE HID devices
  can reconnect when seen, without repeating the pairing flow.
- Pairing can be requested by a foreground settings or diagnostic app through
  the future `device.ble.*` operation family.
- Reconnect policy is system-owned and target-dependent.
- HID reports are parsed by firmware, not SquidScript apps.
- HID usages are mapped to the standard logical key namespace.
- App-visible dispatch uses exact event names such as `key.LEFT` and
  `key.RIGHT`.
- Scanning and connection maintenance are active only while at least one
  foreground/system consumer has matching BLE HID input demand.
- When the foreground app exits or loses foreground and no other consumer
  remains, firmware should stop scanning and may disconnect according to target
  policy.

This is separate from app-owned BLE file transfer. An app should not call
`service.ble.start(...)` merely to use a generic BLE HID input source.

## Diagnostic App

The first proposed example and hardware test app is `ble-hid-tester`. It should
print the latest received logical key to serial output and display so a
developer can confirm pairing, reconnect, HID parsing, and event dispatch. It
may also include a pairing screen that calls the future `device.ble.pair`
operation before switching into key-display mode.

The app should enumerate the standard keys it wants to observe:

```squid
app "ble-hid-tester"

device {
  input {
    use "ble-hid:key.LEFT,key.RIGHT,key.SELECT,key.BACK"
  }
}

event.on("app.start") {
  debug.print("ble hid tester ready")
}

event.on("key.LEFT") {
  debug.print("key", "LEFT")
}

event.on("key.RIGHT") {
  debug.print("key", "RIGHT")
}

event.on("key.SELECT") {
  debug.print("key", "SELECT")
}

event.on("key.BACK") {
  debug.print("key", "BACK")
}
```

Current SquidScript dispatch is exact-match. There is no catchall key handler
such as `event.on("key.*")`. Wildcard event handlers should remain deferred
unless they become useful across the language, not only for BLE diagnostics.

Firmware diagnostics may record raw HID usage codes while developing the
driver, but ordinary SquidScript app handlers should receive only logical keys.

## Acceptance Checks

- The design keeps BLE HID input separate from app-owned BLE file transfer.
- Reader apps handle logical `key.*` events and do not parse raw HID reports.
- Pairing/bonding is durable system state, while BLE HID scanning/connection is
  gated by active input demand.
- The design persists a bounded trusted-device list and uses it for later
  reconnect when a compatible device is seen.
- Pairing is requested through a future `device.ble.*` device capability, not
  by reader apps calling `service.ble.start(...)`.
- `ble-hid-tester` is the first example/test app and enumerates standard key
  handlers.
- No catchall key handler is required for the first BLE HID input slice.
