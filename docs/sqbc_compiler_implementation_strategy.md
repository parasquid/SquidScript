# SquidScript Compiler Implementation Strategy

Status: Draft
Purpose: Define the recommended implementation-language path for the SquidScript compiler, `squidc`, and SQBC bytecode tooling.

---

## 1. Purpose

The SquidScript compiler should be developed in two implementation phases:

1. A Ruby reference prototype for fast language and behavior iteration.
2. A Rust production compiler for long-term correctness, CLI distribution, and browser/WASM workflows.

The goal is to avoid premature implementation friction while still ending with a robust compiler suitable for real use.

`squidc` is the compiler. SQBC is the bytecode format emitted by the compiler.

Firmware build orchestration, backend selection, generated firmware artifacts, and simulator backend policy are described in:

```text
docs/firmware_build_architecture.md
```

---

## 2. Core Decision

The SquidScript compiler should not start directly as a Rust-only compiler.

Instead:

- Ruby should be used first to explore syntax, semantics, diagnostics, target-definition behavior, binary layout, and sourcemap shape.
- Rust should be introduced once the core language behavior and target model are stable enough to justify a production implementation.
- The final authority should be the written specification plus golden fixtures, not the Ruby prototype itself.

Recommended hierarchy:

1. Written SquidScript and SQBC specifications are canonical.
2. Golden fixtures are canonical behavioral evidence.
3. Ruby is the first executable reference prototype.
4. Rust is the production compiler.

Ruby should be treated as a reference prototype, not as the permanent canonical compiler.

---

## 3. Why Ruby First

Ruby is useful at the beginning because SquidScript and SQBC are still design-heavy.

The early compiler will likely need frequent changes to:

- source syntax
- AST shape
- validation rules
- target-definition schema
- integrated target model
- diagnostics
- binary format details
- sourcemap format
- CLI behavior
- app package and registry integration

Ruby allows these details to change quickly with minimal friction.

Ruby also matches the developer's existing strengths, making it suitable for fast experimentation and readable prototype code.

---

## 4. Why Rust Later

Rust is the recommended final implementation language because `squidc` needs:

- strict binary format handling
- reproducible compiler output
- strong modeling of invalid states
- cross-platform CLI builds
- browser/WASM support
- long-term maintainability
- structured diagnostics
- safe refactoring as the language grows

Rust is especially appropriate once the compiler has a stable model for:

- target aliases
- board profiles
- display profiles
- input profiles and button maps
- storage profiles
- power profiles
- runtime profiles
- display bit-depth constraints
- binary package generation
- sourcemaps
- diagnostics
- app registry metadata

Rust should become the production compiler because it is better suited to correctness-critical and distribution-critical work.

---

## 5. Why Ruby Should Not Be Permanently Canonical

Ruby should not be the permanent source of truth for SquidScript or SQBC behavior.

If Ruby remains canonical forever, the Rust compiler may be forced to imitate accidental Ruby behavior, including implementation quirks that were never intended as language semantics.

Instead, Ruby should be used to discover behavior, and once behavior is accepted, it should be moved into:

- written specification sections
- fixture files
- expected diagnostics
- expected IR outputs
- expected binary outputs
- expected sourcemap outputs

The Rust compiler should match the specification and golden fixtures, not blindly match the Ruby implementation.

---

## 6. Terminology

Preferred terms:

- SquidScript Ruby Reference Prototype
- `squidc` Ruby Reference Prototype
- Rust production `squidc`

Avoid:

- SQBC Canonical Ruby Compiler
- SQBC Official Ruby Compiler
- SQBC Master Implementation
- Official Ruby `squidc`

The Ruby version demonstrates intended behavior during design and helps produce golden fixtures.

The Rust version is the production compiler.

---

## 7. Repository Layout

Recommended repository structure:

