# Native X4 Firmware Parity Design

Status: Approved for implementation

## Purpose

Define the XTEINK X4 product behavior that native SquidScript firmware must
provide before Zephyr can be removed. Parity means replacement of supported X4
behavior, not reproduction of every Zephyr callback or support for every board
that once used Zephyr.

XTEINK X4 is the sole firmware product target in this design. XIAO ESP32-C3 and
ESP32-C3 Super Mini do not receive native ports as part of Zephyr removal.

## Required Product Surface

Native X4 firmware must provide:

- production SQBC execution and the current serial device protocol;
- an internal-flash installed-app store with registry, resources, persistent
  app state, and atomic installation;
- foreground launch/exit/return lifecycle behavior;
- armed timer and logical-input triggers;
- physical X4 buttons, target-configured gesture classification, and bounded
  event delivery;
- planned deep sleep with timer and POWER wake, lifecycle restoration, and
  accurate start reasons;
- current display, SD content, BinBook, Wi-Fi, BLE, HTTP upload, and serial
  transfer behavior;
- simultaneous Wi-Fi/BLE operation and reliable BLE terminal upload status;
- serial delivery of firmware images into an inactive OTA slot, boot
  confirmation, and rollback;
- native diagnostics and target-aware hardware tests for all of the above.

The following do not block Zephyr removal:

- XIAO and Super Mini firmware support;
- network delivery of OTA images;
- display speed/LUT/ghosting improvements beyond current correct behavior;
- reading history, metadata caches, richer file-management APIs, or transfer
  throughput tuning;
- a native implementation of indicator APIs on X4, which declares no
  indicator;
- a native implementation of raw app GPIO on X4;
- battery/USB status APIs that are not currently part of the public language.

## Public Language Decisions

### Armed logical-input triggers

`service.input.on(eventName)` is the portable declaration for a logical input
event that may launch an armed app. It is valid only inside `app.triggers`:

```squid
app "screen-refresh"

app.triggers {
  service.input.on("key.POWER.doubleTap")
}

event.on("key.POWER.doubleTap") {
  app.exit()
}
```

Rules:

- `eventName` must be a static string shorter than the portable event-name
  limit.
- It must use `key.<logical>` with an optional `.longTap` or `.doubleTap`
  suffix.
- The app must declare a matching `event.on(eventName)` handler.
- Duplicate trigger declarations in one app are compile errors.
- Firmware rejects an armed registration when the target does not declare the
  logical key or gesture.
- At most one armed app owns a logical input event at a time. A conflicting
  `app.arm` request fails without replacing the existing owner.
- A matching armed trigger launches its owner fresh and dispatches the event.
  The previous foreground app is pushed onto the return stack and does not also
  receive the event.
- With no armed owner, the event is dispatched to the active foreground app.

`service.timer.every/after` remain the timer trigger declarations inside
`app.triggers`.

### Gesture ownership

Firmware samples physical inputs, debounces them, classifies gestures, and
emits logical events. Firmware does not attach product actions to gestures.
SquidScript apps decide whether an event refreshes a screen, requests sleep,
launches another app, exits, or has no effect.

Initial X4 logical events include the existing short key names and, for POWER:

```text
key.POWER
key.POWER.longTap
key.POWER.doubleTap
```

No firmware path automatically sleeps on `longTap` or refreshes on
`doubleTap`.

### Removed device configuration API

`device.config.load`, `device.config.set`, `device.config.rebind`, and
`device.config.save` are removed directly from the pre-1.0 language, compiler,
SQBC, VM, examples, tests, and current documentation. X4 uses target metadata
and package declarations. No compatibility aliases or migration diagnostics
are retained.

Portable `hardware.gpio.*` and `service.indicator.*` remain in the language for
future native targets. X4 reports those capabilities as unavailable.

## Target Input Metadata

Target JSON is the source of truth for X4 pins, ADC ranges, debounce, enabled
gestures, and timing. It must describe timing independently so later targets
can use different values:

