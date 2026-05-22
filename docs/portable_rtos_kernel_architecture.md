# Portable RTOS Kernel Architecture

Status: Draft source of truth
Purpose: Define the new portable firmware runtime direction for SquidScript.

This document replaces the current serial-harness-shaped runtime direction. It
does not describe a compatibility layer or an alternate legacy path. Firmware
that does not fit this architecture should be changed directly.

## Goals

- Put SquidScript app lifecycle, services, timers, diagnostics, and runtime
  fairness behind a Squid-owned kernel boundary.
- Keep RTOS and executor concepts out of SquidScript source, compiler core,
  SQBC, and portable app APIs.
- Support ESP32-C3 through Embassy/esp-rs, Pico W through Embassy/CYW43, and
  host/browser simulation as the first portability set.
- Keep Zephyr/nRF52 as a planned backend adapter target without making Zephyr
  types part of the portable Squid kernel.
- Make non-blocking service behavior a kernel rule, not a per-service
  convention.
- Use static bounded memory for kernel queues, service state, and message
  buffers unless a service explicitly documents a fixed pool.

## Architecture

The firmware runtime is split into three layers:

```text
SquidScript VM and app lifecycle
        |
Squid kernel: actors, bounded queues, timers, diagnostics, service registry
        |
Backend adapter: Embassy, Zephyr, host/browser event loop, or board harness
        |
Target HAL and device drivers
```

The Squid kernel owns portable runtime behavior:

- app start, event dispatch, exit, and app stack policy
- service command and event routing
- fixed-capacity queues and backpressure behavior
- timer registration and delivery
- service diagnostics and memory accounting
- fairness rules for VM, serial, timers, display, storage, radio, and HTTP

The backend adapter owns scheduler integration:

- spawning or registering service actors
- waking actors from interrupts, timers, serial input, or browser events
- mapping kernel timers to target time sources
- mapping queue waits to Embassy futures, Zephyr message queues/work items, or
  host/browser callbacks
- initializing RTOS, executor, allocator, radio stack, clocks, and board drivers

The HAL owns physical devices:

- GPIO, PWM, display, storage, radio, serial, input, and power drivers
- target-specific pin mappings, peripheral allocation, and bus sharing
- board-specific diagnostics and hardware errata

## Service Model

Services are actors. A service actor has fixed state, a fixed-capacity command
queue, and an optional fixed-capacity event queue back to the kernel.

Calling a SquidScript service API must only enqueue a command, read already
available state, or return an immediate bounded error. It must not run a radio
loop, wait for display busy, poll storage until complete, delay for animation,
or block serial input.

Service commands are service-local. For example, Wi-Fi commands may start or
stop Wi-Fi, report Wi-Fi status, or emit Wi-Fi diagnostics. They must not change
indicator state. Apps compose behavior by calling multiple services from
SquidScript.

The first proof service is the default indicator:

- `service.indicator.write(value)` sends a bounded write command.
- `service.indicator.toggle()` sends a bounded toggle command.
- `service.indicator.read()` returns cached logical indicator state.
- `service.indicator.breathe()` sends a bounded breathing command.
- On ESP32-C3, breathing is implemented by the LEDC/PWM backend and advanced by
  service-owned timer progress, not by a VM or serial busy loop.

## Backend Mapping

### ESP32-C3 Reference Backend

The ESP reference backend should use Rust `no_std`, `esp-hal`,
`esp-hal-embassy`, Embassy, and `esp-radio` where those libraries satisfy the
required behavior. Embassy is a backend detail. SquidScript APIs and compiler
core must not expose Embassy tasks, futures, channels, or spawners.

Embassy fits this model because its embedded executor supports async tasks
without requiring heap allocation and uses statically allocated tasks. The Squid
kernel should still define its own actor/message contract so later backends do
not inherit Embassy-specific semantics.

### Pico W Backend

The Pico W backend should use Embassy and the CYW43 driver family as the first
Wi-Fi portability target. CYW43 supports Pico W Wi-Fi station mode, AP mode,
scanning, Ethernet-frame integration, and interrupt-driven device events. The
Squid Wi-Fi service contract must stay above CYW43 driver state and bus details.

### Host And Browser Simulation

The host and browser backends should run the same kernel service contract on a
simulated event loop. Simulation may fake device timing and radio visibility,
but it must not pretend to validate hardware timing, flash endurance, power, or
RF behavior.

### Zephyr/nRF52 Backend

Zephyr/nRF52 remains a planned adapter target. Zephyr has native fixed-size
message queues and workqueues that can represent Squid actors, but Zephyr
objects, Kconfig, devicetree, and thread handles stay behind the backend
adapter. Bluetooth should become a sibling radio service, not a Wi-Fi extension.

## Memory And Fairness

Kernel-owned memory is statically bounded:

- each actor declares command queue capacity
- each event queue declares capacity
- payloads are fixed-size values or handles into explicit pools
- networking, storage, display, and package-resource buffers are named pools
- RAM diagnostics report configured capacity, current use, and available RAM
  where the backend can measure it

When a queue is full, the caller receives a bounded service error or the service
uses an explicitly documented drop/coalesce policy. A service must not hide
unbounded retries behind a convenient API.

Fairness rules:

- VM dispatch must return to the kernel between app events.
- Serial input must remain serviceable while timers, indicator PWM, Wi-Fi,
  storage, or display work is active.
- Display and storage drivers must use bounded slices or backend async waits
  around long busy periods.
- Radio services must expose progress and diagnostics without requiring app code
  to run polling loops.
- Shutdown tears down service-owned resources through service commands and
  backend cleanup, not by reaching across service internals.

## Documentation And Spec Discipline

This architecture is a direct replacement before SquidScript 1.0. Do not add
old/new runtime compatibility, SQBC compatibility modes, versioned function
names, unsupported-version branches, or migration paths unless the user
explicitly asks for a specific bridge.

Build IDs, source revisions, and image hashes are diagnostics/provenance only.
They are not compatibility contracts.

Docs that mention bytecode/runtime/app versions, compatibility profiles,
unsupported bytecode versions, or future-version compatibility should be
removed or rewritten to describe current-format behavior and target capability
checks only.

## Implementation Order

1. Correct docs/specs so they no longer describe versioned compatibility.
2. Add a small `squid-kernel` style service model with host-testable actor
   queues and fake-clock tests.
3. Move the ESP default indicator behind the new service model.
4. Verify indicator breathing remains non-blocking and serial stays responsive.
5. Move timers and serial/app lifecycle next, then Wi-Fi, storage, display, and
   HTTP as separate service actors.

## References

- Embassy project docs: https://embassy.dev/
- Embassy executor docs: https://docs.embassy.dev/embassy-executor/
- Embassy CYW43 docs: https://docs.embassy.dev/cyw43
- Zephyr message queues: https://docs.zephyrproject.org/latest/kernel/services/data_passing/message_queues.html
- Zephyr workqueues: https://docs.zephyrproject.org/latest/kernel/services/threads/workqueue.html
