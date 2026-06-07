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
