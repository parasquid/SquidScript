# AGENTS.md

## Project Guidance For AI Agents

This repository implements SquidScript, its compiler/runtime pieces, target definitions, firmware work, and the browser simulator. Agents should preserve project intent and avoid demo shortcuts that make unsupported behavior look real.

## Agent Guidance Maintenance

- Watch the conversation for recurring user preferences, workflow corrections, verification expectations, and safety constraints that should guide future agents.
- When a preference or workflow rule is likely to apply beyond the current turn, suggest adding it to `AGENTS.md`.
- If the user agrees, update `AGENTS.md` promptly and keep the guidance concise, actionable, and specific to this repository.

## Roadmap Maintenance

- `ROADMAP.md` is the repository issue tracker for agent-visible project work.
- When a roadmap item is completed, remove it from `ROADMAP.md` in the same change or in the next cleanup commit.
- If an AI agent identifies a concrete future task or follow-up while working, add it to `ROADMAP.md` rather than leaving it only in chat.
- Keep roadmap entries concise, actionable, and scoped to repository work.

## Language And Spec Discipline

- Do not invent SquidScript syntax, keywords, helpers, or simulator-only DSL conveniences.
- Implement the documented language/spec as written. Use `docs/language_spec.md` as the primary reference.
- If a feature is not implemented yet, say so clearly and keep fixtures/tests honest.
- Do not add tests for removed fake syntax. Treat fake syntax as if it never existed.
- Until the project reaches a 1.0 release, prefer direct migrations over backwards-compatible aliases or redundant wrappers. Remove obsolete scripts/APIs/docs when replacing them, unless the user explicitly asks for a compatibility bridge.
- `entry.type = "ir"` is a browser-simulator development artifact only. Do not treat it as a production firmware format. See `docs/browser_simulator.md` and `docs/ir_schema.md`.
- Reference firmware exists to exercise SquidScript language semantics on constrained hardware. Do not frame it as XTEINK X4 staging firmware unless the task explicitly targets X4 behavior.

## Architecture Boundary Discipline

- Before adding or moving tests, identify the owning layer: language/compiler semantics, SQBC encoding, firmware VM behavior, host CLI behavior, board-specific firmware harness, example app, docs, or simulator.
- Do not make lower-level crates depend on repo-level examples or board-specific examples. In particular, `squidc-core` tests must not `include_str!` files from `examples/`; put reusable language fixtures under compiler fixtures, and test example apps through CLI/example or hardware target checks.
- Keep board-specific aliases, fixed GPIO mappings, serial protocols, and physical LED assertions out of compiler core. Compiler core may validate portable syntax and emit portable IR/SQBC; firmware/runtime layers resolve device capabilities and aliases.
- Do not let a demo requirement define public language/runtime semantics implicitly. If a demo needs a timer, GPIO, app lifecycle, or service behavior that is not already specified, update the plan/spec first or clearly mark the implementation as harness-only.
- It is acceptable for real implementation work to inform and reshape the language/API design, but those discoveries must be promoted through the correct boundary: spec/docs for language decisions, compiler tests for language semantics, firmware tests for runtime behavior, CLI tests for host workflow, and hardware target tests for board demos.
- Avoid large cross-layer patches when a narrow change would answer the request. If a change touches compiler, SQBC, firmware, CLI, examples, and docs together, explicitly list why each layer is necessary before editing.
- Prefer library-quality seams over one-off firmware harness slots. Fixed app-id storage like `timer-armed-app`, `reader-clock`, or `break-reminder` belongs only in temporary harness code and must be documented as such until replaced by a real app registry/storage model.
- Example app tests should verify the example at its natural boundary: compile/run with `squidc`, simulator tests, or hardware target tests. They should not become compiler-core unit tests unless the example has been promoted into a compiler fixture with a language-semantics purpose.

## Hardware And Placeholder Discipline

- Clearly mark placeholder, illustrative, guessed, typical, variant-dependent, or unverified values as such.
- This is especially important for hardware pinouts, GPIO mappings, board profiles, firmware configuration, protocol constants, and API examples.
- Do not present guessed hardware values as sourced facts. If a value comes from clone-board conventions or community reports, say that directly and preserve the uncertainty in target metadata.

