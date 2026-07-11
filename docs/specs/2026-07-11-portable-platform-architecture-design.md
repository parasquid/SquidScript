# Portable Platform Architecture Design

Status: Approved design; implementation pending

Audience: SquidScript compiler, VM, firmware, target, simulator, tooling, and
example maintainers

## Purpose

SquidScript is a general device-app language for low-memory embedded products.
It is intentionally bounded, event-driven, compiled off-device, and executed
through firmware-owned resources. E-readers and BinBook are the first demanding
use case, but the portable language and runtime must also support watches,
headless devices, LCD products, sensors, and boards based on ESP, Nordic,
STM32, RP2350, and comparable microcontrollers.

The architecture must prevent a concrete board such as XTEINK X4 from defining
portable compiler, bytecode, runtime, CLI, or simulator semantics. X4 remains
the reference hardware target and must keep working throughout the migration.

## Core Decisions

- Target JSON is the only per-target artifact. A target never requires a
  target-specific Rust crate, generated source tree, app manifest, or second
  hardware configuration file.
- App identity, app configuration, triggers, and metadata are valid SquidScript
  source. Physical devices and service wiring belong to target JSON. The
  `device {}` declaration and `.sqdevice` format are removed.
- Source normally does not declare capabilities. The compiler infers them from
  service calls, target GPIO symbols, events, and operations.
- Targetless compilation remains supported and produces portable SQBC.
- A target-checked compile validates inferred demand and concrete operations
  against target JSON without specializing SQBC or embedding an exact target
  ID.
- A runtime validates SQBC capability demand before starting the app.
- Public services and hardware contracts are executor-neutral and `no_std`.
  First-party firmware uses Embassy internally.
- Essential VM/runtime behavior requires no heap. Optional services may use a
  bounded allocator only when the platform declares and budgets it.
- Only one SquidScript VM session is active. Firmware may run concurrent
  services and dormant trigger monitors, but dormant apps execute no code.
- BinBook is a first-party optional service module, not part of VM core.
- X4 and a real host platform are the first portability proofs.
- Verified behavior is migrated. Incomplete, stale, misleading, or unsupported
  surfaces are deleted and may return only through the new contracts.

## Layering

```text
SquidScript source
        |
        v
parser -> typed AST -> semantic analysis -> typed IR -> SQBC
                    |                         |
                    +-- capability demand ---+
                                              |
target JSON -> target model/catalog ----------+-- compatibility check
                                              |
                                              v
                                  portable SQBC artifact
                                              |
                                              v
                     VM + lifecycle + service dispatcher
                                              |
                       semantic service/hardware contracts
                                              |
                        platform implementation and drivers
                                              |
                                       HAL / hardware
```

Portable layers may know logical display coordinates, storage volumes,
logical inputs, power requests, byte transfers, and service results. They must
not know ESP peripheral types, X4 pins, SSD1677 command sequences, flash tool
arguments, webcam details, or board-specific process policy.

## Terminology and Ownership

### Target

A concrete product or breadboard assembly described by one target JSON file.
It owns product facts: MCU choice, memory, pins, buses, attached devices,
storage layout, logical inputs, service bindings, runtime budgets, firmware
image layout, and simulator layout reference.

### Platform

A reusable firmware/tooling integration selected by a code-backed catalog ID,
such as an Embassy ESP32-C3 platform or the host platform. A platform owns the
Cargo package, target triple, HAL/executor integration, linker/build rules,
flasher/monitor adapter, and the device factories it can support.

### Driver

A reusable typed implementation for a device class or controller. A driver
catalog entry declares supported bus/resource kinds, required configuration,
provided semantic capabilities, required memory, and compatible platforms.
Target JSON configures a driver; it cannot invent driver behavior.

### Service

A portable app-facing semantic endpoint, normally under `service.*`. Examples
include display, input, timer, power, upload, and Wi-Fi. Services expose typed
methods, results, operation modes, capability IDs, ownership, and cleanup
rules.

### Target-exported GPIO

