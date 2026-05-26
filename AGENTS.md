# AGENTS.md

## Project Guidance For AI Agents

This repository implements SquidScript, its compiler/runtime pieces, target definitions, firmware work, and the browser simulator. Agents should preserve project intent and avoid demo shortcuts that make unsupported behavior look real.

## Agent Guidance Maintenance

- Watch the conversation for recurring user preferences, workflow corrections, verification expectations, and safety constraints that should guide future agents.
- When a preference or workflow rule is likely to apply beyond the current turn, suggest adding it to `AGENTS.md`.
- If the user agrees, update `AGENTS.md` promptly and keep the guidance concise, actionable, and specific to this repository.
- When presenting decision questions, include the meaningful options with pros,
  cons, and the practical impact of each choice so the user can make an
  informed decision. Add concise examples when they help clarify what an option
  would look like in practice.

## Roadmap Maintenance

- `ROADMAP.md` is the repository issue tracker for agent-visible project work.
- When a roadmap item is completed, remove it from `ROADMAP.md` in the same change or in the next cleanup commit.
- If an AI agent identifies a concrete future task or follow-up while working, add it to `ROADMAP.md` rather than leaving it only in chat.
- Keep roadmap entries concise, actionable, and scoped to repository work.

## Documentation Planning

- When making implementation plans, include documentation work explicitly.
- Create new docs when needed, update related existing docs in the same change, and remove or revise obsolete docs so repository documentation stays aligned with the implementation and current project decisions.
- Before finishing implementation work, check related docs for stale command examples, old API shapes, outdated storage/runtime descriptions, and obsolete compatibility notes.
- Write reference documentation as current-state facts, requirements, commands,
  and interpretation rules, not as a chronological diary of an investigation.
  Future readers need the stable conclusion and what to do with it; omit
  journey framing such as dates, "we observed", or "first this failed, then
  that proved..." unless the chronology is itself operationally relevant.
- If an investigation log is useful but not appropriate for repo reference
  docs, keep it in saved memory for durable cross-session context or in `/tmp`
  or another gitignored scratch file for temporary notes instead of turning
  project documentation into a diary.
- When executing a plan, create or update the todo tracker before doing any
  implementation, investigation, or verification work from that plan. Treat
  this as a hard gate for plan execution: every active implementation step
  should be represented in the tracker, and item statuses should be updated as
  work starts, completes, or becomes blocked so progress and remaining work
  survive context compaction.
- When the user asks for memory numbers without further qualification, report
  RAM numbers by default. Treat flash/app-storage/image-size numbers as flash
  storage and only include them when requested or clearly relevant.

## Language And Spec Discipline

- Do not invent SquidScript syntax, keywords, helpers, or simulator-only DSL conveniences.
- Implement the documented language/spec as written. Use `docs/language_spec.md` as the primary reference.
- If a feature is not implemented yet, say so clearly and keep fixtures/tests honest.
- Do not add tests for removed fake syntax. Treat fake syntax as if it never existed.
- Critical pre-1.0 rule: replace directly. Do not preserve, detect, migrate,
  alias, wrap, special-case, document, test, or otherwise carry old APIs,
  syntax, storage formats, examples, scripts, or behavior unless the user
  explicitly asks for that specific bridge. Removed forms should fail through
  the same ordinary unknown or unsupported path as any other invalid form.
- SquidScript is still early in development, so it is acceptable to break
  incomplete or unstable internals to get requested features working. When the
  user asks for a feature to be done, do not stop at a stub, partial slice, or
  unsupported placeholder when a real implementation path is available. Drive
  the requested behavior through to working code and honest verification, and
  use follow-up roadmap items only for genuinely separate work.
- Do not treat a plan's "spec/API slice first" wording, prior roadmap split, or
  existing unsupported stub as a hard boundary when the user asks for a working
  feature. If repo code or available libraries show a plausible real
  implementation path, attempt it and verify it. Stop at `unsupported`, a stub,
  or a roadmap-only follow-up only when there is a concrete blocker: missing
  hardware/API support, unsafe or destructive risk, unavailable credentials,
  failing documented toolchain, or an explicit user scope limit. State that
  blocker with evidence.
- SQBC is unreleased and has no compatibility contract before 1.0. Do not add
  SQBC version fields, versioned module/function names, compatibility modes,
  backwards readers, or "unsupported version" paths. If current bytecode does
  not run on current compiler/runtime/firmware, treat it as a bug to fix or an
  artifact to rebuild.