## Browser Simulator Verification

When changing `simulator/browser`, verify the actual app behavior, not only unit tests. Use `docs/browser_simulator.md` for the simulator design and workflow.

## Test-Driven Development

- Default to TDD for implementation work: write or update the smallest meaningful failing test first, then implement the behavior, then run the relevant checks.
- Keep tests honest. Do not add assertions for unsupported SquidScript syntax, simulator-only conveniences, or fake firmware behavior.
- For firmware work, separate host-testable logic from hardware-bound code so behavior can be driven by unit tests before flashing a device.
- If a change cannot reasonably be test-driven, say why and use the narrowest practical verification instead.

## Script And Firmware Tooling Discipline

- Dry-run new scripts before calling them ready: run `bash -n`, verify required tools and Rust targets, check wrapped command help where practical, and confirm wrapper scripts forward user-supplied arguments.
- For firmware flashing scripts, avoid auto-monitoring by default when USB reset or re-enumeration can break the serial session. Prefer `squidc device monitor` for ESP32-C3 Super Mini SquidScript output, and use explicit opt-in monitoring such as `MONITOR_AFTER_FLASH=1` only when needed.
- Do not filter or suppress flashing tool stderr in firmware scripts. Surface warnings and errors directly, and document known harmless tool warnings instead of hiding them.
- Clearly report host visibility limits, such as Codex sandbox sessions that cannot see `/dev/ttyACM*`, `/dev/ttyUSB*`, or `/dev/bus/usb`.
- Do not run hardware scripts or serial commands in parallel against the same physical target. A single USB serial device is a shared mutable resource; run flash, install, hardware test, monitor, and `squidc` device commands sequentially.
- Hardware target tests are listed in `docs/hardware_target_tests.md`; use that inventory to identify real-device tests before running them.
- When running the ESP32-C3 Super Mini hardware target suite, use `scripts/c3-supermini-test-hardware.sh` so stateful checks run first and the blinky app runs last. Blinky is the final visible board-state check and should be left running unless the user asks otherwise.
- Hardware target tests and serial/flashing commands must run outside the Codex sandbox. Sandboxed sessions do not reliably expose `/dev/ttyACM*`, `/dev/ttyUSB*`, or `/dev/serial/by-id`, even after host reboot. Use escalated command execution for ESP32-C3 Super Mini serial visibility checks and hardware target tests.
- When troubleshooting ESP32-C3 Super Mini flashing access, check `firmware/README.md` and `firmware/squid-firmware/README.md` for the documented `/dev/ttyACM0` ACL workaround before suggesting broader sudo changes.
- For v4 REPL work, default app and firmware profiles are `dev`. Hardware target tests should include `tests/repl/default-dev.session`, which intentionally does not set `:profile dev`.
- For `hardware.gpio.*` work on the ESP32-C3 Super Mini, run the serial GPIO REPL session and the blinky upload session when hardware is available; the blinky check requires both serial assertions and physical onboard LED observation.
- Do not require `--target` for normal `squidc repl` upload/run flows. SquidScript apps compile against the portable language/runtime API; target definitions are opt-in for explicit compatibility checks, simulator config, firmware metadata, docs, and autocomplete.
- When changing the `squidc` CLI surface, update `docs/squidc_cli.md`, scripts, and command examples in docs in the same change.

## Git Workflow

- Git commits must run outside the Codex sandbox. Sandboxed commits cannot create `.git/index.lock` in this environment, so use escalated command execution for `git commit` instead of trying once in the sandbox.

## Command Matrix

Run checks from the directory shown unless noted.

| Change area | Commands |
| --- | --- |
| Rust compiler crates, fixtures, IR lowering, SQBC container | `cargo test` from repo root |
| Browser simulator TypeScript runtime, compiler fallback, rendering, storage, input | `npm test` from `simulator/browser` |
| Browser simulator production build or WASM compiler bridge | `npm run build` from `simulator/browser` |
| Browser UI behavior, Hello Menu flow, canvas pixels, Firefox/mobile coverage | `npm run test:e2e` from `simulator/browser` |
| Target definitions or render policy docs | `cargo test` plus relevant browser tests if browser-sim consumes the target |
| Docs-only edits | Usually no tests required, unless examples/fixtures changed |