```text
compiler/
  spec/
    SQUIDSCRIPT_LANGUAGE_SPEC.md
    SQBC_BINARY_FORMAT.md
    SQBC_TARGET_PROFILES.md
    SQBC_DIAGNOSTICS.md
    SQBC_SOURCEMAP.md
    SQBC_LAUNCHER_INTEGRATION.md

  fixtures/
    manifest.toml

    valid/
      hello_world.squid
      xteink-x4_hello_menu.squid
      simple_buttons.squid
      binbook_reader.squid

    invalid/
      invalid_4bit_on_x4.squid
      missing_target.squid
      unknown_button.squid
      unsupported_display_mode.squid

    targets/
      xteink-x4.target.json
      browser-sim-xteink-x4.target.json

    layouts/
      xteink-x4.layout.json

    expected/
      hello_world.ir.json
      hello_world.diagnostics.json
      hello_world.sourcemap.json
      hello_world.sqbc

      xteink-x4_hello_menu.ir.json
      xteink-x4_hello_menu.diagnostics.json
      xteink-x4_hello_menu.sourcemap.json
      xteink-x4_hello_menu.sqbc

      binbook_reader.ir.json
      binbook_reader.diagnostics.json
      binbook_reader.sourcemap.json
      binbook_reader.sqbc

      invalid_4bit_on_x4.diagnostics.json

    resources/
      sample.binbook

  ruby/
    squidc_ref/
      lexer.rb
      parser.rb
      ast.rb
      validator.rb
      profile_resolver.rb
      ir.rb
      encoder.rb
      sourcemap.rb
      diagnostics.rb
      cli.rb

  rust/
    crates/
      squidc-core/
        src/
          ast.rs
          lexer.rs
          parser.rs
          diagnostics.rs
          profile.rs
          validate.rs
          ir.rs
          encode.rs
          sourcemap.rs
          lib.rs

      squidc-cli/
        src/
          main.rs

      squidc-wasm/
        src/
          lib.rs

  tools/
    compare_outputs.rb
    update_golden.rb
    check_fixture_manifest.rb
```

---

## 8. Canonical Artifacts

The canonical artifacts should be:

1. The specification documents.
2. The input fixtures.
3. The expected output fixtures.
4. The fixture manifest.

The Ruby implementation should generate expected outputs early in the project.

The Rust implementation should later reproduce those outputs.

Important expected outputs:

- diagnostics JSON
- IR JSON
- sourcemap JSON
- binary package output
- profile-resolution output
- SQBC app metadata output

IR means intermediate representation: the compiler's normalized internal form between parsed source and final SQBC bytecode.

IR JSON should be treated as an internal compiler test artifact unless an external consumer is explicitly introduced. Its schema can change directly before 1.0.

Binary fixtures should be byte-exact for a fixed source set, fixed target definition, and current compiler/runtime behavior.

When the SQBC binary format intentionally changes before 1.0, update the compiler, runtime, firmware, docs, and fixture expectations together. Rebuild stale bytecode artifacts instead of preserving old formats.

---

## 9. Golden Fixture Strategy

Every meaningful feature should have fixtures.

Fixtures should be listed in `fixtures/manifest.toml` instead of relying only on filename conventions.
Repository fixture ownership is tracked in `docs/fixture_ownership.md`.

For each fixture, the manifest should record:

- source files
- entrypoint
- target definition
- expected success or failure
- expected output files
- current SQBC layout

A valid fixture should usually include:

- `.squid` source file
- target definition
- expected diagnostics
- expected IR
- expected `.sqbc` binary output
- expected sourcemap

Integration fixtures should include a minimal BinBook reader app compiled to `.sqbc` plus a sample `.binbook` resource. These fixtures should validate API calls, target-feature expectations, and compile-time expectations without merging the SQBC and BinBook formats.

An invalid fixture should usually include:

- `.squid` source file
- target definition
- expected diagnostic codes
- expected diagnostic spans
- expected help messages, where applicable

Example fixture group:

