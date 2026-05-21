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

### 2. Design Portable Wi-Fi And Services Runtime

- Define portable SquidScript Wi-Fi and service APIs that can map onto the
  ESP32-C3 first without baking ESP-specific behavior into the language.
- Decide where driver/service boundaries live across target metadata, firmware
  runtime services, and app-facing APIs.
- Specify runtime errors and target capability checks for devices without Wi-Fi
  or with restricted networking support.
- Decide whether timers should use the same service model, including how a
  target chooses RTC-backed scheduling versus internal timer peripherals.
- Implement `httpServer.*` static serving with arbitrary app-selected asset
  roots, bounded content-type handling, upload staging, and target capability
  checks.

### 3. Implement Device Bindings And SQDEVICE/SQDC

- Load packaged `.sqdevice` resources from SQBC device binding metadata emitted
  from top-level `device {}` declarations before `app.start`; fail launch with
  a runtime error when a binding cannot initialize.
- Implement `device.config.load/set/rebind/save` builtins with result records,
  transactional rebind, volatile temp-run config, and explicit
  `device.config.save("flash")` SQDC persistence.
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

### 4. Consider Reproducible Browser Build Container

- Evaluate a Docker or devcontainer workflow for browser simulator builds and
  Playwright checks so Rust, Node, `wasm-pack`, and system libraries are
  reproducible without making containers mandatory for local development.

### 5. Consider SQBC Library Artifacts

- Investigate whether reusable functionality should be packaged as SQBC library
  artifacts that other SQBC apps can import or link against, including versioning,
  validation, install layout, and firmware/runtime loading semantics.
