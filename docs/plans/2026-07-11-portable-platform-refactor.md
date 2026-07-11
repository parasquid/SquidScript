# Portable Platform Refactor Implementation Plan

**Goal:** Replace X4-shaped cross-layer architecture with a portable,
target-JSON-composed SquidScript platform while keeping verified X4 behavior
working and adding an executable X4 feature-example regression suite.

**Design authority:**
`docs/specs/2026-07-11-portable-platform-architecture-design.md`

**Execution strategy:** Staged replacement. Every task below is a separate
review and verification boundary. Do not batch independent firmware changes.
There is no backwards-compatibility work before 1.0.

## Cold-Start Protocol

An agent starting without conversation history must do these steps in order:

1. Read repository `AGENTS.md` completely, then `AGENTS.local.md` when present.
2. Read `.current_agent_work`, this plan, and the design authority above.
3. Read `ROADMAP.md`, `ICEBOX.md`, `docs/language_spec.md`,
   `docs/runtime_limits.md`, `docs/target_definition_reference.md`, and the
   verification standards.
4. Run `git status --short`. Treat every existing change as user-owned. Never
   discard, hide, overwrite, or fold it into this work without agreement.
5. Replace `.current_agent_work` with the active task, evidence, next commands,
   and expected handoff before changing code, running tests, or touching
   hardware.
6. Keep this entire checklist. Mark items complete or blocked in place; never
   delete completed items merely to show remaining work.
7. Work on only one numbered task, and only one independently measurable
   firmware sub-slice, at a time.
8. Do not commit until the task's verification gate passes. A failed or
   unverified slice may remain in the working tree but may not be committed.
9. On hardware tasks, probe the attached device and serial ownership first.
   Run hardware commands sequentially. Use a fresh inspected webcam capture
   for any visual claim.
10. Save noisy transcripts and temporary measurements under `/tmp` or ignored
    scratch paths, not in this plan or reference documentation.

## Known Starting State

Confirm these facts rather than assuming they remain true:

- The workspace currently has compiler/VM/protocol crates under
  `compiler/rust/crates`, firmware crates under `firmware/native/crates`, one
  executable target JSON, and an X4-specific browser target import.
- The largest responsibility tangles are the CLI main module, firmware runtime,
  and X4 firmware main/lib modules. Do not mechanically split them; move code
  only after its destination contract exists.
- `squidc-core` owns both language compilation and SQBC encoding.
  `squidvm-core` privately owns matching bytecode/builtin definitions.
- `CapabilityDemand` exists in the VM program loader as a hardcoded boolean
  structure but is not the shared compiler/target/runtime contract.
- Target loading in the CLI parses only a small subset of target JSON and
  hardcodes ESP32-C3 flash/partition assumptions.
- Generic tests still contain X4 and retired-board identities.
- Firmware core contains service-specific traits and no-op backends in one
  large runtime module, including X4-specific display/backend strings.
- Browser code imports the X4 target directly and keeps separate handwritten
  target types and runtime behavior.
- X4 firmware consumes BinBook crates through sibling-directory paths.
- The repository already has a bounded foreground lifecycle, return stack,
  dormant trigger registrations, and drop-newest event queue. Preserve their
  verified semantics while moving ownership.
- The current branch recently completed native X4 and Zephyr-removal work.
  Do not revive Zephyr, old targets, backend selectors, or C FFI architecture.

## Target End State

The intended workspace ownership is:

```text
crates/
  squid-service-model/       canonical service/capability/resource model
  squid-bytecode/            SQBC encoding, decoding, validation, metadata
  squid-target-model/        target schema, catalogs, validation, build plans
  squidc-core/               parser, typed AST/IR, semantic compiler
  squidvm-core/              heap-free VM execution
  squid-device-protocol/     portable host/device wire protocol
  squidscript-runtime/       lifecycle, app store, service routing
  squidc/                    CLI binary and host workflows
platforms/
  esp/                       reusable Embassy ESP firmware/platform
  host/                      executable deterministic host platform
targets/
  *.target.json              the only per-target artifacts
simulator/browser/           generic browser target UI and host services
```

The exact directory move occurs only after shared crate boundaries compile.
Package names above are fixed. Do not invent alternate crate names or leave
duplicate old/new crates after consumers have migrated.

## Global Implementation Rules

- Target JSON is canonical for product wiring and configuration. Generated
  constants go to `OUT_DIR`; never track generated per-target Rust.
