# AGENTS.md

## Project Guidance For AI Agents

This repository implements SquidScript, its compiler/runtime pieces, target definitions, firmware work, and the browser simulator. Agents should preserve project intent and avoid demo shortcuts that make unsupported behavior look real.

## Agent Guidance Maintenance

- Watch the conversation for recurring user preferences, workflow corrections, verification expectations, and safety constraints that should guide future agents.
- When a preference or workflow rule is likely to apply beyond the current turn, suggest adding it to `AGENTS.md`.
- If the user agrees, update `AGENTS.md` promptly and keep the guidance concise, actionable, and specific to this repository.

## Language And Spec Discipline

- Do not invent SquidScript syntax, keywords, helpers, or simulator-only DSL conveniences.
- Implement the documented language/spec as written. Use `docs/language_spec.md` as the primary reference.
- If a feature is not implemented yet, say so clearly and keep fixtures/tests honest.
- Do not add tests for removed fake syntax. Treat fake syntax as if it never existed.
- `entry.type = "ir"` is a browser-simulator development artifact only. Do not treat it as a production firmware format. See `docs/browser_simulator.md` and `docs/ir_schema.md`.
- Reference firmware exists to exercise SquidScript language semantics on constrained hardware. Do not frame it as XTEINK X4 staging firmware unless the task explicitly targets X4 behavior.

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
- For firmware flashing scripts, avoid auto-monitoring by default when USB reset or re-enumeration can break the serial session. Prefer a separate monitor script and an explicit opt-in such as `MONITOR_AFTER_FLASH=1`.
- Do not filter or suppress flashing tool stderr in firmware scripts. Surface warnings and errors directly, and document known harmless tool warnings instead of hiding them.
- Clearly report host visibility limits, such as Codex sandbox sessions that cannot see `/dev/ttyACM*`, `/dev/ttyUSB*`, or `/dev/bus/usb`.
- When troubleshooting ESP32-C3 Super Mini flashing access, check `firmware/README.md` and `firmware/squid-firmware/README.md` for the documented `/dev/ttyACM0` ACL workaround before suggesting broader sudo changes.

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
