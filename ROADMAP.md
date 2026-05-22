# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

## Current Track: ESP32-C3 Persistent Reference Runtime

Goal: finish turning the ESP32-C3 Super Mini reference firmware into a
persistent SquidScript app platform prototype.

### 1. Promote App Resource Packages Beyond Browser-Sim

- Implement firmware install protocol support for `.squid.zip` packages,
  including host-side ZIP validation, `main.sqbc` install, and read-only
  package resource files under arbitrary safe package-relative paths.
- Add generic app resource APIs for packaged files such as icons, BinBook
  samples, config files, and future `app.resource(...)` handles.

### 2. Continue Non-Wi-Fi RTOS Service Actor Migration

- Replace the current one-step LittleFS storage actor backend with a fully
  async storage backend design: every installed-app, package-resource, state,
  startup-scan, and format operation should progress through bounded jobs or
  backend async waits rather than blocking the serial/runtime loop. Keep the
  portable SquidScript storage API unchanged, and document any LittleFS/flash
  calls that cannot be made interruptible.
- Extend resumable VM dispatch coverage beyond linear handlers so nested
  functions and screen rendering can suspend around storage-backed SQBC chunk
  reads without falling back to synchronous reads.
- Promote the headless draw-log display queue into the firmware display service
  contract, then add a backend adapter for physical displays.
- Replace the standalone XTEINK X4 hello display bring-up's blocking init,
  refresh, busy wait, and sleep path with bounded/async display backend
  progress before using X4 for routine development iteration.
- Consider promoting `firmware/squid-firmware/src/kernel.rs` into a reusable
  `squid-kernel` library once a second backend consumes the actor/service
  primitives.
- Keep firmware target profile roles explicit: ESP32-C3 Super Mini is the
  serial-first `dev` iteration platform; XTEINK X4 is the `release` and
  reference hardware validation platform.

### 3. Design Portable Wi-Fi And Services Runtime

- Complete ESP32-C3 SoftAP hardware validation: prove beacon visibility and
  client joinability from a phone/laptop with
  `STRICT_PROBE=1 scripts/c3-supermini-test-wifi-state.sh`, and debug radio
  startup if host scans do not show the AP despite successful firmware status
  records.
- Investigate why Rust/esp-rs AP probes report started/configured SoftAP state
  but host scans do not show the SSID on the current ESP32-C3 Super Mini, while
  MicroPython AP mode has only been intermittently visible on the same board.
  Verify with an alternate client and board placement/power checks before
  treating this as a Rust-only radio backend issue.
- Split the current Wi-Fi service actor's bounded start/stop wrapper into
  smaller async progress steps if future esp-rs APIs expose non-blocking radio
  configuration/start/stop operations.
- Use the minimal Rust `wifi-ap-probe` experiment as the optional isolation
  path when SquidScript firmware reports started/configured AP state but
  external beacon visibility or client joinability is still unclear.
- Add the network stack needed for AP IP behavior, DHCP, and later HTTP serving;
  the AP-first backend should not imply these are complete until verified.
- Define password/security policy and `startAP` option support for AP mode;
  current developer AP defaults allow open networks only.
- Design app-managed Wi-Fi profile persistence: decide how a future Wi-Fi
  settings app can create, update, select, and delete named station profiles;
  where profile metadata and credentials are stored; whether credentials are
  encrypted or hardware-protected per target; how profiles are shared across
  apps without exposing passwords to SquidScript source, state, logs,
  diagnostics, or source maps; and what recovery/reset flow clears saved
  credentials.
- Implement the ESP32-C3 Rust station backend behind the public named-profile
  `service.wifi.connect/disconnect/status` API. The current API layer may
  report explicit backend errors until scan/auth/DHCP are proven.
- Add station scan listing, profile setup UI, hostnames, and configurable IP
  APIs after named-profile station connection status is stable.
- Add target capability checks for devices without Wi-Fi or with restricted
  networking support.
- Add Pico W Wi-Fi backend exploration using the same `service.wifi.*`
  contract.
- Add bus-attached Wi-Fi co-processor support as `WifiBackend`
  implementations over UART/SPI/I2C where modules support those transports,
  including ESP-AT-style ESP8266/ESP32 modules.
- Add nRF52840 Bluetooth backend exploration as a sibling radio service rather
  than as part of the Wi-Fi trait.
- Implement PWM-capable output backends for indicators and GPIO-style outputs.
  `service.indicator.breathe()` should be a convenience over target hardware
  PWM where available, with shared output semantics for future dimming,
  breathing, and smooth transitions rather than software blinking in the main
  loop.
- Define configurable GPIO PWM APIs and target metadata: expose supported PWM
  pins/channels, frequency ranges, duty-cycle resolution, polarity, and
  allocation conflicts so apps can request PWM output without assuming every
  GPIO can provide it.
- Add firmware diagnostic protocol commands for radio and serial debugging:
  app-independent Wi-Fi status, AP config dump, station/client list or count,
  last radio error details, and a serial framing/self-test command so hardware
  checks do not depend only on SquidScript `debug.print` output.
- Implement `httpServer.*` static serving with arbitrary app-selected asset
  roots, bounded content-type handling, upload staging, and target capability
  checks.

### 4. Implement Device Bindings And SQDEVICE/SQDC

- Load packaged `.sqdevice` resources from SQBC device binding metadata emitted
  from top-level `device {}` declarations before `app.start`; fail launch with
  a runtime error when a binding cannot initialize.
- Implement `device.config.load/set/rebind/save` builtins with result records,
  transactional rebind, volatile temp-run config, and explicit
  `device.config.save("flash")` SQDC persistence.
- Add explicit device binding unassign/release semantics so a pin claimed by an
  indicator, display, PWM output, or other peripheral can be freed before being
  rebound or reused as raw GPIO.
- Move SQDEVICE/SQDC parsing and persistence into firmware-owned runtime paths
  and share bounded typed-record helpers with SQST where practical.
- Wire the ESP32-C3 `indicator.default` binding through real SQDEVICE/SQDC
  config instead of the current fixed onboard-indicator default.
- Extend browser-sim package install/runtime to store `.sqdevice` resources,
  bind browser canvas/keyboard configs, and route multiple display targets.
- Implement `service.display.select(...)` in compiler, SQBC, VM, browser-sim,
  and firmware display routing.
- Extend ESP32-C3 reference firmware for dynamic GPIO/SPI display binding,
  unknown GPIO failures, duplicate pin warnings, volatile temp-run config, and
  explicit SQDC flash save.
- Add example package resources that demonstrate `device {}` bindings for
  `indicator.default`, `display.default`, and browser input/display configs.

### 5. Consider Reproducible Browser Build Container

- Evaluate a Docker or devcontainer workflow for browser simulator builds and
  Playwright checks so Rust, Node, `wasm-pack`, and system libraries are
  reproducible without making containers mandatory for local development.

### 6. Consider SQBC Library Artifacts

- Investigate whether reusable functionality should be packaged as SQBC library
  artifacts that other SQBC apps can import or link against, including
  validation, install layout, and firmware/runtime loading semantics.