- Platform and driver catalogs are typed Rust data. JSON selects catalog IDs;
  it does not carry arbitrary shell commands, Rust paths, or executable policy.
- App configuration is `.squid`; physical devices and service wiring are target
  JSON. Delete `device {}` and `.sqdevice` once target drivers own all active
  device configuration.
- Source use implies capability demand. Do not add a general hardware
  `requires` declaration or a separate app manifest.
- Targetless SQBC is supported and portable. Target checking is validation,
  not target-specific lowering.
- Missing optional services are absent, not no-op implementations.
- Service method metadata has one owner. Do not copy builtin IDs, argument
  rules, method names, render safety, or capability inference across crates.
- Essential crates must build without a heap. Do not introduce `Vec`, `String`,
  `Box`, allocation-backed futures, or unbounded queues into default embedded
  paths.
- First-party platform implementations use Embassy, but shared interfaces may
  not expose Embassy types.
- Preserve non-reentrant app execution. Awaited handlers suspend; other
  foreground events queue.
- Use canonical APIs only. Remove obsolete syntax directly and do not add
  aliases, deprecation warnings, migration tests, or old-name diagnostics.
- Before deleting or deferring a real public/documented syntax, API, transport,
  configuration model, or architecture option, add or update its `ICEBOX.md`
  entry with rationale, concrete revival conditions, and surviving parts. Do
  not preserve fake or never-implemented syntax as a historical contract.
- GPIO remains available through generic `hardware.gpio` calls using typed
  `target.gpio.<name>` board symbols. Do not add generic app-facing ADC, PWM,
  I2C, SPI, display-controller, or storage-controller APIs in this refactor;
  configure those devices with target-selected drivers and expose services.
- Add or update a focused example whenever a public language construct or
  service method is delivered.

## Task 0: Activate the Plan and Freeze Evidence

- [x] Record the approved architecture in the design spec.
- [x] Record this implementation checklist in the repository plan tracker.
- [x] Record the target-authoring assistant follow-up wording with the user.
- [x] Record the initially known removed/deferred platform alternatives in
  `ICEBOX.md`: app-owned device configuration, source exact targets, service
  namespace aliases, generic raw ADC/PWM/I2C/SPI APIs, per-target firmware/
  tooling branches, independent TypeScript runtime semantics, and no-op
  optional service backends.
- [ ] Update `.current_agent_work` for Task 0 before executing it.
- [ ] Confirm the working tree and current commit; record only the concise
  status in `.current_agent_work`.
- [ ] Probe the current Rust toolchains, Node toolchain, X4 serial device,
  Bluetooth controller, Wi-Fi access path, and webcam using local guidance.
- [ ] Run and save the current software baseline:

  ```sh
  RUSTUP_TOOLCHAIN=stable cargo test
  (
    cd simulator/browser
    npm test
    npm run build
    npm run test:e2e
  )
  ```

- [ ] Run the complete current X4 hardware inventory sequentially through
  `squidc hardware test`, with the probed target and port.
- [ ] Capture and inspect a fresh live panel image from a full example app.
- [ ] Classify every failure as pre-existing, environment/tooling, stale test,
  or product defect. Fix a broken safety-net baseline before continuing unless
  a concrete blocker is documented and approved.
- [ ] Save a concise verified-feature inventory in ignored scratch evidence.
  Include language/SQBC/runtime services, host workflows, X4 hardware paths,
  and simulator behavior. This inventory decides what Task 1 must preserve.
- [ ] Create a removal ledger from actual parser acceptance, public docs,
  examples, SQBC builtins, runtime hosts, and target configuration. For each
  planned deletion, point to an existing ICEBOX entry or add one before the
  deletion task begins. Do not infer public support solely from stale tests.
- [ ] Commit no product changes in Task 0. If documentation-only activation is
  committed, run `git diff --check` first.

**Gate:** Known-green software and X4 baseline, or an explicitly approved list
of pre-existing blockers. No architecture code starts before this gate.

## Task 1: Create Canonical Shared Models

### 1A. Service model

- [ ] Update `.current_agent_work` for Task 1A.
- [ ] Create `crates/squid-service-model` as `#![no_std]` with no default
  allocator dependency.
- [ ] Define stable typed enums/newtypes for capability IDs, service IDs,
  method IDs, resource kinds, value types, execution classes, ownership, and
  render safety.
- [ ] Define descriptors for every **verified** current public method. Keep
  incomplete methods out of the canonical table until their module slice is
  implemented.