- Before 1.0, optimize firmware and CLI workflows for development ergonomics. Do not introduce release-profile trimming, disabling of `RUN.TEMP`, or flash-writing temp runs unless the user explicitly asks to revisit that tradeoff.
- Browser-sim IR JSON is a development artifact only. Do not treat it as a production firmware format. See `docs/browser_simulator.md` and `docs/ir_schema.md`.
- Reference firmware exists to exercise SquidScript language semantics on constrained hardware. Do not frame it as XTEINK X4 staging firmware unless the task explicitly targets X4 behavior.

## Architecture Boundary Discipline

- When planning or adding a feature, research existing libraries, established
  patterns, and relevant existing repositories before designing a custom
  implementation. Present viable options with tradeoffs, and explain why the
  chosen approach fits this repository's constraints.
- Treat non-blocking firmware/runtime behavior as a critical service design
  requirement. A SquidScript service should not monopolize the main loop, starve
  serial input, delay timers, or hide long busy waits behind a convenient API.
  Prefer short poll/step functions, explicit async/event progress, bounded
  time slices, or target scheduler integration over blocking loops.
- Before adding or moving tests, identify the owning layer: language/compiler semantics, SQBC encoding, firmware VM behavior, host CLI behavior, board-specific firmware harness, example app, docs, or simulator.
- When making platform decisions, distinguish the public SquidScript contract from board-specific implementation details. Standardize portable concepts in docs/specs, and keep physical storage layouts, partitions, pins, and device quirks in firmware/target-specific docs and metadata.
- For every architecture or implementation decision, consider cross-platform
  portability before choosing a board-specific path. Keep generic concepts
  generic, isolate target-specific bindings behind target metadata or firmware
  integration layers, and ask the user when portability concerns or ambiguity
  affect the tradeoff.
- For storage decisions, model logical APIs and physical volumes separately. A board may use LittleFS, flash records, SD, or another backend without changing the portable app/compiler contract.
- Do not make lower-level crates depend on repo-level examples or board-specific examples. In particular, `squidc-core` tests must not `include_str!` files from `examples/`; put reusable language fixtures under compiler fixtures, and test example apps through CLI/example or hardware target checks.
- Keep board-specific aliases, fixed GPIO mappings, serial protocols, and physical LED assertions out of compiler core. Compiler core may validate portable syntax and emit portable IR/SQBC; firmware/runtime layers resolve device capabilities and aliases.
- Do not let a demo requirement define public language/runtime semantics implicitly. If a demo needs a timer, GPIO, app lifecycle, or service behavior that is not already specified, update the plan/spec first or clearly mark the implementation as harness-only.
- It is acceptable for real implementation work to inform and reshape the language/API design, but those discoveries must be promoted through the correct boundary: spec/docs for language decisions, compiler tests for language semantics, firmware tests for runtime behavior, CLI tests for host workflow, and hardware target tests for board demos.
- Avoid large cross-layer patches when a narrow change would answer the request. If a change touches compiler, SQBC, firmware, CLI, examples, and docs together, explicitly list why each layer is necessary before editing.
- Prefer library-quality seams over one-off firmware harness slots. Fixed app-id storage like `timer-armed-app`, `reader-clock`, or `break-reminder` belongs only in temporary harness code and must be documented as such until replaced by a real app registry/storage model.
- Example app tests should verify the example at its natural boundary: compile/run with `squidc`, simulator tests, or hardware target tests. They should not become compiler-core unit tests unless the example has been promoted into a compiler fixture with a language-semantics purpose.
- Future Zephyr VM host ABI additions should move as one implemented slice:
  compiler lowering, SQBC builtin IDs, Rust VM callbacks, FFI, Zephyr runtime
  wiring, docs, Rust FFI equivalence tests, and Zephyr ztests. Keep
  `docs/zephyr_vm_host_abi_coverage.md` current when callbacks are added, and
  prefer caller-owned buffers over hidden allocation across the FFI boundary.

## Hardware And Placeholder Discipline

- Clearly mark placeholder, illustrative, guessed, typical, variant-dependent, or unverified values as such.
- This is especially important for hardware pinouts, GPIO mappings, board profiles, firmware configuration, protocol constants, and API examples.
- Be critical before encoding hardware metadata. Distinguish measured facts,
  datasheet facts, target-owner decisions, common clone-board conventions, and
  guesses. Do not present guessed hardware values as sourced facts. If a value
  comes from clone-board conventions or community reports, say that directly
  and preserve the uncertainty in target metadata.