```json
{
  "input": {
    "debounceMs": 5,
    "gestureTiming": {
      "longTapMs": 350,
      "doubleTapWindowMs": 350
    },
    "buttons": [
      {
        "logical": "POWER",
        "type": "gpio-button",
        "gpio": "GPIO3",
        "mode": "input_pullup",
        "activeLow": true,
        "wake": true,
        "gestures": ["longTap", "doubleTap"]
      }
    ]
  }
}
```

The six ADC-ladder buttons initially enable no gestures and emit their ordinary
events on debounced release.

POWER classification is:

1. A debounced press starts duration tracking.
2. Reaching `longTapMs` while held emits `key.POWER.longTap` once and suppresses
   the ordinary event.
3. A release before `longTapMs` becomes a pending short tap.
4. A second debounced press beginning within `doubleTapWindowMs` emits
   `key.POWER.doubleTap` immediately and suppresses the pending ordinary event.
5. If the window expires first, firmware emits `key.POWER`.

Generated Rust constants must come from target JSON. Firmware source must not
duplicate X4 ADC thresholds, GPIO numbers, logical names, or timing values.

## Event Delivery And Diagnostics

Input and timer events use an eight-entry bounded pending queue. Producers may
enqueue while the VM is busy; the lifecycle poller drains the queue only when
it can start or resume work safely.

Overflow uses drop-newest behavior so already queued temporal order remains
stable. Overflow records a retained device diagnostic. Normal development
firmware also logs:

- physical classification result;
- foreground versus armed routing;
- armed owner and event name;
- duplicate ownership rejection;
- queue insertion, drain, and overflow.

Normal routing logs are compiled out of release firmware with
`debug_assertions`. Errors and retained diagnostics remain bounded.

## Internal Flash And OTA Layout

The native X4 uses the stock ESP-IDF bootloader with this 16 MiB partition
geometry:

| Name | Type/subtype | Offset | Size |
| --- | --- | ---: | ---: |
| `nvs` | data/nvs | `0x9000` | `0x5000` |
| `otadata` | data/ota | `0xe000` | `0x2000` |
| `app0` | app/ota_0 | `0x10000` | `0x280000` |
| `app1` | app/ota_1 | `0x290000` | `0x280000` |
| `squidscript` | data/littlefs | `0x510000` | `0xae0000` |
| `coredump` | data/coredump | `0xff0000` | `0x10000` |

The equal 2.5 MiB application slots retain rollback-capable OTA and leave
10.875 MiB for installed apps and internal content. The current native image
must retain measured headroom within each slot. SquidScript uses a writable
flash filesystem rather than SPIFFS in the data region.

The intended storage stack is `esp-storage 0.9` plus `littlefs2 0.8`. A
throwaway no-allocator hardware spike must prove mount, format, read, write,
rename, remount, and interrupted-write recovery before production app-store
work begins. Failure of that spike is a blocking architecture result, not a
reason to add an ad hoc flash format.

Internal LittleFS and SD FAT are separate physical volumes behind one logical
content API. New uploads prefer SD and fall back to internal flash only when SD
is missing. Reads search both volumes, with SD winning duplicate logical names.
An upload remains pinned to one volume for its complete staging and publication
lifecycle. Content names are simple ASCII filenames up to 121 bytes; UTF-8 name
support is separate planned work.

## App Store And Lifecycle Persistence

The native app store owns:

```text
/apps/<app-id>/main.sqbc
/apps/<app-id>/resources/...
/state/<app-id>.state
/lifecycle/resume
/tmp/install-<app-id>.sqbc
/books/<content-name>
/content-tmp/<content-name>
```

Rules:

- Maximum installed apps: 8.
- Maximum app-id storage: 40 bytes including terminator; authored IDs are at
  most 39 bytes.
- Installation writes a temporary file, verifies length, integrity, app id,
  and SQBC structure, then atomically renames it into the app directory.