```text
fixtures/
  manifest.toml

  valid/
    hello_world.squid

  expected/
    hello_world.ir.json
    hello_world.diagnostics.json
    hello_world.sourcemap.json
    hello_world.sqbc
```

Example invalid fixture group:

```text
fixtures/
  manifest.toml

  invalid/
    invalid_4bit_on_x4.squid

  expected/
    invalid_4bit_on_x4.diagnostics.json
```

---

## 10. Ruby Prototype Responsibilities

The Ruby prototype should be responsible for early exploration of:

- source syntax
- lexer/parser behavior
- AST structure
- semantic rules
- profile resolution
- target alias behavior
- display target checks
- page-format validation
- input and button mapping validation
- IR format
- binary encoding experiments
- diagnostic shape
- sourcemap generation

Ruby should prioritize clarity and iteration speed over performance.

Ruby code should be readable and direct.

It should avoid excessive metaprogramming, clever DSLs, or hidden behavior, because the Rust implementation will eventually need to replicate the accepted behavior.

---

## 11. Rust Production Compiler Responsibilities

The Rust implementation should be responsible for:

- production CLI compiler
- WASM compiler module
- stable binary generation
- strict profile validation
- structured diagnostics
- sourcemap generation
- current-format binary generation
- app registry integration
- reproducible builds
- cross-platform distribution

The Rust compiler should be split into clean crates:

squidc-core:

- parser
- AST
- semantic validation
- profile model
- IR
- encoder
- sourcemap generator
- diagnostics

squidc-cli:

- filesystem access
- command-line interface
- profile loading
- terminal output
- build/check/inspect commands

squidc-wasm:

- wasm-bindgen wrapper
- TypeScript bindings
- browser-safe compile interface
- Uint8Array binary output
- JSON conversion for diagnostics and sourcemaps

---

## 12. WASM Requirement

Because browser workflows are required, the production compiler core should be designed from the beginning to compile to WASM.

The squidc-core crate must not depend on:

- filesystem APIs
- process APIs
- environment variables
- terminal libraries
- network APIs
- OS-specific paths
- native threading assumptions

The compiler core should behave as a pure function:

```text
compile(input) -> output
```

Example conceptual API:

```text
compile(
  files,
  entrypoint,
  resolved_target_profile,
  options
) -> {
  binary,
  sourcemap,
  diagnostics
}
```

The CLI can load files and profiles from disk.

The browser UI should pass all files, profile data, and options explicitly into the WASM compiler.

The WASM boundary should expose explicit serializable request and response objects instead of Rust internal compiler structs.

Browser-facing requests should use virtual, normalized paths so diagnostics and sourcemaps are stable across operating systems.

---

## 13. Target Definition Model

SquidScript source should compile against the portable language/runtime API by
default. A board target is not required for normal compilation or reference
firmware upload.

Normal portable build:

```bash
squidc app build app.squid --out app.sqbc
```

Explicit target check:

```bash
squidc app build app.squid --target targets/xteink-x4.target.json --check-target --out app.sqbc
```

The compiler should load the resolved target model produced from:

```text
targets/xteink-x4.target.json
```

Integrated targets are still useful because they keep board, display, input,
storage, power, runtime, feature, firmware-update, simulator,
documentation, and autocomplete metadata in one maintainable source artifact.
They should be opt-in for explicit target-check workflows.

Split profile parts remain an optional advanced authoring mode for development-board reuse. If used, they should resolve to the same integrated target model before compiler validation.

Optional composable profile concepts:

- board profile
- display profile
- input profile
- storage profile
- power profile
- runtime profile
- target alias

Illustrative split composition:

```text
xteink-x4 =
  board: esp32c3
  display: gdeq0426t82
  input: xteink_x4_buttons
  storage: xteink_x4_sdcard
  power: xteink_x4_power
  runtime: esp32c3_lowram

xteink_x4_buttons =
  buttons:
    button_0: gpio ...
    button_1: gpio ...
    button_2: gpio ...
    button_3: gpio ...
    button_4: gpio ...
    button_5: gpio ...
    button_6: gpio ...
```

