# SquidScript Icebox

This file tracks speculative or conditional ideas that are not active roadmap
work. Move an item back to `ROADMAP.md` only when there is a concrete target,
use case, or implementation reason to make it actionable.

## Display And Output

- Consider explicit output grouping if a real target needs one logical app
  output to fan out to multiple physical endpoints. Prefer a deliberate grouping
  model over implicit multiple `use service.indicator` fan-out.

## BLE Transport

- Standard Bluetooth OTS (Object Transfer Service) + L2CAP CoC transport for app
  upload. Dropped from the active design in favor of a custom GATT service
  because the chosen upload client is a Web Bluetooth page, and the Web
  Bluetooth API is GATT-only (no L2CAP). Reconsider OTS/CoC if a future goal
  needs: (a) interop with generic standard-OTS phone clients, or (b) a native
  iOS/Android uploader app (both platforms expose L2CAP CoC: iOS CBL2CAPChannel,
  Android createInsecureL2capChannel). Note CoC is testable on Linux via BlueZ
  raw L2CAP sockets but not on Windows, and never in a browser. OTS-specific
  extensions stay parked with the transport: an OTS client/pull role,
  host-platform CoC probing, OACP Calculate Checksum, and higher
  `CONFIG_BT_MAX_CONN` for multiple OTS clients are useful only if OTS returns
  as an active transport. The transport-neutral core (`ble_object_transfer.c`:
  staging session, object-name parser, pending-event handoff) was kept precisely
  so an OTS front-end could be re-added later beside the GATT one without
  touching the core.

## Capability Demand-Loading

- A `use <capability>` keyword (e.g. `use binbook`) that gates runtime RAM
  allocation for a capability: the module's static/bound RAM is loaded when an
  app declares use of it and freed when that app exits. Rationale: apps should
  not pay RAM for capabilities they do not use. The pattern generalizes beyond
  BinBook to the display composed fast-refresh previous frame
  (`previous_composed_ops`, ~18 KiB), BLE, Wi-Fi, and HTTP, not just the BinBook
  page metadata. Revive when designing the 1.0 capability/RAM-ownership model:
  it needs a language spec for the keyword, a compiler lowering that records
  declared capabilities, and a runtime that binds/unbinds capability-backed
  state on app lifecycle. Surviving parts: the `binbook.*` capability direction
  already in `docs/language_spec.md` §31B, and the firmware radio
  demand-activation investigation (BLE `bt_disable` reclaim, Wi-Fi
  `esp_wifi_deinit` seam absence) captured in
  `docs/specs/2026-06-20-x4-ram-reduction-design.md` for the separate heap/stack
  right-sizing plan.

## Future ESP32-C3 Boards

- Reintroduce XIAO ESP32-C3 e-paper and ESP32-C3 Super Mini support only as
  fresh native firmware targets. Their prior target support was removed because
  native XTEINK X4 is the sole maintained firmware product. Revive a board when
  attached hardware and a concrete product or regression use case justify a
  complete native port and target-aware verification. The retained board pinout
  and wiring documents remain the starting evidence; no old firmware, target
  configuration, examples, or hardware suites survive.