For browser-sim changes that affect the real app experience, run `npm test`, `npm run build`, and `npm run test:e2e`; then try the flow manually on `http://127.0.0.1:5174/` when visual behavior is relevant.

Expected baseline checks:

- `npm test`
- `npm run build`
- `npm run test:e2e`
- Run the dev server on `http://127.0.0.1:5174/`
- Exercise the browser UI: reset, compile, upload, run, and input navigation for Hello Menu.

Hello Menu should prove:

- compile succeeds with the WASM compiler
- upload installs `/sd/apps/hello-menu/app.json`
- upload installs `/sd/apps/hello-menu/main.ir.json`
- run opens the `menu` screen
- selected row pixels are black and unselected/background pixels are white
- `UP`/`DOWN` move selection correctly and stay bounded
- `SELECT` opens screens or exits according to the script
- `BACK` returns from `hello`/`about` to `menu`; `BACK` exits only from `menu`
- reload preserves saved app state
- reset controls clear the right state/storage

## Firefox Canvas Caveat

Firefox on Linux may visually composite a scaled `<canvas>` incorrectly when CSS uses:

```css
image-rendering: pixelated;
```

We observed a case where Firefox displayed the X4 canvas as black even though `getImageData()` returned correct pixels:

- background: white
- selected row: black
- unselected row: white

The browser simulator should avoid `image-rendering: pixelated` on the main device canvas unless this is re-tested in real Firefox. Prefer `image-rendering: auto` for the scaled device display.

If Firefox appears visually wrong, inspect the actual canvas before changing runtime logic:

```js
(() => {
  const c = document.querySelector('canvas[aria-label="X4 display"]');
  const ctx = c?.getContext("2d");
  const pix = (x, y) => ctx ? Array.from(ctx.getImageData(x, y, 1, 1).data) : null;

  return {
    canvasFound: !!c,
    attrs: c ? {
      renderOk: c.getAttribute("data-render-ok"),
      commandCount: c.getAttribute("data-command-count"),
      firstCommand: c.getAttribute("data-first-command"),
      width: c.width,
      height: c.height,
      clientWidth: c.clientWidth,
      clientHeight: c.clientHeight
    } : null,
    pixels: {
      background: pix(10, 10),
      selectedRow: pix(40, 170),
      aboutRow: pix(40, 226)
    },
    diagnostics: document.querySelector('[aria-label="display diagnostics"]')?.textContent
  };
})();
```

If backing pixels are correct but the canvas looks wrong, suspect CSS/compositor behavior before changing compiler/runtime semantics.

## Grayscale Semantics

SquidScript logical grayscale follows the language spec. See `docs/language_spec.md` and `docs/target_profile_architecture.md`.

- `gray0` is white
- `gray15` is black
- `white` is equivalent to `gray0`
- `black` is equivalent to `gray15`

Do not introduce internal inversions that make renderer and runtime disagree. Renderer-facing draw commands should use the same logical grayscale values.

## Browser State

The browser simulator uses browser storage:

- IndexedDB backs simulated `/sd`
- `localStorage` stores the editor draft

`localhost:5174` and `127.0.0.1:5174` are separate browser origins. Clearing one does not clear the other.

Use the simulator's reset controls when debugging:

- `Reset App State`: clears app-scoped runtime state
- `Reset Storage`: clears simulated `/sd`
- `Reset Simulator`: clears simulated `/sd`, editor draft, compiled state, installed app selection, runtime state, and debug log
- `Clean Launch`: resets, restores default Hello Menu source, compiles, uploads, and runs in one path

## Target And Rendering References

- `targets/xteink-x4.target.json`: XTEINK X4 target data used by browser-sim
- `docs/target_definition_reference.md`: target definition reference
- `docs/target_profile_architecture.md`: target profile and grayscale semantics
- `docs/browser_simulator.md`: browser simulator architecture and workflow
- `docs/ir_schema.md`: browser-sim IR JSON shape

## Dev Server

If browser behavior disagrees with code, check for a stale Vite server on port `5174`. Restart the dev server and hard reload the browser before assuming runtime/compiler behavior is wrong.

Use `http://127.0.0.1:5174/` as the default URL for simulator verification.