- [ ] Assign each method exactly one execution class:
  `Immediate`, `Awaited`, `EventOperation`, or `Subscription`.
- [ ] Represent method signatures and result-record layouts without `serde_json`
  or heap allocation in the embedded model. Add host-only serialization behind
  a feature when needed for tooling/docs.
- [ ] Add lookup tests proving namespace/name and numeric ID round trips,
  uniqueness, capability inference, and operation-class metadata.
- [ ] Add compile checks for default no-heap and host feature configurations.

### 1B. Bytecode model

- [ ] Update `.current_agent_work` for Task 1B.
- [ ] Create `crates/squid-bytecode` and move opcode, section, builtin, header,
  reader, validation, and capability-metadata ownership into it.
- [ ] Keep decoding/validation `no_std` and allocation-free. Gate encoding and
  host diagnostics behind host/alloc features.
- [ ] Move conformance fixtures to this crate's ownership. Preserve wire-format
  equality tests and malformed/truncated/budget validation tests.
- [ ] Make both old compiler and old VM consume the new constants before moving
  further code. Delete their private definitions in the same slice.
- [ ] Verify compiler-produced SQBC byte equality and current VM execution.

### 1C. Target model skeleton

- [ ] Update `.current_agent_work` for Task 1C.
- [ ] Create `crates/squid-target-model` with typed source/resolved target
  structures, validation diagnostics, and code-backed platform/driver catalogs.
- [ ] Generate JSON Schema from the typed model and add a drift test. The Rust
  model is authoritative; generated schema is an editor/tooling artifact.
- [ ] Add fixture-only target profiles for headless, small monochrome display,
  LCD, absent radio, exported GPIO symbols, and invalid configurations.
- [ ] Do not migrate the real X4 build yet; make the new parser validate a copy
  of current X4 facts in tests first.

**Verification:** Focused tests for all three new crates, full `cargo test`,
SQBC golden equality, and `git diff --check`.

**Commit boundary:** Shared models compile and existing behavior consumes the
new bytecode constants. No target build behavior changes yet.

## Task 2: Replace the Target Schema and Build Planner

- [ ] Update `.current_agent_work` for Task 2.
- [ ] Define the resolved target fields from the design spec. Do not add JSON
  inheritance, per-target code paths, arbitrary build commands, or handwritten
  generated interfaces.
- [ ] Add the first platform catalog entries:
  - `embassy-esp32c3`, selecting the reusable ESP firmware package, Cargo
    target/features, image adapter, and espflash tooling;
  - `host`, selecting the deterministic host executable.
- [ ] Add driver entries for every driver required by the verified X4 baseline:
  X4/SSD1677 display path, SD storage, internal flash storage, ADC ladder input,
  power/USB detection, GPIO, Wi-Fi, BLE, and BinBook adapters. The X4 target
  must select the correct SSD1677/panel driver and bind its provided display
  service; generic code must not infer that driver from the target ID.
- [ ] Move platform-owned build/tool fields out of X4 JSON. Keep X4-owned
  wiring, partitions, devices, drivers, service bindings, GPIO exports, caps,
  layouts, and verification suites
  in target JSON.
- [ ] Implement semantic validation for IDs, pins, directions, buses, driver
  compatibility, service bindings, GPIO exports, memory budgets,
  partitions, and verification entries.
- [ ] Replace CLI's partial `TargetDefinition` and hardcoded ESP32-C3 partition
  validation with `squid-target-model`.
- [ ] Make `target list/inspect/doctor` consume the resolved model and report
  catalog, capability, memory, and validation diagnostics.
- [ ] Make `target build/flash/monitor --print-plan` resolve platform tooling
  from the catalog. Preserve the current X4 command behavior through generic
  planning; no `if target.id == "xteink-x4"` remains in generic planning.
- [ ] Pass the canonical target path to the reusable platform build and generate
  constants into `OUT_DIR`.
- [ ] Update target Markdown generation to consume the same resolved model or
  its canonical source fields. Regenerate X4 docs; do not hand-edit tables.
- [ ] Build and flash X4 through the new generic planner.
- [ ] Run target inspect/doctor tests, X4 build/flash smoke, app runtime smoke,
  display smoke with fresh capture, input smoke, and storage smoke.

**Gate:** X4 target JSON alone configures the existing firmware behavior through
generic target tooling. Host/fixture targets validate. No target-specific Rust
crate or CLI branch exists.