A target may export safe board-level GPIO symbols under `target.gpio.*`.
SquidScript passes these typed symbols to the generic `hardware.gpio` API.
Target checking validates the pin, direction, electrical mode, and app-access
policy. A targetless build retains the symbolic GPIO name and required mode for
later compatibility validation.

GPIO is the low-level escape hatch in this architecture. ADC controllers,
PWM engines, I2C/SPI buses, display controllers, storage controllers, radios,
and comparable devices are firmware/driver concerns unless a future language
design establishes a concrete app-facing need for another primitive.

## Shared Rust Contracts

### `squid-service-model`

A heap-free `no_std` crate is the canonical source for:

- capability and resource-kind identifiers;
- service namespace and method identifiers;
- argument and result type descriptions;
- method execution class: immediate, awaited request, event operation, or
  subscription;
- render-safety and lifecycle ownership;
- required capability inference;
- standard error/result field definitions.

Compiler, bytecode, VM, runtime, target validation, WASM, and generated
reference documentation consume this model. They must not maintain parallel
method-name or builtin-ID match tables. The existing BinBook capability JSON
ceases to be an authoritative implementation input.

### `squid-bytecode`

A heap-free `no_std` crate owns SQBC opcodes, sections, builtin IDs, decoding,
validation, and program metadata. Encoding is enabled with an `alloc`/host
feature. Neither compiler nor VM may privately redefine bytecode constants.

SQBC carries inferred capability/resource demand but no exact target identity
and no compatibility-version framework. Target validation does not rewrite
logical operations.

### `squid-target-model`

A host/build crate owns target JSON deserialization, JSON Schema generation,
catalog lookup, semantic validation, compatibility checking, build planning,
and generated target constants. Browser/WASM validation uses the same model or
generated schema; handwritten partial TypeScript target interfaces are not an
independent contract.

## Target JSON Model

Target JSON remains explicit and readable. It references code-backed catalog
IDs rather than importing JSON fragments or accepting arbitrary commands.

The resolved model contains at least:

- identity and status;
- platform ID and MCU facts;
- memory and optional allocator budget;
- pins and electrical capabilities;
- buses and ownership/sharing constraints;
- devices with driver IDs and driver configuration;
- semantic service bindings and default instances;
- app-visible GPIO exports and aliases;
- logical input events and gesture policy;
- storage volumes/libraries and persistence properties;
- power/wake capabilities;
- radios and transports;
- runtime caps;
- firmware image/partition facts required by the selected platform;
- simulator layout and simulation policy;
- target-owned verification suites.

Validation must reject unknown catalog IDs, incompatible platforms/drivers,
missing required pins, bus conflicts, unsafe duplicate ownership, invalid
electrical modes, invalid GPIO exports, impossible service bindings, duplicate
device IDs, invalid flash geometry, and budgets exceeding target memory.

Platform/build metadata such as Cargo package, toolchain, target triple, and
flasher program belongs to the platform catalog. Product-specific partition
geometry and image placement remain target facts.

`squidc target build` resolves the target, selects the platform package and
Cargo features, passes the canonical target path to its build script, and
generates typed constants into `OUT_DIR`. No generated target code is tracked.

## Source and Compilation Model

### App and target selection

Exact target clauses are removed from app declarations. Runnable artifacts may
be built in two modes:

- Portable: parse, type-check, infer demand, and emit portable SQBC without a
  target.
- Target-checked: do all portable work, then validate against a selected target
  before emitting the same logical artifact.

Commands that communicate with a concrete device resolve its target through
the command argument and/or device identity. They must validate the artifact
before install/run.

### Capability inference

Using a method, event, or target GPIO symbol is a requirement. For example:

- `service.display.text` requires a bound display service;
- `event.on("key.BACK")` requires the logical BACK input;
- `file.readText` requires the file service and an appropriate logical volume;
- `binbook.open` requires the optional BinBook module;
- `hardware.gpio.write(target.gpio.statusLed, true)` requires the exported
  `statusLed` GPIO with output permission;