Compatibility tooling may eventually allow both:

1. compiling against an integrated target ID
2. compiling against explicit split profile components for development targets

Example CLI forms:

```bash
squidc app build app.squid --target targets/xteink-x4.target.json --check-target --out app.sqbc

squidc app build app.squid \
  --board esp32s3 \
  --display waveshare_4_2_bw \
  --input custom_buttons.toml \
  --storage dev_sdcard.toml \
  --power usb_dev_power.toml \
  --runtime esp32s3_psram.toml
```

The resolver should expand aliases and split profile parts into a fully resolved target model before validation. `squidc` should validate app requirements against that resolved model, not against partially loaded profile fragments.

---

## 14. BinBook Integration

BinBook is a separate compiled raster-book format, not part of SQBC.

The authoritative BinBook format reference is the GitHub-hosted [BinBook Format Specification](https://github.com/parasquid/binbook/blob/main/BINBOOK_FORMAT_SPEC.md).

The relationship between the formats is:

- `.squid` is SquidScript source.
- `.sqbc` is executable SquidScript bytecode produced by `squidc`.
- `.binbook` is a compiled raster-book document container produced by BinBook tooling.
- `.uf2` is a firmware replacement image produced by the firmware build, not by `squidc`.

SQBC should not embed, redefine, or reinterpret the BinBook binary file format. SquidScript apps should access BinBook documents through firmware-native capabilities.

Likewise, SQBC tooling should not package apps into UF2 images. UF2 belongs to firmware replacement for targets whose bootloader supports a drag-and-drop update flow; app installation remains a storage and app-registry concern.

The draft BinBook capability contract lives at:

```text
capabilities/binbook.cap.json
```

The contract defines the compiler-visible interface: names, signatures, return types, handle types, target features, render-safety rules, diagnostics, and symbolic builtin IDs. It is a spec/build artifact, not runtime metadata. The compiler should depend on this contract rather than on BinBook parser or decoder source code.

The preferred source API shape is:

```squid
let book = binbook.open(state.file)
if (book.ok) {
  let page = binbook.readPage(book.book, state.pageIndex)
  if (page.ok) {
    service.display.draw(page.drawable)
  }
}
```

The BinBook capability owns:

- opening and validating `.binbook` files
- reading BinBook metadata
- resolving page handles
- decoding or streaming page content
- converting BinBook page data into a display-ready drawable resource

The display capability owns final composition through `service.display.draw(...)`.

For the first X4 target family, keep these names distinct:

- `xteink-x4` is the SquidScript app/device target alias.
- `xteink-x4-portrait` is the BinBook document/display profile for rendered book pages.

For BinBook `xteink-x4-portrait` resources:

- default storage is canonical `GRAY2_PACKED`
- `GRAY1_PACKED` is allowed only when explicitly configured for fast/lower-quality output
- `GRAY4_PACKED` should not be emitted for this profile
- canonical `GRAY2_PACKED` values are `0=black`, `1=dark gray`, `2=light gray`, `3=white`

Both `.sqbc` and `.binbook` should remain binary-native runtime formats. Required runtime metadata must not be represented as JSON, CBOR, protobuf, or other dynamic serialization sections inside either format.

---

## 15. Diagnostics Contract

Diagnostics should be structured from the beginning.

Each diagnostic should include:

- severity
- code
- message
- file
- span
- optional help text
- optional related spans

Example diagnostic:

```json
{
  "severity": "error",
  "code": "SQBC_DISPLAY_BIT_DEPTH_UNSUPPORTED",
  "message": "Target xteink-x4 supports 2-bit pages only.",
  "file": "main.squid",
  "span": {
    "start": 120,
    "end": 147,
    "line": 8,
    "column": 3
  },
  "help": "Use a 2-bit page format or select a display target that supports 4-bit grayscale."
}
```

Diagnostics should be usable by:

- CLI output
- browser editor squiggles
- sourcemap/debug views
- automated tests
- AI coding agents

---

## 16. Binary Output Contract

The binary output should be treated as a current-format artifact.

The binary format should be specified separately in SQBC_BINARY_FORMAT.md.

The compiler should avoid writing binary output through implicit struct layout.

Preferred behavior:

- explicit byte writing
- explicit endianness
- explicit field widths
- explicit section offsets
- explicit checksums if used
- golden binary fixtures for current behavior
- byte-exact fixture matching for fixed inputs, target definition, and current compiler/runtime behavior

The Ruby prototype may generate early binary layouts.

Once accepted, the binary layout must be documented and captured in golden fixtures.

The Rust compiler must reproduce the documented output.

When the binary format intentionally changes before 1.0, update fixture
expectations and rebuild stale bytecode artifacts. Do not add backwards readers
or alternate old-format paths.

---

## 17. Sourcemap Contract

SQBC should generate sourcemaps so firmware/runtime errors can be mapped back to source.

The sourcemap should be stable enough to support:

- crash diagnostics
- bad page/resource references
- invalid action handlers
- debug views in browser tooling
- runtime error reporting from firmware or app tooling

Minimum sourcemap fields should include:

- source file
- source span
- generated binary section
- generated offset or object ID
- logical SQBC entity
- optional symbol name

The Ruby prototype should help discover the right shape.

The Rust compiler should eventually produce the final sourcemap format.

---

## 18. AI Coding Workflow

This project should be structured to work well with AI coding agents.

The most important rule is to give agents small, testable tasks.

Good agent task:

```text
Implement target-definition validation for display bit depth.

Context:
- Read spec/SQBC_TARGET_PROFILES.md.
- Read compiler/rust/crates/squidc-core/src/profile.rs.
- Read compiler/rust/crates/squidc-core/src/diagnostic.rs.
- Do not change public structs unless necessary.
- Add tests for invalid 4-bit pages on xteink-x4.
- Expected diagnostic code: SQBC_DISPLAY_BIT_DEPTH_UNSUPPORTED.
- Run cargo test.
```

Bad agent task:

```text
Build the compiler.
```

Recommended AI-friendly project files:

- SPEC.md
- ARCHITECTURE.md
- CONTRIBUTING.md
- FIXTURE_FORMAT.md
- PROFILE_SCHEMA.md
- CODEGEN_RULES.md

CODEGEN_RULES.md should include rules such as:

- Do not use unsafe Rust.
- Prefer simple owned AST nodes initially.
- Do not add filesystem access to squidc-core.
- Do not add native-only dependencies to squidc-core.
- All binary format changes require golden fixtures.
- All diagnostics require stable diagnostic codes.
- All new validation rules require invalid fixtures.
- Fixture tests should be declared in `fixtures/manifest.toml`.
- Capability APIs should be namespaced, such as `service.display.draw`, `state.load`, `file.pickFile`, and `binbook.open`.
- Do not introduce implicit truthiness, implicit local creation, unchecked arithmetic, mutable records, mutable lists, or unspecified evaluation order.
- Invalid source should produce diagnostics; invalid dynamic behavior should produce structured runtime errors, not undefined behavior.
- Keep modules small and explicit.
- Avoid clever macros.

---

## 19. Migration Plan

### Phase 1: Ruby Reference Prototype

Goals:

- establish basic syntax
- parse simple SquidScript files
- define initial AST
- resolve target IDs and optional aliases
- validate xteink-x4 constraints
- emit basic diagnostics
- emit basic IR
- emit experimental binary output
- emit experimental sourcemap

Exit criteria:

- at least one valid compile path
- at least one invalid diagnostic path
- at least one BinBook capability compile path
- initial xteink-x4 target definition
- initial binary header
- initial sourcemap format
- fixture directory created
- fixture manifest created

### Phase 2: Fixture Stabilization

Goals:

- freeze accepted behavior into fixtures
- write specification documents
- produce golden expected outputs
- separate intentional behavior from Ruby quirks

Exit criteria:

- valid fixtures have expected IR, binary, diagnostics, sourcemap
- invalid fixtures have expected diagnostics
- BinBook reader integration fixture has expected API calls, diagnostics, IR, binary, and sourcemap
- fixture manifest documents source files, targets, expected outputs, and expected success/failure
- profile schema documented
- binary format documented
- diagnostic format documented

### Phase 3: Rust Core Implementation

Goals:

- implement parser
- implement AST
- implement profile resolver
- implement validator
- implement IR
- implement encoder
- implement sourcemap generator
- match golden fixtures

Exit criteria:

- Rust squidc-core passes fixture tests
- output matches expected IR
- output matches expected diagnostics
- output matches expected binary files
- output matches expected sourcemaps

### Phase 4: CLI Implementation

Goals:

- implement `squidc app build`
- implement `squidc check`
- implement `squidc inspect`
- implement target/profile loading
- implement useful terminal diagnostics

Exit criteria:

- CLI can compile fixtures from disk
- CLI can inspect generated binaries
- CLI can run validation without output
- CLI passes cross-platform tests

### Phase 5: WASM Implementation

Goals:

- expose compiler through squidc-wasm
- generate TypeScript bindings
- support browser input as explicit file map
- return binary as Uint8Array
- return diagnostics and sourcemap as structured objects

Exit criteria:

- browser can compile sample SquidScript file
- browser can show diagnostics
- browser can download binary output
- browser can display sourcemap/debug information

### Phase 6: Browser Tooling

Goals:

- build playground/editor
- add target selector
- add syntax highlighting
- add diagnostics panel
- add preview/simulator if useful
- add binary download/export

Exit criteria:

- user can write SquidScript in browser
- user can select xteink-x4
- user can compile in browser
- user can inspect errors
- user can export the compiled package

---

## 20. When to Start Rust

Do not wait until the Ruby compiler is complete.

Start Rust when the following are stable enough:

- basic syntax
- target definition model
- xteink-x4 target resolution
- display bit-depth validation
- diagnostics shape
- binary header
- minimum viable IR
- minimum viable sourcemap
- fixture format
- fixture manifest format
- minimum viable BinBook capability API shape

The Rust implementation should begin once the architecture stops changing daily.

---

## 21. Long-Term Ruby Role

After Rust becomes the production compiler, Ruby can remain useful as:

- fixture generator
- golden-output updater
- conformance checker
- quick experiment harness
- readable behavior sketch
- spec exploration tool

Ruby should not remain the only implementation that defines SquidScript or SQBC behavior.

Any accepted Ruby behavior should be moved into specs and fixtures.

---

## 22. Browser Simulator Rust/WASM Path

The browser XTEINK X4 simulator brings the Rust/WASM compiler work forward for the Hello Menu subset. Ruby can still be useful as a reference-oriented prototype, but browser-sim needs a Rust compiler frontend early because the simulator's normal compile path must run in the browser.

The browser simulator compile path emits SQBC:

```text
.squid -> CST -> typed AST -> validated IR -> SQBC
```

Browser-sim installs `main.sqbc` under simulated `/sd` and runs it through
`squidvm-core`, the same shared VM crate exposed to Zephyr firmware through
`squidvm-ffi`.

---

## 23. Final Recommendation

Use Ruby first for speed.

Use it to discover SquidScript and SQBC behavior, generate examples, and produce early golden fixtures.

Do not make Ruby permanently canonical.

Make the written specification, fixture manifest, and golden fixtures canonical.

Use Rust for the production compiler, CLI, and WASM browser compiler.

Keep the compiler core pure, deterministic, fixture-tested, and browser-safe.

This gives the project a practical development path:

- Ruby for design velocity.
- Specs and fixtures for stable behavior.
- Rust for correctness and distribution.
- WASM for browser workflows.