## Task 3: Move the Compiler to Typed Contracts

### 3A. Typed AST and IR

- [ ] Update `.current_agent_work` for Task 3A.
- [ ] Move shared crates to the target workspace layout only when imports are
  mechanically understood and tests are green. Preserve git history with
  ordinary moves; do not combine this with semantic changes.
- [ ] Split compiler tests by owning modules instead of retaining one monolithic
  test file.
- [ ] Replace `serde_json::Value` in AST/IR expressions and options with typed
  enums/records and target GPIO references. Serialization belongs at explicit
  IR/debug boundaries.
- [ ] Make builtin parsing and semantic lookup use `squid-service-model`.
- [ ] Preserve current verified diagnostics unless the API is intentionally
  replaced below.

### 3B. Portable target semantics

- [ ] Remove exact target syntax from the app declaration, AST, IR, fixtures,
  examples, docs, formatter, and diagnostics.
- [ ] Remove target mismatch handling and target IDs from portable SQBC app
  metadata.
- [ ] Keep two compiler entry paths:
  - portable compile with no target profile;
  - compile plus compatibility validation with a resolved target.
- [ ] Infer a typed capability/resource demand set from service calls, events,
  target GPIO references, storage use, triggers, and operations.
- [ ] Preserve demand in SQBC and expose it through inspect tooling.
- [ ] Add target diagnostics at source spans for unavailable methods, logical
  keys, GPIO export names/directions, display options, and other statically
  known incompatibilities.
- [ ] Prove that checking the same source against two compatible targets emits
  semantically identical SQBC.

### 3C. Headless apps and canonical APIs

- [ ] Stop inserting an implicit `main` screen.
- [ ] Permit apps with no screens and no display capability.
- [ ] Remove display namespace sugar and every other accepted duplicate spelling
  found by the API audit. Update source directly; add no aliases or removed-form
  regression tests. Confirm the corresponding ICEBOX concept is current before
  deletion.
- [ ] Normalize standard namespaces according to the design spec and repository
  namespace guidance.
- [ ] Delete unsupported public APIs from parser/semantic/docs when they lack a
  verified implementation owner. Before deletion, record every real documented
  concept in ICEBOX with rationale, revival conditions, and surviving pieces;
  never memorialize fake syntax as a compatibility surface.

### 3D. Board symbols and target-owned device configuration

- [ ] Delete the app-level `device {}` declaration from lexer/parser, AST/IR,
  semantics, formatter, SQBC, runtime activation, examples, and docs.
- [ ] Delete `.sqdevice` parsing, packaging, resource activation, fixtures, and
  docs in the same completed migration slice. Add no replacement app manifest
  or device-configuration language. Keep the approved ICEBOX entry aligned with
  the actual surviving target/service behavior.
- [ ] Add typed target board symbols for app-visible GPIO using the fixed
  expression shape `target.gpio.<name>`.
- [ ] Require exported GPIO names to be valid SquidScript identifiers. Resolve
  them to target pin, direction, electrical policy, and compact GPIO ID during
  target checking.
- [ ] For targetless compilation, retain the symbolic GPIO name and required
  access mode in SQBC demand metadata. Firmware resolves and validates it before
  app start.
- [ ] Make `hardware.gpio.read/write/toggle` accept only typed target GPIO
  symbols. Reject unexported raw pin strings, wrong direction, unsafe pins, and
  unsupported electrical modes.
- [ ] Make target-selected drivers provide semantic services automatically.
  Apps use `service.display`, storage, input, radio, and document APIs without
  binding physical devices.
- [ ] Require target JSON to select an explicit default when multiple configured
  drivers can provide the same service.

### 3E. Await syntax

- [ ] Add the `await` expression/statement marker.
- [ ] Use method descriptors to require or forbid `await` and report the method
  and expected operation style in diagnostics.
- [ ] Reject `await` in render-pure contexts and other contexts where suspension
  violates lifecycle rules.
- [ ] Add focused positive/negative compiler fixtures and formatter round trips.

**Verification:** Compiler unit/fixture tests, bytecode conformance, targetless
and target-checked CLI tests, negative app tests, all existing examples updated
to current syntax, full `cargo test`, and X4 app compile/package smoke.

**Commit rule:** Do not commit a language form before compiler, SQBC, VM
validation, docs, and at least one focused example agree on it.

## Task 4: Generalize VM Suspension and Service Dispatch