- `hardware.gpio.read(target.gpio.userButton)` requires the exported
  `userButton` GPIO with input permission.

Targetless SQBC records this demand. Target checks report source diagnostics at
the originating call, event, or GPIO reference. Runtime rejection is a safety
net for portable artifacts, not the normal target-checked feedback path.

No general `requires` block is added for hardware. Literal coordinates, font
requests, target GPIO names, logical keys, and pixel formats are validated
where statically known. Dynamic code may query target-derived service
information and adapt at runtime.

### Board definitions, GPIO, and drivers

Target JSON is the board definition available to SquidScript. It exposes safe
GPIO symbols directly:

```squid
event.on("key.SELECT") {
  hardware.gpio.write(target.gpio.statusLed, true)
  let pressed = hardware.gpio.read(target.gpio.userButton)
}
```

The `target.gpio` namespace is generated conceptually from the selected target,
but no source file is generated. Export names must be valid SquidScript
identifiers. Each export identifies a target pin plus allowed input/output
operations and electrical policy. Targetless compilation records the symbolic
name and required access; target checking and firmware loading resolve it to a
compact GPIO ID.

Complex devices are not surfaced as generic raw buses or app bindings. Target
JSON selects a registered driver, supplies its bus/pin/controller
configuration, and binds the capability it provides. For X4, the target
describes the SSD1677-based e-paper device and selects the matching driver; the
driver provides the default `service.display` implementation. SquidScript uses
only the portable display API:

```squid
screen("main") {
  service.display.text("Hello", { x: 8, y: 8 })
}
```

The target validator must reject a configured device whose driver is missing,
incompatible with its bus/controller/panel, or unable to provide the declared
service. When more than one device could provide a service, target JSON must
select the default explicitly rather than making the app bind it.

### Awaited and event operations

`await` is required at every yielding request-response call:

```squid
let result = await file.readText(fileRef)
```

The method descriptor determines whether `await` is required, forbidden, or
inapplicable. Compilation rejects a missing `await`, awaiting an immediate
method, or awaiting an event/subscription method.

Naming conventions are normative:

- immediate or awaited request-response methods use domain verbs;
- `.start`, `.cancel`, and `.status` describe finite event-oriented work;
- `.watch` and `.unwatch` describe continuing subscriptions;
- `app.triggers` describes dormant firmware-owned activation routes.

Awaiting suspends the current handler in the VM. Firmware continues polling
and running platform services. Completion resumes the same handler exactly
once with a typed result or timeout. No other event handler may re-enter the
VM while that handler is active or suspended.

### Namespace normalization

- `service.*`: portable target/firmware services.
- `hardware.gpio`: generic GPIO operations on typed target-exported pins.
- `app`, `screen`, `state`, `system`, and `string`: VM/app concepts.
- `file`: app-facing logical file/document references.
- `binbook`: optional first-party document API.

Only canonical spellings remain. Source sugar such as `display.*` is removed
without aliases or migration diagnostics.

## Runtime and Lifecycle

Screens and displays are optional. A valid headless app may contain events,
state, timers, services, and hardware operations without a screen.

The runtime has one active VM session. Firmware services may use Embassy tasks,
interrupts, and bounded queues, but app bytecode is serialized.

Dormant apps may register triggers. When a trigger fires:

1. Firmware enqueues it using the bounded event queue.
2. A running or awaited foreground handler completes first.
3. Firmware runs the foreground app's exit lifecycle.
4. The foreground app ID is pushed as a return target.
5. The triggered app starts with a fresh VM session and receives the trigger.
6. When it exits, the previous app starts fresh with return semantics.

The queue remains ordered and drops the newest event on overflow while
retaining an overflow diagnostic. App execution is never re-entrant.

The VM/runtime operation interface generalizes the existing storage resume
path into typed pending requests. It allows one pending awaited request for the
active handler. Every request has an owner, timeout, cancellation path, result
shape, and cleanup rule. Exit, crash, replacement, and timeout must release
all owned resources.

## Memory Model