- If a target metadata decision needs user judgment, cannot be verified from
  local evidence, or would require choosing between plausible board-specific
  tradeoffs, do not silently choose a value just to make progress. Ask for
  guidance when the choice blocks the current task; otherwise add a concise
  `ROADMAP.md` item and keep the target metadata conservative.
- Target JSON files are the canonical target descriptions. Human-readable
  target pin/device tables must be generated from target JSON, such as with
  `scripts/generate-target-markdown.py`. Do not hand-edit generated target Markdown tables
  or let them become a second source of truth.
- Do not dox the user or their environment when reporting or documenting
  hardware/network investigations. Redact SSIDs, BSSIDs, MAC addresses, local
  IPs, credentials, and other environment-identifying values unless the user
  explicitly asks for raw identifiers. Preserve technical evidence with counts,
  lengths, channels, RSSI, auth modes, and redacted placeholders instead.

## Constrained Device RAM Discipline

- Treat RAM as a constrained hardware resource by default in firmware,
  firmware-facing Rust libraries, hardware harnesses, and serial tooling.
- Prefer caller-owned buffers, streaming/file-backed staging, borrowed views,
  and in-place construction over fixed temporary arrays, full-payload RAM
  sessions, by-value aggregate transfers, or harness-only buffers.
- Keep fixed buffers only when they represent intentional persistent runtime
  state or an explicitly bounded hardware contract. Avoid stack-sized
  aggregates in resumable VM, protocol, storage, and service paths.
- When diagnosing constrained-device failures, test whether a buffer, callback,
  or FFI boundary materializes hidden temporaries before increasing stack or
  heap sizes. Larger stacks are diagnostic data, not the default fix.
- If a temporary RAM harness is unavoidable, mark it as temporary, keep the
  bound narrow, and add a roadmap item to replace it with a streaming,
  file-backed, or caller-owned design.

## Browser Simulator Verification

When changing `simulator/browser`, verify the actual app behavior, not only unit tests. Use `docs/browser_simulator.md` for the simulator design and workflow.

## Test-Driven Development

- Default to TDD for implementation work: write or update the smallest meaningful failing test first, then implement the behavior, then run the relevant checks.
- For lifecycle, runtime, firmware storage, VM, compiler semantics, and CLI behavior changes, TDD is mandatory unless explicitly impossible: add or update the failing test before implementation, and do not wait for the user to remind you.
- If a change cannot reasonably be test-driven, state the concrete reason before implementation and use the narrowest practical verification instead.
- Keep tests honest. Do not add assertions for unsupported SquidScript syntax, simulator-only conveniences, or fake firmware behavior.
- For firmware work, separate host-testable logic from hardware-bound code so behavior can be driven by unit tests before flashing a device.

## Script And Firmware Tooling Discipline

- Firmware source for the canonical ESP32-C3 firmware lives under
  `firmware/zephyr`; the old Rust ESP32-C3 firmware tree has been removed.
- Before reporting that firmware build, flashing, serial, or hardware checks do
  not work in this environment, check the relevant repository docs and wrapper
  scripts first. Prefer the documented wrapper command over ad hoc direct tool
  invocations, and only call something blocked after the documented path fails.
- Use `scripts/c3-supermini-build.sh` to build or type-check the ESP32-C3
  canonical firmware binary. The wrapper delegates to the Zephyr build wrapper
  and sources the repository Zephyr environment.
- Run ESP32-C3 Super Mini Zephyr build wrappers outside the Codex sandbox in
  this environment. Zephyr/ccache may write host cache files outside the
  workspace, so sandboxed firmware builds can fail with read-only filesystem
  errors unrelated to the source.