- [ ] Update `.current_agent_work` for Task 4.
- [ ] Replace the storage-specific resume state with a typed `PendingRequest`
  carrying service/method ID, owner, request data, deadline, expected result,
  and cancellation state.
- [ ] Allow exactly one pending awaited request for the active handler.
- [ ] Make VM dispatch return one of: completed value, pending request, emitted
  immediate effect, normal handler completion, or structured VM error.
- [ ] Add a resume entrypoint that validates request identity and result type,
  resumes once, and rejects stale/duplicate completions.
- [ ] Keep event dispatch blocked while a handler is running or suspended.
  Events continue entering the bounded firmware queue.
- [ ] Add deterministic timeout, explicit cancellation, app-exit cancellation,
  crash cleanup, and target-replacement cleanup.
- [ ] Split VM host contracts by service/resource module. Remove the giant
  implicit host surface and Wi-Fi-specific VM implementation policy.
- [ ] Validate SQBC capability/resource demand before creating the VM session.
- [ ] Add host tests for completion, timeout, cancel, duplicate completion,
  wrong result type, queued events, trigger arrival during suspension, exit,
  and crash.
- [ ] Run no-default-feature/no-allocator checks and explicit RAM budget tests.

**Gate:** Storage behavior still works through the generic request mechanism;
no other service is migrated until this is green.

## Task 5: Split and Migrate the Runtime

Create `crates/squidscript-runtime`. Move behavior by ownership, never by file
size alone.

### 5A. Lifecycle and event core

- [ ] Update `.current_agent_work` for Task 5A.
- [ ] Move foreground lifecycle, return stack, dormant routes, pending event
  queue, app-store coordination, and start reasons into focused modules.
- [ ] Preserve drop-newest overflow and diagnostic retention.
- [ ] Implement the approved dormant trigger sequence: finish handler, exit,
  push, fresh triggered start, fresh return.
- [ ] Keep firmware event producers independent of active VM state; they enqueue
  bounded logical events rather than reading raw hardware in the VM loop.
- [ ] Run lifecycle unit tests and X4 launch/exit/trigger/planned-sleep tests.

### 5B. Core service registry

- [ ] Update `.current_agent_work` for Task 5B.
- [ ] Add explicit service registration/composition based on target-generated
  configuration and typed contracts.
- [ ] Represent absent optional services as absent. Delete `Noop*Backend` and
  default unsupported implementations that make a capability appear present.
- [ ] Separate state, timer, input, display, storage, power, app lifecycle,
  hardware, radio, upload, and BinBook adapters.
- [ ] Define foreground ownership and cleanup for every service resource.
- [ ] Add resource metrics and debug instrumentation under `debug_assertions`.

### 5C. Heap-free floor

- [ ] Update `.current_agent_work` for Task 5C.
- [ ] Audit all default runtime storage. Replace hidden allocation with fixed
  arrays, heapless collections, program references, caller-provided buffers, or
  explicit optional allocator modules.
- [ ] Derive caps from target/runtime metadata or shared constants; do not add
  duplicated exact values in tests.
- [ ] Add default no-allocator build/test jobs and budget-bound tests.

**Gate:** Host runtime tests cover every migrated service seam, and the X4
lifecycle/state/input/display baseline remains green.

## Task 6: Convert X4 Firmware into the Reusable ESP Platform

Do these sub-slices sequentially. Build, flash, test, and measure after each
one. Never combine two independent hardware migrations in one commit.

### 6A. Platform shell

- [ ] Update `.current_agent_work` for Task 6A.
- [ ] Create `platforms/esp` with reusable startup, Embassy executor, clocks,
  allocator option, serial/diagnostics, generated-target inclusion, and service
  assembly.
- [ ] Move X4 board facts to target JSON/generated config. Keep only reusable
  ESP32-C3 HAL policy in the platform.
- [ ] Rename package/binary away from X4 without adding a compatibility binary.
- [ ] Verify generic target build, recovery flash, boot, identity, diagnostics,
  and app runtime.

### 6B. Buses, storage, and app state

- [ ] Update `.current_agent_work` for Task 6B.
- [ ] Build internal SPI, ADC, GPIO, and storage driver factories around
  embedded-hal contracts and generated target configuration. These factories
  serve registered drivers; they do not create general SquidScript bus APIs.
- [ ] Migrate shared SPI ownership, SD storage, internal flash, app store, state,
  and file services without X4 constants in shared runtime code.
