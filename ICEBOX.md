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

## Portable Platform Alternatives

- **App-owned device binding and `.sqdevice` configuration.** Remove the
  top-level `device {}` binding model and separate `.sqdevice` resources during
  the portable-platform refactor. The target JSON is the board definition and
  owns physical devices, driver selection, and service wiring; duplicating
  those facts inside an app creates two hardware configuration authorities and
  makes portable services depend on package-local device files. Revive an
  app-owned binding layer only when a concrete product needs an installed app
  to select or replace physical devices independently of the flashed target
  definition, and target-level defaults plus ordinary service selection cannot
  express it. Surviving parts: target JSON device/driver declarations, automatic
  service binding, package assets unrelated to hardware configuration, and
  `target.gpio.*` access to explicitly exported GPIO.

- **Source-embedded exact target selection.** Remove exact board IDs from app
  declarations. A source file should compile portably from inferred capability
  demand, while optional target checking verifies it against a selected target
  JSON. Revive source-level exact targeting only when a real app has semantics
  that cannot be represented by service use, target GPIO symbols, or target
  compatibility checks and must refuse every otherwise compatible board.
  Surviving parts: CLI/device target selection, target-checked compilation,
  portable SQBC capability demand, and runtime compatibility validation.

- **Shorthand service namespace aliases.** Remove alternate spellings such as
  `display.*` while the service API is normalized around canonical namespaces.
  Parallel spellings multiply compiler, formatter, documentation, and example
  surfaces without adding capability. Revive a shorthand only if authoring
  studies show a material usability problem with the canonical namespace and a
  general alias rule can cover services consistently rather than special-case
  one API. Surviving parts: the complete `service.display.*` capability and its
  compiler, SQBC, VM, firmware, simulator, and example behavior.

- **Generic raw ADC, PWM, I2C, and SPI app APIs.** Keep generic
  `hardware.gpio` as the low-level escape hatch, but defer raw ADC/PWM/bus APIs.
  Displays, sensors, storage, radios, and other nontrivial peripherals need
  controller-specific initialization, bounds, ownership, and error policy;
  target-selected typed drivers should expose semantic services instead of
  forcing apps to reproduce drivers. Revive an individual primitive when a
  concrete SquidScript use case needs direct portable access, no suitable
  service/driver abstraction fits, and its bounded nonblocking API plus target
  safety model can be specified and verified on real hardware. Surviving parts:
  internal embedded-hal ADC/PWM/I2C/SPI use by registered drivers, target JSON
  bus/device descriptions, the generic awaited-operation machinery, and
  target-exported `hardware.gpio`.

- **Per-target firmware crates and target-specific tooling branches.** Replace
  the X4-named firmware assembly and CLI target-ID branches with reusable
  platform packages, registered drivers, and target JSON composition. A crate
  or command branch per board makes a breadboard require code even when every
  component already has a driver. Revive a target-specific Rust integration
  only when verified hardware behavior cannot be represented by the target
  schema, a reusable driver, or a platform hook without making the shared
  abstraction dishonest. Surviving parts: the reusable ESP platform, X4 panel
  and board drivers, target-owned partitions/layout/tests, and X4 target JSON.

- **Independent TypeScript language/runtime semantics.** Remove browser-owned
  compiler/VM behavior that duplicates Rust semantics; the browser should use
  shared Rust/WASM and retain only UI plus browser host-service adapters.
  Parallel implementations drift and make simulator success weaker evidence
  for firmware. Revive an independent browser engine only if a concrete
  supported environment cannot execute WASM and accepting a separately tested
  language/runtime implementation becomes an explicit product goal. Surviving
  parts: React UI, canvas renderer, IndexedDB-backed storage, browser input,
  layouts, host adapters, and browser E2E tests.

- **No-op implementations for absent optional services.** Remove production
  no-op backends that make an unavailable service appear registered and return
  generic unsupported results later. Capability absence should be visible to
  target checking and SQBC load validation. Revive a no-op implementation only
  as an explicitly named test double or simulator fixture whose observable
  behavior is the purpose of the test, never as target capability evidence.
  Surviving parts: explicit absent-capability descriptors, structured target/
  load diagnostics, and honest simulator unsupported states.

## Future ESP32-C3 Boards

- Reintroduce XIAO ESP32-C3 e-paper and ESP32-C3 Super Mini support only as
  fresh native firmware targets. Their prior target support was removed because
  native XTEINK X4 is the sole maintained firmware product. Revive a board when
  attached hardware and a concrete product or regression use case justify a
  complete native port and target-aware verification. The retained board pinout
  and wiring documents remain the starting evidence; no old firmware, target
  configuration, examples, or hardware suites survive.