- Dry-run new scripts before calling them ready: run `bash -n`, verify required tools and Rust targets, check wrapped command help where practical, and confirm wrapper scripts forward user-supplied arguments.
- For firmware flashing scripts, avoid auto-monitoring by default when USB reset or re-enumeration can break the serial session. Prefer `squidc device monitor` for ESP32-C3 Super Mini SquidScript output, and use explicit opt-in monitoring such as `MONITOR_AFTER_FLASH=1` only when needed.
- Do not filter or suppress flashing tool stderr in firmware scripts. Surface warnings and errors directly, and document known harmless tool warnings instead of hiding them.
- Clearly report host visibility limits, such as Codex sandbox sessions that cannot see `/dev/ttyACM*`, `/dev/ttyUSB*`, or `/dev/bus/usb`.
- Never run hardware scripts or serial commands in parallel against the same
  physical target. A single USB serial device is a shared mutable resource:
  concurrent flash, install, hardware-test, monitor, REPL, or `squidc device`
  commands can interleave bytes, reset the board, steal foreground app state,
  or leave hardware in a misleading state. Run hardware commands sequentially,
  wait for each command to exit, and do not start a second monitor or helper
  while another command owns the port.
- Do not put any hardware-owning command in `multi_tool_use.parallel`. This
  includes `squidc device ...`, `squidc app ...` when it talks to attached
  hardware, `cargo run ... -- device ...`, `cargo run ... -- app ...`,
  firmware flash scripts, monitor scripts, hardware test scripts, and hardware
  benchmark scripts. Use one standalone tool call per hardware command.
- Hardware target tests are listed in `docs/hardware_target_tests.md`; use that inventory to identify real-device tests before running them.
- When firmware work changes behavior that has hardware coverage and an ESP32-C3 Super Mini is attached or reasonably available, run the relevant hardware target tests. Sandbox isolation is not a reason to skip them; use escalated command execution for serial visibility checks and the hardware test command, and report the result.
- When running the ESP32-C3 Super Mini hardware target suite, use
  `scripts/c3-supermini-test-hardware.sh` so stateful checks run first and the
  blinky app runs last. Blinky is the final visible board-state check and
  should be left running unless the user asks otherwise. Do not run any serial
  command after the final blinky launch unless you are deliberately debugging
  the final board state.
- Hardware target tests and serial/flashing commands must run outside the Codex sandbox. Sandboxed sessions do not reliably expose `/dev/ttyACM*`, `/dev/ttyUSB*`, or `/dev/serial/by-id`, even after host reboot. Use escalated command execution for ESP32-C3 Super Mini serial visibility checks and hardware target tests.
- For ESP-IDF hardware-isolation experiments under
  `experiments/esp32c3-supermini/firmware/esp-idf-softap-hwtest`, the user has
  approved the repository's documented containerized ESP-IDF build path when no
  local `idf.py` is installed. Do not re-ask for approval just because Podman or
  Docker will run the official Espressif IDF image with the experiment mounted;
  still avoid passing Wi-Fi credentials to containers unless the specific test
  requires station credentials.
- When troubleshooting ESP32-C3 Super Mini flashing access, check
  `firmware/README.md` and the Zephyr wrapper scripts before suggesting broader
  sudo changes.
- For REPL work, default app and firmware profiles are `dev`. Hardware target tests should include `tests/repl/default-dev.session`, which intentionally does not set `:profile dev`.
- For `hardware.gpio.*` work on the ESP32-C3 Super Mini, run the serial GPIO REPL session and the blinky upload session when hardware is available; the blinky check requires both serial assertions and physical onboard LED observation.
- Do not require `--target` for normal `squidc repl` upload/run flows. SquidScript apps compile against the portable language/runtime API; target definitions are opt-in for explicit target checks, simulator config, firmware metadata, docs, and autocomplete.
- When changing the `squidc` CLI surface, update `docs/squidc_cli.md`, scripts, and command examples in docs in the same change.

## Git Workflow

- Work on the current branch and checkout unless the user specifically asks for
  a separate branch or worktree.
- For slice-based implementation work, commit and push each completed,
  verified slice before moving on to the next slice.
- Git commits must run outside the Codex sandbox. Sandboxed commits cannot create `.git/index.lock` in this environment, so use escalated command execution for `git commit` instead of trying once in the sandbox.

## CLI Workflow Ergonomics

- Before 1.0, prefer developer-friendly CLI defaults when they are safe and
  unambiguous. For example, package creation may write a sensible default
  output in the current directory while still offering `--out` for explicit
  paths.

## Command Matrix

Run checks from the directory shown unless noted.

| Change area | Commands |
| --- | --- |
| Rust compiler crates, fixtures, IR lowering, SQBC container | `cargo test` from repo root |
| Browser simulator TypeScript runtime, WASM compiler bridge, rendering, storage, input | `npm test` from `simulator/browser` |
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
- upload installs `/sd/apps/hello-menu/main.sqbc`
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