- [ ] Test SD present/absent/reinsert, internal fallback, state persistence,
  install/run, byte-exact serial transfer, and cleanup.

### 6C. Display and input

- [ ] Update `.current_agent_work` for Task 6C.
- [ ] Register the X4 SSD1677/panel display driver and ADC/power input drivers
  through catalogs and generated config.
- [ ] Keep controller/panel and physical gesture details inside drivers/target
  bindings; runtime receives logical display/input contracts.
- [ ] Test all physical keys and gestures, composed display operations, full and
  fast refresh, dynamic text, grayscale paths, and lifecycle redraw.
- [ ] Capture and inspect a fresh native-resolution webcam image for each visual
  acceptance slice.

### 6D. Generic GPIO escape hatch

- [ ] Update `.current_agent_work` for Task 6D.
- [ ] Implement target-exported GPIO resolution and generic
  `hardware.gpio.read/write/toggle` operations.
- [ ] Keep GPIO operations bounded and nonblocking. Reject wrong direction,
  unsafe or unexported pins, conflicting driver ownership, unsupported pull or
  drive modes, and runtime HAL failure.
- [ ] Keep ADC, PWM, I2C, SPI, display controller, storage controller, and radio
  access internal to registered drivers. Do not expose them merely because the
  ESP HAL supports them.
- [ ] Add host mocks plus X4-safe GPIO examples/tests. Do not exercise
  destructive or electrically unsafe endpoint combinations.

### 6E. Radio, upload, and transport services

- [ ] Update `.current_agent_work` for Task 6E.
- [ ] Migrate Wi-Fi, BLE, upload, HTTP, and coexistence into explicit optional
  modules selected by target capabilities.
- [ ] Keep operations nonblocking and foreground-owned. Apply the standard
  start/cancel/status and event naming rules.
- [ ] Keep the patched radio dependency isolated to the ESP platform.
- [ ] Verify scan, station/AP, DHCP, HTTP upload, BLE transfer, cancellation,
  foreground cleanup, repeated radio reuse, and coexistence on hardware.

### 6F. BinBook extension

- [ ] Update `.current_agent_work` for Task 6F.
- [ ] Add a typed optional BinBook service adapter outside VM/runtime core.
- [ ] Replace sibling paths with exact Git revisions and document a local
  `[patch]` workflow for joint development.
- [ ] Preserve bounded file access, handles, decompression buffers, drawables,
  chapter navigation, and content validation.
- [ ] Verify strict fixtures, SD/fallback libraries, reader selection, BW/GRAY2
  rendering, navigation, failure records, and live page display.

**Final Task 6 gate:** The reusable ESP platform contains no product wiring;
X4 JSON is the sole X4 assembly artifact; complete X4 hardware inventory and
fresh panel evidence pass.

## Task 7: Modularize CLI and Device Protocol

- [ ] Update `.current_agent_work` for Task 7.
- [ ] Move the CLI package to `crates/squidc` after behavior is characterized.
- [ ] Keep `main.rs` limited to argument definitions, top-level dispatch, exit
  status, and output selection.
- [ ] Separate command modules and application services for app, device,
  transport, protocol, target, hardware verification, and diagnostics.
- [ ] Remove hardcoded X4 test inventory. Resolve target-owned verification
  suites from validated metadata and registered runner kinds.
- [ ] Preserve targetless `app build/check/package`. Apply target validation
  when `--target` is supplied and before concrete install/run/device actions.
- [ ] Split `squid-device-protocol` into framing, field codec, messages,
  sessions, transfer, and diagnostics modules without changing verified wire
  behavior accidentally.
- [ ] Keep protocol constants in one module and exact wire fixtures at the
  protocol boundary.
- [ ] Make transport availability follow device/target capabilities rather than
  X4 assumptions.
- [ ] Run CLI parsing, JSON output, protocol wire, REPL, app-test, target-plan,
  serial, HTTP, BLE, and complete hardware-runner tests.

**Gate:** No generic CLI command branches on X4 identity. Existing public
workflows operate through generic target/device capabilities.

## Task 8: Deliver the Host Platform and Generic Browser Simulator

### 8A. Host platform

- [ ] Update `.current_agent_work` for Task 8A.
- [ ] Create `platforms/host` as a real executable target using the same runtime,
  target model, SQBC, service contracts, and lifecycle.
- [ ] Provide deterministic in-memory or temporary-directory implementations
  for supported services and explicit absence for unsupported services.