The following must work without an allocator:

- SQBC validation and execution;
- lifecycle and event queues;
- state primitives and bounded state persistence interfaces;
- timers and logical input;
- basic display command emission;
- generic GPIO operations on target-exported pins;
- essential protocol parsing.

Optional network stacks, package staging, richer document services, or display
pipelines may use allocation when a platform provides it. Their target/catalog
metadata must expose required static buffers, heap budget, stack budget, and
failure behavior. Allocation is never implied merely because a development
target has spare RAM.

## Service Modules and BinBook

Services are typed first-party modules selected by target capability. Target
JSON configures registered devices and drivers, then binds the services those
drivers provide. It cannot create new language builtins. Complex peripherals
belong behind these drivers rather than generic app-facing I2C/SPI/controller
APIs.

BinBook integration remains in SquidScript as an optional module with compiler,
SQBC, VM, runtime, simulator, and X4 tests. The reusable BinBook crates are
consumed through pinned Git revisions so a SquidScript checkout is reproducible
without a sibling-directory convention. Joint local development may use a
documented Cargo patch override.

## Simulator and Host Port

The host port is a real executable platform with deterministic implementations
or explicit unsupported capabilities. It is used for lifecycle, capability,
service, headless, and failure testing without pretending to be hardware.

The browser simulator loads bundled validated target profiles and layouts. It
derives display surfaces and logical controls from metadata, supports headless
profiles, and simulates only declared portable capabilities. Hardware-only
behavior remains explicitly unsupported. Arbitrary user target loading is not
part of this refactor.

Compiler and VM semantics are shared through Rust/WASM. TypeScript owns browser
UI and browser-host service adapters, not a second language or VM contract.

## Examples as Executable Documentation

The repository provides focused SquidScript examples for language constructs
and X4 capabilities. Examples remain target-neutral source and are compiled or
run with `--target xteink-x4` for the X4 regression matrix.

Each public language construct and service method must have a focused example
or be exercised by a clearly identified larger example. Examples are tested at
their natural boundary: CLI/compiler, browser simulator, host port, or X4
hardware. Full visual examples are not replaced by stripped app-test fixtures;
small deterministic companions are allowed when automation requires them.

## Non-goals

- Backwards compatibility, aliases, migration readers, or deprecated syntax.
- A target-authoring wizard or arbitrary browser target loader in this slice.
- Third-party runtime service registration or a stable external service SDK.
- Multiple simultaneously executing SquidScript apps.
- Dynamic allocation as a core language/runtime requirement.
- Target-specific SQBC optimization variants.
- Adding a second physical MCU port without supported hardware and verification.

## Removal and Revival Record

Public syntax, APIs, transports, configuration models, and substantive
architecture alternatives removed or deferred by this refactor must remain
discoverable in `ICEBOX.md`. Each entry records why the idea is not active,
the concrete condition that would justify reviving it, and the implementation
pieces that remain useful.

The initial ledger covers app-owned device/`.sqdevice` configuration,
source-embedded exact targets, shorthand service namespaces, generic raw
ADC/PWM/I2C/SPI APIs, per-target firmware/tooling branches, independent
TypeScript runtime semantics, and no-op optional service backends. During
implementation, an agent must extend the ledger before deleting any additional
real public or documented surface. Do not preserve fake or never-implemented
syntax as compatibility fixtures; record the underlying design idea without
inventing a historical language contract.

## Acceptance

- Generic compiler, bytecode, VM, runtime, CLI, and browser code contains no X4
  policy or retired-board identity.
- X4 target JSON alone selects and configures the reusable ESP platform.
- A host target executes the same VM/runtime contracts.
- Portable SQBC runs on compatible targets and is rejected before execution on
  incompatible targets.
- Headless apps, target-exported GPIO, driver-provided services, `await`, and
  dormant trigger switching work through tests.
- Essential runtime crates build and test without allocator support.
- All verified X4 software and hardware behavior remains green.
- The focused X4 feature-example inventory is executable as a regression suite.
