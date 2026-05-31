# SquidScript

SquidScript is a small event-driven language, compiler, bytecode VM, simulator, and firmware runtime for constrained display devices.

The project is pre-1.0 and design-active. The current target is a practical development stack for low-RAM e-ink and simple display devices: write apps in a bounded language, compile them off-device, validate and run compact bytecode, and exercise the same app model in browser and firmware environments.

SquidScript is not a JavaScript runtime. It uses familiar syntax where that helps app authors, but the runtime model is built around firmware-owned resources, explicit capabilities, deterministic event handlers, and bounded execution.

## How It Looks

A SquidScript app declares persistent state, reacts to runtime events, and describes screens with explicit display service calls:

```squid
app "counter"

state {
  count: int = 0
}

event.on("app.start") {
  state.load()
  screen.open("main")
}

event.on("key.SELECT") {
  count = count + 1
  state.save()
  screen.refresh()
}

event.on("key.BACK") {
  state.save()
  app.exit()
}

screen("main") {
  display.clear("white")
  display.text("Counter", { x: 20, y: 40, fontHeight: 32 })
  display.text(count, { x: 20, y: 120, fontHeight: 48 })
  display.text("SELECT increments  BACK exits", {
    x: 20,
    y: 220,
    w: 440,
    h: 32,
    fontHeight: 18
  })
}
```

The screen block is render work. Input handlers update state and ask the runtime to refresh, then the runtime reruns the active screen.

`service.display.*` is the canonical display capability namespace. The shorter `display.*` form is source sugar that compiles to the same display operations.

## What This Repo Contains

- `squidc`, the compiler and CLI used to compile, package, inspect, and run SquidScript apps.
- `squidvm-core`, the shared bytecode VM and runtime semantics used by host tools and firmware-facing code.
- SQBC support for the compact bytecode container loaded by runtimes.
- `simulator/browser`, the browser simulator, TypeScript runtime, and WASM compiler bridge.
- `firmware/`, including the Zephyr real-firmware work.
- `targets/`, target definitions for device capabilities and simulator metadata.
- `examples/`, sample SquidScript apps and workflows.
- `docs/`, the language, bytecode, simulator, target, CLI, and firmware documentation.

## Why SquidScript Exists

SquidScript exists for devices where normal application assumptions are too large or too vague: small displays, low memory, slow refresh, limited storage, tight power budgets, and firmware that must stay in charge of hardware.

Apps are event-driven because device interactions are event-driven. They draw screens, react to input, update state, request bounded services, and return control to the runtime. Firmware owns boot, input, display, storage, lifecycle, validation, crash recovery, and hardware capabilities. Apps use those resources through explicit service APIs instead of assuming raw framebuffer access, arbitrary filesystem control, or direct device ownership.

That boundary is the point. It keeps user-authored apps useful without turning the device into a general-purpose scripting host with undefined performance and safety behavior.

## Why Rust

Rust is the main production implementation language because SquidScript needs the same core ideas to work in several places at once.

The compiler, bytecode validation, VM behavior, SQBC encoding, and host tooling benefit from shared types and tests. The browser simulator can reuse compiler logic through WASM, which keeps the web development loop close to the real compiler. Firmware work benefits from Rust's memory discipline, explicit error handling, and fit for embedded systems where unchecked allocation and hidden runtime costs matter.

Rust also gives the CLI a practical cross-platform distribution path while keeping correctness-sensitive code in one implementation family instead of scattering language semantics across unrelated tools.

## SquidScript And CrossPoint

SquidScript and CrossPoint are adjacent projects, not competing names for the same thing.

SquidScript is centered on constrained device apps: a small language, precompiled bytecode, runtime services, firmware-owned resources, and target-aware execution. Its design questions are about what a portable app can ask of a device, how firmware validates that request, and how bounded app behavior can survive on microcontroller-class hardware.

CrossPoint may share nearby interests, but SquidScript's scope is the language and runtime stack for these device apps. Features belong here when they strengthen that app model, its compiler/runtime contract, or the firmware and simulator paths that execute it.

## SquidScript And General Embedded Scripting

SquidScript, [Espruino](https://www.espruino.com/),
[MicroPython](https://micropython.org/), and
[PikaPython](https://github.com/pikasTech/PikaPython) all make small-device
programming more approachable, but they choose different runtime contracts.

Espruino is a mature JavaScript runtime and ecosystem for microcontrollers. It
is a strong fit when you want a general programming language on the device,
broad JavaScript familiarity, interactive hardware exploration, and an
established app/module ecosystem. MicroPython is another strong fit for users
who want a mature Python implementation for microcontrollers and its embedded
ecosystem. PikaPython is a useful reference for constrained-device positioning,
serial script download workflows, host/C binding surfaces, memory visibility in
examples, and board/capability matrices.

SquidScript is intentionally narrower. It is an off-device compiled app
language for constrained display devices: apps become prevalidated bytecode,
run through firmware-owned services, use explicit target capabilities, handle
declared events deterministically, and keep screen rendering replayable.
Espruino has inspired some of SquidScript's hardware-facing design questions,
especially around clear input configuration and edge-triggered device behavior,
and PikaPython is a useful reference for documenting tiny-device workflows and
host-to-firmware bindings. SquidScript adapts those ideas into a bounded app
model instead of becoming a general embedded programming language, embedded
Python runtime, or on-device source compiler.

## Related Xteink Firmware References

These projects are useful references for Xteink X3/X4 hardware behavior, memory tradeoffs, reader workflows, and alternative firmware architecture:

- [CrossPoint Reader](https://github.com/crosspoint-reader/crosspoint-reader): mature Arduino/PlatformIO C/C++ reader firmware for Xteink X3/X4.
- [Papyrix](https://github.com/bigbag/papyrix-reader): CrossPoint-derived Arduino/PlatformIO firmware focused on lightweight reading features and additional formats.
- [pulp-os](https://github.com/hansmrtn/pulp-os): bare-metal `no_std` Rust Xteink X4 firmware; useful reference for Rust-first display, async, and memory-constrained architecture.

## Design Stance

SquidScript is still before 1.0, so accepted design changes are reflected directly in specs, code, fixtures, examples, and docs. The repository does not preserve old APIs or syntax by default while the language is still settling.

The goal is not churn for its own sake. The goal is to keep the implementation honest: unsupported behavior should fail normally, examples should show real language behavior, and the public contract should live in the specs and tests that enforce it.

## Useful Commands

Run Rust compiler, VM, SQBC, and CLI checks from the repository root:

```sh
cargo test
```

Run browser simulator checks from `simulator/browser`:

```sh
npm test
npm run build
npm run test:e2e
```

Build the ESP32-C3 Zephyr firmware with the repository wrapper:

```sh
scripts/c3-supermini-build.sh
```

## License

SquidScript is licensed under the [GNU Affero General Public License v3.0](LICENSE).

## Where To Read Next

- [Language specification](docs/language_spec.md)
- [Language philosophy](docs/language_philosophy.md)
- [SQBC binary format](docs/sqbc_binary_format.md)
- [Browser simulator](docs/browser_simulator.md)
- [Firmware notes](firmware/README.md)
- [Roadmap](ROADMAP.md)
- [Agent guidance](AGENTS.md)