- [ ] Provide controllable display, input, clock, storage, exported GPIO, and
  pending-operation mocks with observable call logs.
- [ ] Test display and headless target profiles, capability rejection, service
  call sequences, timeouts, triggers, and cleanup.

### 8B. Browser simulator

- [ ] Update `.current_agent_work` for Task 8B.
- [ ] Replace direct X4 imports and handwritten partial target interfaces with
  validated bundled target/profile data derived from the target model.
- [ ] Add profile selection for at least X4 display and a headless/reduced
  capability host profile.
- [ ] Derive device shell, display surfaces, logical buttons, and supported
  service panels from target/layout metadata.
- [ ] Remove X4 names from generic UI labels, accessibility labels, runtime
  state, renderer assumptions, and tests.
- [ ] Move compiler and VM semantics behind shared Rust/WASM entrypoints.
  TypeScript remains responsible for UI, IndexedDB/browser storage, canvas,
  and simulated host service adapters.
- [ ] Display explicit unsupported diagnostics for hardware-only behavior.
- [ ] Run unit tests, production build, and Playwright checks for display and
  headless profiles.

**Gate:** Host and browser execute portable artifacts without X4 policy in
shared code. Browser does not claim real radio/hardware behavior.

## Task 9: Build the X4 Feature Example Regression Matrix

### 9A. Inventory and conventions

- [ ] Update `.current_agent_work` for Task 9A.
- [ ] Inventory existing full examples, app-test fixtures, negative fixtures,
  REPL sessions, and X4 hardware apps. Assign each one an owning feature and
  natural test boundary.
- [ ] Define one examples index with: feature ID, full example path, target
  capability, automated boundary, deterministic companion when present, and
  live-hardware/visual requirement.
- [ ] Keep full examples useful and readable. Keep deterministic app-test
  companions small. Do not make compiler-core tests include example files.
- [ ] Keep all source target-neutral; regression commands supply
  `--target xteink-x4`.

### 9B. Core language examples

- [ ] Add or revise focused examples for literals/types, expressions, control
  flow, bounded loops, functions/modules, typed state, result records, errors,
  debug/release behavior, and headless execution.
- [ ] Add negative fixtures only for current generic invalid behavior. Never
  name removed syntax or aliases in migration tests.
- [ ] Test through `squidc app test`, host target, and compiler fixtures at the
  layer that owns the contract.

### 9C. Lifecycle and input examples

- [ ] Add or revise examples for app start/exit/launch/return, foreground
  timers, dormant timers, dormant input triggers, queued delivery, planned
  sleep/wake, logical key input, long press, double tap, and chords.
- [ ] Use X4 hardware sessions for physical input and power. Check actual state
  before commands and keep the suite sequential.

### 9D. Display examples

- [ ] Add focused examples for text, dynamic values, rectangles/lines,
  composition order, clipping, multiple screens, grayscale, refresh modes,
  partial/full behavior, and returned drawables.
- [ ] Test portable rendering in browser/host and physical rendering with full
  examples on X4. App-test fixtures may assert routing but cannot substitute
  for visual evidence.

### 9E. Storage, hardware, transport, and BinBook examples

- [ ] Add focused examples for persistent state, file reads/lists/copies,
  libraries, removable/fallback storage, target-exported generic GPIO, Wi-Fi
  operations, BLE/upload lifecycle, serial/HTTP/BLE transfers, BinBook
  discovery/info/navigation/page drawing, and dynamic failure handling.
- [ ] Never expose credentials or local identifiers in examples or evidence.
- [ ] Keep unsafe hardware access out of the example matrix; use only target
  endpoints explicitly designated for app testing.

### 9F. Discovery and regression runner

- [ ] Add a deterministic inventory check that discovers every indexed example,
  validates its files/test assets, compiles it at the declared boundary, and
  reports skipped hardware prerequisites explicitly.
- [ ] Make software-only example regression part of the normal repository check.
- [ ] Make X4 example hardware regression part of the target-owned sequential
  hardware inventory.
- [ ] Require future public language/service changes to update the examples
  matrix and owning regression test as a definition-of-done rule.

**Gate:** Every implemented public language construct and X4 capability is
covered by a focused example or an explicitly indexed larger example, with a
green automated or hardware-owned boundary.

## Task 10: Delete Superseded Architecture and Align Documentation

