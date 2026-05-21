# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

## Current Track: ESP32-C3 Persistent Reference Runtime

Goal: finish turning the ESP32-C3 Super Mini reference firmware into a
persistent SquidScript app platform prototype.

### 1. Promote App Resource Packages Beyond Browser-Sim

- Implement runtime resource/static serving APIs, including `web/` asset-root
  enforcement and firmware install protocol support.

### 2. Design Portable Wi-Fi And Services Runtime

- Define portable SquidScript Wi-Fi and service APIs that can map onto the
  ESP32-C3 first without baking ESP-specific behavior into the language.
- Decide where driver/service boundaries live across target metadata, firmware
  runtime services, and app-facing APIs.
- Specify runtime errors and target capability checks for devices without Wi-Fi
  or with restricted networking support.
- Decide whether timers should use the same service model, including how a
  target chooses RTC-backed scheduling versus internal timer peripherals.

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

### 5. Refactor Large Modules Into Logical Owners

- Split the ESP32-C3 serial firmware binary so boot wiring stays in
  `src/bin/c3_supermini_serial_hello.rs` and serial protocol, runtime host,
  app lifecycle, timers, and logging move into library modules.
- Move the browser WASM loader beside generated WASM assets or rename it for
  clearer compiler ownership, and split `simulator/browser/src/types.ts` once
  runtime/compiler/rendering/target types grow further.
- Clarify fixture ownership across compiler language fixtures, VM runtime
  fixtures, and hardware/CLI conformance fixtures.

### 6. Draft README Project Philosophy Notes

- Brainstorm a README entry explaining why the implementation leans on Rust:
  browser WASM reuse first, plus shared compiler/runtime logic, portability,
  memory discipline, and embedded-firmware fit.
- Explain how SquidScript differs from Crosspoint in goals, constraints, app
  model, runtime philosophy, and target-device assumptions.
- Capture the project philosophy clearly enough for new contributors to
  understand what belongs in SquidScript versus adjacent projects.
