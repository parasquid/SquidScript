# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

## Current Track: ESP32-C3 Persistent Reference Runtime

Goal: finish turning the ESP32-C3 Super Mini reference firmware into a
persistent SquidScript app platform prototype.

### 1. Define App Resource Packaging

- Decide how apps bundle non-executable resources such as HTML, JavaScript,
  images, BinBook content, or webserver assets alongside the SQBC artifact.
- Specify install-time layout, resource lookup APIs, and target/runtime limits
  without reintroducing permission-gated app manifests.

### 2. Design Portable Wi-Fi And Services Runtime

- Define portable SquidScript Wi-Fi and service APIs that can map onto the
  ESP32-C3 first without baking ESP-specific behavior into the language.
- Decide where driver/service boundaries live across target metadata, firmware
  runtime services, and app-facing APIs.
- Specify runtime errors and target capability checks for devices without Wi-Fi
  or with restricted networking support.
- Decide whether timers should use the same service model, including how a
  target chooses RTC-backed scheduling versus internal timer peripherals.

### 3. Consider Reproducible Browser Build Container

- Evaluate a Docker or devcontainer workflow for browser simulator builds and
  Playwright checks so Rust, Node, `wasm-pack`, and system libraries are
  reproducible without making containers mandatory for local development.