- [ ] Update `.current_agent_work` for Task 10.
- [ ] Delete old crate locations after workspace consumers use the new paths.
- [ ] Delete `.sqdevice`, old target loaders, duplicated builtin/capability
  tables, no-op optional backends, X4 generic constants, retired-board fixture
  identities, sibling BinBook paths, and duplicate TypeScript runtime semantics.
- [ ] Reconcile the Task 0 removal ledger against the final diff. No removed
  public/documented concept may exist only in chat or commit history; update
  ICEBOX before its deletion commit when the inventory finds an omission.
- [ ] Search deleted basenames and removed APIs across docs, README, roadmap,
  examples, scripts, tests, and related commit messages; fix active references
  in the same commit.
- [ ] Replace the current BinBook capability JSON as an authoritative source.
  Generate service/capability reference data from typed descriptors.
- [ ] Update current-state reference docs for language, philosophy, SQBC,
  targets, runtime limits, lifecycle, firmware build/storage, CLI, protocol,
  browser simulator, fixture ownership, examples, and developer workflow.
- [ ] Correct known drift: X4-only simulator framing, outdated production SQBC
  claims, ignored target fields, old firmware file references, and language
  wording that denies the chosen general embedded-device purpose.
- [ ] Remove roadmap entries completed or invalidated by the refactor. Preserve
  unrelated work and move only genuinely speculative follow-ups to ICEBOX with
  the required rationale/revival/surviving-parts record.
- [ ] When a documentation commit touches more than three doc files, include
  `doc snapshot — re-validate after next refactor` in its commit message.

## Task 11: Final Verification and Completion

- [ ] Update `.current_agent_work` for Task 11.
- [ ] Run formatting and `git diff --check`.
- [ ] Run the complete Rust workspace test suite, explicit no-default-feature
  embedded checks, SQBC conformance, target-schema/catalog tests, CLI/REPL/app
  tests, and software-only example inventory.
- [ ] Build all executable target profiles, including host and X4.
- [ ] Run browser unit tests, WASM build, production build, and E2E tests for
  display and headless profiles.
- [ ] Probe X4 and host radio state; run the complete X4 hardware inventory
  sequentially on the final image.
- [ ] Run the X4 feature-example hardware matrix, including storage absent/
  present cases and radio coexistence.
- [ ] Confirm final lifecycle/resources/errors are idle and clean.
- [ ] Take and inspect a fresh native-resolution webcam capture from a full
  SquidScript display example and report the capture path to the user.
- [ ] Run repository-wide searches for active X4 coupling in generic layers,
  `.sqdevice`, target source clauses, duplicate builtin IDs, no-op capability
  backends, sibling BinBook paths, and obsolete docs. Classify every remaining
  match as target-owned, historical plan/spec, fixture data, or defect.
- [ ] Audit every removal against ICEBOX and confirm each entry still states the
  actual rationale, revival trigger, and surviving implementation after the
  refactor.
- [ ] Confirm every checked task has evidence and every unchecked task is either
  intentionally out of scope or recorded as an approved roadmap/icebox item.
- [ ] Update `.current_agent_work` to the final handoff state.
- [ ] Commit only fully verified slices. Do not collapse the implementation into
  one giant final commit.

## Final Acceptance Criteria

- [ ] Target JSON is the only per-target artifact and X4 has no target crate.
- [ ] Generic layers contain no X4 policy or retired-board identity.
- [ ] Service/method IDs, signatures, operation modes, and capability inference
  have one typed source of truth.
- [ ] SQBC definitions have one source of truth and portable SQBC embeds demand,
  not exact target identity.
- [ ] Targetless and target-checked compilation both work.
- [ ] Headless apps, target-exported GPIO, driver-provided services, explicit
  await, and hybrid event operations work end to end.
- [ ] The runtime is non-reentrant, awaited calls are nonblocking, and dormant
  triggers perform deferred stack handoff.
- [ ] Essential VM/runtime paths require no heap.
- [ ] Host and browser provide honest generic target execution.
- [ ] BinBook is an optional first-party module consumed reproducibly through
  pinned Git dependencies.
- [ ] Verified X4 display, input, power, storage, radio, upload, lifecycle,
  transport, and BinBook behavior remains green.
- [ ] The indexed X4 SquidScript feature examples act as executable regression
  coverage and are documented for users.
- [ ] Current reference documentation matches the implemented architecture.
- [ ] ICEBOX records every substantive removed or deferred public platform idea
  with rationale, revival conditions, and surviving parts.