- Package resources are published only with the corresponding valid app.
- Boot scans valid app directories to rebuild the registry. Overflow or
  malformed entries are visible failures, not silent truncation.
- Installed `main` is the root app. With no valid installed `main`, firmware
  runs an embedded native fallback SQBC through the same VM contract.
- Explicit `state.save/reset` use per-app atomic state records.
- Temporary development runs remain RAM-backed and never write flash.
- `device storage-format` clears app, state, lifecycle, install-temp, and
  internal content data, but not firmware OTA slots or SD content.

Foreground lifecycle uses a two-entry return stack. Launch, exit/return, armed
activation, and planned wake start fresh VM sessions. Ordinary foreground key
and timer events reuse the active session.

Armed registrations are rebuilt from installed SQBC metadata after boot or
planned wake. Armed timers and logical-input triggers do not retain background
VMs.

## Planned Sleep

`service.power.sleep({ wakeAfterMs })` records a deferred lifecycle request. It
does not enter sleep while the requesting handler is still executing.

After the handler returns, firmware:

1. dispatches `power.sleep` to the active app;
2. allows bounded app cleanup and explicit state save;
3. flushes app storage and display work;
4. stops upload profiles and releases Wi-Fi/BLE services;
5. writes a CRC-protected lifecycle checkpoint containing active app, return
   stack, and armed app ids;
6. configures POWER/GPIO3 wake and optional timer wake;
7. enters ESP32-C3 deep sleep.

On wake, firmware consumes a valid checkpoint, rebuilds the registry and armed
registrations, and starts the recorded foreground app with
`system.startReason() == "wake"`. Missing, corrupt, or stale checkpoints are
discarded with diagnostics before normal root boot.

The checkpoint does not contain VM frames, foreground timers, screens, service
handles, or temporary apps.

## Serial OTA

`squidc target build` produces an OTA-compatible raw app image. Runtime OTA is
exposed through:

```text
squidc device firmware-info --port <port>
squidc device firmware-update <image.bin> --port <port>
```

The update protocol is begin/chunk/commit/status over the framed serial
transport. Firmware writes only the inactive OTA slot using bounded sector
erase/write steps and acknowledges durable progress.

Before activation, host and firmware verify:

- ESP image structure and size within `0x280000`;
- the declared byte count;
- SHA-256 of the complete written image;
- readback of written flash through the inactive partition boundary.

Firmware then selects the inactive slot, marks it pending verification, and
reboots. The new image marks itself valid only after LittleFS mount, registry
rebuild, runtime initialization, and serial protocol readiness. Failure to
reach that gate leaves rollback to the stock bootloader.

Serial is the only delivery transport in this design. Authenticated network
delivery is separate roadmap work and must include signed-image policy,
resume, inactive-slot integrity, rollback, and recovery.

## Radio Completion

One native app may enable HTTP and BLE upload transports together. The hardware
acceptance gate requires:

- X4 AP association while BLE remains discoverable and connectable;
- HTTP `HEAD` responsiveness while a BLE upload is active;
- a BLE connection remaining valid through an HTTP upload;
- successful byte-count/CRC publication for both transports;
- a real terminal BLE success/error status observable through GATT;
- service stop/reset returning radio leases and reusable memory to baseline.

If current `esp-radio`, TrouBLE, BlueZ, or btleplug behavior blocks the gate,
the owning integration must be fixed or upgraded. The gate is not weakened and
no replacement upload protocol is introduced.

## Zephyr Removal Gate

Zephyr removal may begin only after native X4 passes automated and physical
acceptance for app storage, lifecycle, all physical keys, POWER gesture
classification, armed input/timer launch, planned sleep, timer and POWER wake,
serial OTA and rollback, simultaneous Wi-Fi/BLE use, BLE terminal status,
display, BinBook, SD, and all transfer paths.

The subsequent Zephyr removal is a separate plan covering source deletion,
FFI-only glue, obsolete targets, scripts, tests, generated artifacts, build
dependencies, and documentation.
