# Browser XTEINK X4 Simulator

Status: Draft implementation roadmap
Scope: `simulator/browser`

## Purpose

The browser simulator is a development target for SquidScript apps on the XTEINK X4 profile. It is not production firmware and it does not define a production package format.

The simulator provides:

- a React and TypeScript editor/device UI
- a Canvas-rendered XTEINK X4 display surface
- logical X4 buttons and keyboard mappings
- persistent simulated `/sd` storage
- an explicit Compile, Upload, and Run workflow
- a Rust compiler frontend path for WASM builds

## Compile, Upload, Run

The browser workflow is intentionally explicit:

1. `Compile` compiles the editor source to diagnostics plus `main.ir.json`.
2. `Upload` writes `app.json` and `main.ir.json` under `/sd/apps/<app-id>/`.
3. `Run` loads the installed app through an executable loader and starts the app lifecycle.

Editor draft source is saved separately from simulated `/sd`. A debug workflow may later upload `main.squid`, but source upload is non-authoritative.

## Browser-Only IR Entry

Browser-sim v1 installs manifests with:

```json
{
  "entry": {
    "type": "ir",
    "file": "main.ir.json",
    "browserSimOnly": true
  }
}
```

`entry.type = "ir"` is a browser-simulator development artifact. Production firmware remains bytecode-only and must not treat IR JSON as an executable firmware format.

## Loader Boundary

Runtime loading is split behind a common executable loader boundary:

```text
IrJsonLoader -> RuntimeProgram
SqbcLoader   -> RuntimeProgram
```

`IrJsonLoader` is active in browser-sim v1. `SqbcLoader` is reserved for the future bytecode runtime path. The simulator UI should not couple directly to either input format.

## Target and Layout

The simulator imports `targets/xteink-x4.target.json` for logical display dimensions, grayscale level count, supported font heights, render policy metadata, logical buttons, storage, features, and compatibility declarations.

The current target JSON has no physical simulator layout. Browser-sim therefore uses an approximate shell layout in code and marks it as placeholder. A future `squid-layout-v1` layout file should replace the approximation.

Keyboard mapping:

- arrows: `UP`, `DOWN`, `LEFT`, `RIGHT`
- `Enter`: `SELECT`
- `Backspace`: `BACK`
- `Tab`: `POWER`

## Runtime Semantics

The v1 runtime executes a validated `RuntimeProgram` with services for screen redraw, display refresh, app-scoped state under `/sd/system/app-state`, and app exit.

Rendering uses redraw-from-state semantics. Canvas rendering preserves source order, clips to the logical display, maps to the target's 16-level grayscale palette, and chooses deterministic font heights from the target definition.

Button outcomes are mutually exclusive:

- short press fires on release
- long press fires at the configured threshold and suppresses short press
- chord fires when a second key is pressed inside the chord window and suppresses component keys

## Debug Logging

Browser-sim includes an in-app debug log and mirrors the same events to `console.debug`.

Current event scopes:

- `compile`: compile start, backend, diagnostics count, app id
- `upload`: app install writes to simulated `/sd`
- `run`: manifest/executable load and runtime start
- `input`: logical key dispatch and resulting runtime state
- `state`: app-state reset
- `storage`: simulated `/sd` reset

The log is intended for simulator/runtime debugging, not for app-visible behavior.

## WASM Compiler Build

The browser app tries to load `src/compiler/wasm/squidc_wasm.js`. When that generated module is present, compile status reports `Compiler: WASM`. If it is absent, browser-sim uses the TypeScript fallback compiler and reports `Compiler: FALLBACK`.

The Rust/WASM compiler is the authoritative browser-sim compiler path. The TypeScript fallback exists only so the UI remains demonstrable when the generated WASM package is unavailable. It must stay constrained to documented SquidScript syntax and must not introduce simulator-only syntax or semantics.

Build the WASM package from `simulator/browser`:

```bash
npm run wasm:build
```

Requirements:

- `wasm-pack`
- a Rust toolchain with the `wasm32-unknown-unknown` target installed

The current Homebrew Rust toolchain in this environment does not include `wasm32-unknown-unknown`, so `npm run wasm:build` fails early with a toolchain message. The simulator remains testable through the fallback compiler until a Rustup-managed or otherwise WASM-capable Rust toolchain is available.

## Current Compiler Subset

The initial compiler subset follows the documented SquidScript app shape used by the Hello Menu fixture:

```squid
app "hello-menu" target "xteink-x4"

state {
  selected: int = 0
}

event.on("app.start") {
  state.load()
  screen.open("main")
}

event.on("key.DOWN") {
  selected = selected + 1
  state.save()
  screen.refresh()
}

screen("main", { render: "compose" }) {
  display.clear("gray0")
  display.text("Hello Menu", { x: 20, y: 60, w: 440, h: 48, fontHeight: 32, align: "center" })
}
```

This subset compiles to versioned `squidscript-ir` JSON with state defaults, event handlers, screens, and display statements.

Currently supported syntax:

- `app "id" target "target-id"`
- `state { name: type = literal }`
- `event.on("app.start") { ... }`
- `event.on("key.KEY") { ... }`
- `screen("name", { render: "compose" }) { ... }`
- `function name(...) { ... }`
- `state.load()`, `state.save()`, `state.reset()`
- `screen.open("name")`, `screen.refresh()`
- `app.exit()`
- local `let` bindings, typed local annotations, assignment, `if/else`, `repeat`, and bounded `for ... in ... max ...`
- expression calls and binary operators `+`, `-`, `==`, `!=`, `<`, `<=`, `>`, and `>=`
- `display.clear(...)`, `display.text(...)`, `display.rect(...)`, `display.line(...)`
- expression-valued `display.text(...)` text arguments and option values

Unsupported areas remain explicit future work: includes, modules, full arithmetic/logical expression precedence, content APIs, BinBook APIs, and production SQBC execution.

## Missing Target Metadata

The target definition now references an external simulator layout file:

```text
targets/layouts/xteink-x4.layout.json
```

The browser simulator still keeps a derived fallback layout in code for resilience. When the layout JSON is available, that file is the preferred source for shell and physical button placement.
