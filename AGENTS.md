# AGENTS.md

## Project Guidance For AI Agents

This repository implements SquidScript, its compiler/runtime pieces, target definitions, firmware work, and the browser simulator. Agents should preserve project intent and avoid demo shortcuts that make unsupported behavior look real.

## Agent Guidance Maintenance

- Watch the conversation for recurring user preferences, workflow corrections, verification expectations, and safety constraints that should guide future agents.
- When a preference or workflow rule is likely to apply beyond the current turn, suggest adding it to `AGENTS.md`.
- If the user agrees, update `AGENTS.md` promptly and keep the guidance concise, actionable, and specific to this repository.
- Do not trim the todo list down to just the pending items at the end of a
  slice. The completed items are the record of what was done in this
  session; leave them in the list and add new pending items below.
- When presenting decision questions, include the meaningful options with pros,
  cons, and the practical impact of each choice so the user can make an
  informed decision. Add concise examples when they help clarify what an option
  would look like in practice.
- Never revert, discard, or overwrite a user's working-tree changes without
  asking first. If a change looks unrelated to the current task, surface it to
  the user and let them decide. Untracked, unstaged, and uncommitted edits in
  the working tree are the user's work, not noise to clean up.
- If a `git checkout` / `git restore` / `git reset` / `git stash` operation
  could destroy or hide the user's work, stop and ask before running it. This
  applies even when the goal is to keep a commit "clean".
- Never commit code that hasn't been verified to work. Build, flash, and test
  on hardware (or at minimum run relevant unit tests) before staging and
  committing. An unverified change in the working tree is fine; an unverified
  commit is a liability.

## Local Environment Notes

- If `.local-agents.md` exists at the repository root, read it at the start
  of a session for environment-specific notes (attached hardware, host paths,
  local tooling shortcuts, prior-session context). It is gitignored and
  never committed; it carries personal setup for the maintainer's machine,
  not project contract. Do not copy its contents into committed docs, and
  do not repeat environment identifiers (USB serials, device paths) from it
  in chat output or commit messages beyond what the placeholder-discipline
  rules allow.

- When starting any implementation work (including debugging, verification,
  or any technical task), update `/var/home/tristan/Documents/parasquid/SquidScript/.current_agent_work`
  with a concise summary of:
  - What work is being done
  - Current status and next steps
  - Any relevant context from prior investigation or decisions
  - What the next agent should expect
- This file is gitignored and never committed; it serves as a durable
  hand-off surface for quota/interruption scenarios.
- The file should be updated before any code changes, test runs, or
  hardware interactions so that a different agent can pick up the work
  if the current agent runs out of quota or is interrupted.
- Always append to the file with a clear header indicating the new work
  being started, preserving previous entries for context.
- When work is completed, add a completion note with the final outcome
  and any follow-up items that should be tracked separately.

## Roadmap Maintenance

- `ROADMAP.md` is the repository issue tracker for agent-visible project work.
- When a roadmap item is completed, remove it from `ROADMAP.md` in the same change or in the next cleanup commit.
- If an AI agent identifies a concrete future task or follow-up while working, add it to `ROADMAP.md` rather than leaving it only in chat.
- Before writing a new ROADMAP entry, surface the proposed wording to the user and wait for confirmation. ROADMAP.md is the user's planning surface; do not commit agent-authored entries silently.
- Keep roadmap entries concise, actionable, and scoped to repository work.
- Keep speculative or conditional ideas in `ICEBOX.md`, not `ROADMAP.md`.
  Move them back to the roadmap only when they have a concrete target, use
  case, or implementation reason that makes them actionable.
- When dropping a transport or API before 1.0 (for example, a spec-named
  protocol that doesn't fit the actual user-facing client), the dropped work
  goes to `ICEBOX.md` with three things: the rationale — why was it dropped;
  the conditions to revive it — what use case would make it worth re-adding;
  the parts that are still in use — which code, docs, or tests survive the
  drop. Don't leave "what was dropped and why" in chat or in a commit
  message; it gets lost. `ICEBOX.md` is the durable surface.

## Test Safety Net

- Keep the relevant automated tests green as much as practical, even when
  pursuing a different implementation goal. Failing tests weaken the safety net
  and can hide newly introduced bugs; treat broken tests as active project risk,
  distinguish pre-existing failures from regressions, and restore the test
  baseline before continuing unless the user explicitly narrows the scope or a
  concrete blocker makes that impossible.
- When an agent encounters behavior that could plausibly be a bug, regression,
  semantic gap, or documentation mismatch, highlight it explicitly to the user
  with the evidence observed and whether it was verified, suspected, or still
  ambiguous. Do not silently treat possible bugs as incidental tool noise or
  bury them in a long status update.
- Keep tests focused on durable contracts, not incidental implementation text.
  Use exact assertions for public behavior, compiler semantics, ABI/protocol
  wire formats, generated artifact equality, and manifest-selected budgets.
  Avoid source-shape sentinels such as exact helper names, field order, old
  numeric values, or stale doc prose unless they are the narrowest way to catch
  a real pre-1.0 API/storage/protocol compatibility regression.
- Treat RAM, stack, pool, response-size, and target capability tests as budget
  tests. Exact values should come from target/runtime metadata, generated
  headers, or another explicit source of truth; otherwise prefer bounds or
  behavior checks. Delete historical old-value `assertNotIn` checks once the
  current budget is checked from its source of truth.
- Migration tests for removed syntax, APIs, storage formats, protocol names, or
  compatibility bridges are allowed only while they protect an active removal
  decision. Keep them grouped and purposeful; do not let them accumulate as
  permanent "old value must not appear" tests.
- For firmware performance work, build/flash/measure on hardware after each
  independent slice, not after batching all changes together. Batching masks
  which change introduced a regression. If a plan has two independent fixes
  (e.g. handle reuse + buffer allocation), measure after the first fix before
  starting the second. This was violated during binbook latency work: both
  fixes were committed together, a 5.6x decompression regression went
  undetected until final measurement, and isolating the cause required
  undoing work.

## Debug Instrumentation

- Wrap all debug timing, measurement, and diagnostic logging with
  `#if IS_ENABLED(CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC)` so they compile
  out in release builds. Use `sq_debug_log_append` for timestamped trace
  markers. The guard ensures zero overhead in production while keeping
  instrumentation available for development and hardware debugging.

## Lessons Learned

- When tests fail after a format change, determine whether the tests are stale
  (written for the old format) or whether the code is wrong before choosing a
  fix. The user explicitly asked for this discipline: "don't work around tests
  if they're provably wrong." Preserve test *intention* (what contract is being
  verified) and update the test to match the new contract when the format changes.
- When a debug test passes but integration tests fail with identical logic and
  scratch size, check the binary output of the encoder (Python) against the
  parser (Rust) field-by-field. In the binbook per-plane work, the debug test
  happened to exercise a code path that masked the parser bug. A clean
  `cargo clean && cargo test` is the reliable way to rule out stale builds.
- When porting struct layouts between languages, verify the packed byte layout
  matches the encoder exactly. The binbook plane directory uses `4I4I` (all
  offsets, then all sizes), not interleaved `offset,size` pairs. Off-by-8-byte
  layout mismatches produce plausible-looking but wrong values (e.g. a size
  appearing in an offset field).
- Compute expected output sizes from source metadata (pixel format, dimensions)
  rather than relying on the output buffer length. Passing `out.len()` as the
  expected decompression size makes size checks degenerate into `len < len`.

## Documentation Planning

- When making implementation plans, include documentation work explicitly.
- Create new docs when needed, update related existing docs in the same change, and remove or revise obsolete docs so repository documentation stays aligned with the implementation and current project decisions.
- Before finishing implementation work, check related docs for stale command examples, old API shapes, outdated storage/runtime descriptions, and obsolete compatibility notes.
- When a commit deletes a `.c`/`.h` file or replaces a transport, protocol, or core module, grep the deleted file's basename across `docs/`, `README.md`, `ROADMAP.md`, and the commit message of any related doc commits. Fix references in the same commit, not the next. A 30-second grep catches the obvious staleness that would otherwise leave docs describing a state that no longer exists.
- Write reference documentation as current-state facts, requirements, commands,
  and interpretation rules, not as a chronological diary of an investigation.
  Future readers need the stable conclusion and what to do with it; omit
  journey framing such as dates, "we observed", or "first this failed, then
  that proved..." unless the chronology is itself operationally relevant.
- If an investigation log is useful but not appropriate for repo reference
  docs, keep it in saved memory for durable cross-session context or in `/tmp`
  or another gitignored scratch file for temporary notes instead of turning
  project documentation into a diary.
- This applies to code and config comments too, not just reference docs.
  Describe what the code currently does, never what it used to do or what was
  removed. A surviving comment like "X was dropped", "replaces Y", "formerly
  Z", or "historical name, rename later" is noise to a new reader who never saw
  X/Y/Z — it describes a delta against a state that no longer exists. Put
  removal/transition rationale in the commit message (history) or, for parked
  ideas, in `ICEBOX.md` — not in the comment that outlives the change.
- Every active implementation plan must be added to the todo tracker before
  doing implementation, investigation, or verification work from that plan.
  Treat this as a hard gate for plan execution: every active implementation
  step should be represented in the tracker, and item statuses should be
  updated as work starts, completes, or becomes blocked so the plan, progress,
  and remaining work survive context compaction.
- When the user asks for memory numbers without further qualification, report
  RAM numbers by default. Treat flash/app-storage/image-size numbers as flash
  storage and only include them when requested or clearly relevant.
- Design specs for major features (written before implementation) live at
  `docs/specs/YYYY-MM-DD-<topic>-design.md`. Use this path consistently;
  do not use `docs/designs/`, `docs/rfcs/`, or other locations. The naming
  convention is `<date>-<topic>-design.md` where `topic` is short
  kebab-case (e.g. `ble-object-transfer`, `runtime-tunable-caps`).
- Implementation plans for feature, refactor, hardware, or transport slices
  live at `docs/plans/YYYY-MM-DD-<topic>.md`. Use this path consistently;
  do not use tool-specific locations such as `docs/superpowers/plans/`.
  Plans are execution checklists; durable design decisions belong in
  `docs/specs/`, and final current-state reference material belongs in the
  relevant top-level docs file.

## Language And Spec Discipline

- Do not invent SquidScript syntax, keywords, helpers, or simulator-only DSL conveniences.
- Implement the documented language/spec as written. Use `docs/language_spec.md` as the primary reference.
- When a clean SquidScript app design is blocked by missing compiler, runtime,
  or firmware functionality, prefer implementing the missing capability over
  adding app-level workarounds. Ask the user if the capability boundary or
  scope is unclear.
- Runtime resource caps (foreground timer slots, armed timer slots, event-name
  length, active bindings, input buttons, app store, wire-format limits) are
  bounded and live in `docs/runtime_limits.md` with the C macros as the
  source of truth. Read the table before adding tests, fixtures, or app
  code that depends on a specific cap.
- When designing SquidScript language features, public APIs, references, or
  declarations, prefer explicit typed forms over stringly or implicit forms
  unless there is a concrete implementation or usability reason not to.
- Treat `service.*` as the namespace for target- or firmware-backed runtime
  capability endpoints that app code invokes and that may vary by target
  availability, binding, configuration, or runtime state. Use it for portable
  services such as display, indicator, Wi-Fi, power, timers, BLE profiles, and
  future bindable/configurable target endpoints. Do not move VM/app concepts
  such as `app.*`, `screen.*`, `state.*`, `string.*`, or `system.*` under
  `service.*`; keep raw target access under `hardware.*`.
  For content/document APIs such as `file.*` and future document namespaces,
  choose the app-facing namespace based on the authoring concept unless the API
  is explicitly exposing a runtime service endpoint.
- If a feature is not implemented yet, say so clearly and keep fixtures/tests honest.
- Do not add tests for removed fake syntax. Treat fake syntax as if it never existed.
- Do not add regression tests that name removed syntax or removed API forms.
  Tests may verify current generic parser/compiler behavior, but must not
  preserve old names, compatibility diagnostics, examples, or aliases.
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
- Pre-1.0 redesign churn is not a reason to avoid recommending or choosing the
  better SquidScript authoring model, API shape, or architecture. When comparing
  approaches, treat compatibility churn as acceptable by default and optimize
  for the best long-term language/runtime design unless the user explicitly
  asks for a smaller compatibility-preserving slice or there is a concrete
  blocker such as hardware limits, unavailable tooling, safety risk, or missing
  implementation evidence.
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
- Firmware, display, storage, input, and hardware work in this repository must
  serve SquidScript. Do not create standalone Rust firmware, board demos, or
  device bring-up projects as ends in themselves unless the user explicitly
  asks for that scope. Standalone harnesses are acceptable only as supporting
  tests, diagnostics, or temporary bring-up steps for a SquidScript compiler,
  runtime, firmware, target, simulator, or service implementation, and must be
  labeled that way.

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
- For physical device input work, keep serial and runtime polling responsive.
  Do not add unbounded or raw GPIO reads to the firmware main-loop path just to
  detect a button press. Prefer target-supported interrupts or another bounded
  nonblocking mechanism that records a pending logical input event for the
  runtime to dispatch.
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
- When a doc commit touches more than 3 doc files, flag the doc set as a snapshot that may need re-validation against the current code at the next major refactor. A large doc commit is correct when written, becomes stale the same day a refactor lands, and the user has to re-derive the actual state from the refactor commits. Mark the commit message with "doc snapshot — re-validate after next refactor" so future agents know to verify.
- Before 1.0, when committing to a transport, protocol, or API shape, ask "what is the actual user-facing client, and what can it do?" Examples of client capabilities that have already shaped this codebase: Web Bluetooth is GATT-only (no L2CAP); iOS Safari has no Web Bluetooth at all; bleak 3.x's cross-platform client does not expose L2CAP CoC writes. When the spec names a transport that the actual client cannot drive, the spec is wrong, not the client. Design from the client up, not from the protocol down. After 1.0 the spec wins and the client must catch up; before 1.0 the client wins because compatibility is not yet a commitment.
- For any transport, protocol, or external dependency that the spec names but this session has not yet exercised on the actual host, do a small spike first. The spike should be throwaway (a separate commit that is reverted or deleted in the next commit) and answer one question: "can the actual host do this, and if so, how?" Then decide, then implement. A 30-minute spike that returns "no, the host cannot do this" saves a multi-slice implementation that would have hit the same wall.
- When a spec text and an obvious ergonomic API are in tension (for example, "the file is ephemeral, no final path" vs. an app that needs to know where the file lives during the handler), surface the ambiguity to the user with the two readings before committing the spec change. Don't remove a field, tighten a payload, or pick a minimalist interpretation unilaterally — the spec text and the use case both need to survive the decision.
- Prefer library-quality seams over one-off firmware harness slots. Fixed app-id storage like `timer-armed-app`, `reader-clock`, or `break-reminder` belongs only in temporary harness code and must be documented as such until replaced by a real app registry/storage model.
- Example app tests should verify the example at its natural boundary: compile/run with `squidc`, simulator tests, or hardware target tests. They should not become compiler-core unit tests unless the example has been promoted into a compiler fixture with a language-semantics purpose.
- Keep small SquidScript hardware/runtime regression tests as example-backed
  app tests where possible: `examples/app-tests/**/main.squid` plus sibling
  `test.session`, driven by `cargo run -p squidc -- app test <suite>`.
  Keep negative compile fixtures under `tests/app-tests/negative` and run them
  with `cargo run -p squidc -- app test --negative tests/app-tests/negative`.
  Do not collapse portable language/app checks into one huge script; prefer
  small unit-style examples that are useful to users and proven by the test
  runner.
- `examples/app-tests/` are test fixtures, not full example apps. They may be
  stripped-down versions of the corresponding `examples/` app (e.g.
  `examples/grid-cursor/` has full 5×5 grid rendering while
  `examples/app-tests/xteink/grid-cursor/` only tests input routing with no
  visible drawing). When verifying display, rendering, or visual behavior, use
  the full example app from `examples/`, not the app-test fixture.
- Future Zephyr VM host ABI additions should move as one implemented slice:
  compiler lowering, SQBC builtin IDs, Rust VM callbacks, FFI, Zephyr runtime
  wiring, docs, Rust FFI equivalence tests, and Zephyr ztests. Keep
  `docs/zephyr_vm_host_abi_coverage.md` current when callbacks are added, and
  prefer caller-owned buffers over hidden allocation across the FFI boundary.
- When C helpers, defaults, callback initializers, or result-shape utilities
  are mechanically derived from the SquidVM FFI ABI, generate them from
  `compiler/rust/crates/squidvm-ffi/abi/manifest.json` through
  `scripts/check-squidvm-ffi-abi.py` instead of hand-duplicating equivalent C
  logic. Hand-written C should be reserved for behavior that cannot reasonably
  be expressed as ABI metadata, and that exception should be explicit in the
  code or plan.

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

## Hardware Availability — Probe Before Assuming

- Do not claim you "can't test on hardware" or "have no radio/device" without
  first checking. An agent working on this project may be running on a developer
  workstation with real hardware attached, and assuming otherwise has led to
  shipping firmware as "build-verified only" when it could have been driven
  end-to-end on a live device.
- This is an *example* of what may be present (your environment may differ —
  probe, do not hardcode these assumptions):
  - A host **Bluetooth controller**: `ls /sys/class/bluetooth/`, `bluetoothctl
    list`, `rfkill list`, and a Python BLE stack (`python3 -c "import bleak"`)
    let you scan, connect, and drive GATT against a flashed device.
  - An **ESP32-C3 dev board on a serial port**: `ls /dev/ttyACM* /dev/ttyUSB*`,
    then `cargo run -p squidc -- target flash --target <id>` (set
    `ESPFLASH_PORT`) to flash, and `cargo run -p squidc -- app install|launch|
    list` / `device output` to drive it over serial.
- When a change touches firmware, a transport, or a hardware path, attempt the
  real end-to-end run (flash, drive, observe) before reporting completion. Unit
  tests and mocks catch logic bugs; only real hardware catches things like ATT
  MTU limits, advertising/GATT registration, and on-wire timing. If hardware
  genuinely is not reachable after probing, say so explicitly *and* say what you
  checked — never assume it away.
- Reflashing the project's dev board is routine for this work; treat it like
  running a test, not a destructive action. Still redact environment
  identifiers per the placeholder-discipline rules above.
- The SD card used by the XTEINK X4/dev setup is owned by the device over its
  SPI SD interface. It will not appear as the same mounted filesystem on the
  host while it is inside the device. Do not infer that a host-visible
  `/run/media/...`, `/dev/sd*`, or USB MassStorageClass volume is the X4's
  internal SD card unless the user explicitly says that exact mass-storage
  bridge is active. To place files on the in-device card, use a firmware-backed
  transfer path or ask the user to move/copy the card externally.

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

## Magic Numbers And Legible Diagnostics

- Avoid magic numbers. Prefer named constants, enum values, or the platform's
  named symbols (e.g. `-ENODEV`, `EIO`) over bare integer literals in code,
  config, protocol framing, and especially in diagnostics. A literal `-19` or
  `0x01` forces every future reader to look it up and rots silently when the
  underlying value changes.
- When a status, errno, opcode, or protocol code is emitted to a human-facing
  surface (device error/trace output, logs, CLI), pair the number with its
  name so it is legible at the point of use, e.g. `code=-12 (ENOMEM)` via
  `sq_errno_name` rather than a bare `code=-12`. The number alone is a dead end
  during debugging.
- Preserve error specificity end to end. Do not collapse distinct failures into
  one opaque code: when a boundary (FFI, callback, transport) must narrow a rich
  error, record the original code/name at the source before it is flattened, so
  the real cause is recoverable instead of surfacing as a generic catch-all.
- When you must introduce a numeric literal that is not yet named, define a
  named constant next to its definition and reference that, rather than
  repeating the literal across call sites.

## Device Protocol Encoding Discipline

- Prefer compact, typed, stable identifiers on constrained device wire
  protocols instead of repeated human-readable strings when the response can
  grow with diagnostics, resources, metrics, or table rows. Use named
  constants/enums for those identifiers, never bare numeric literals.
- Keep human-facing names in host tooling, docs, and CLI output by translating
  protocol identifiers at the boundary. Firmware should not spend RAM or frame
  budget repeating strings solely to make the raw wire payload readable.
- Do not collapse this into opaque diagnostics: protocol IDs must have a
  maintained name table in the host codec/tests so `squidc` output remains
  readable and stable.

## Screen Render Discipline

- SquidScript display calls are render-only: call `service.display.*` or `display.*` from `screen(...)` bodies, not from event handlers or ordinary helper functions reached from events. Event handlers should update state and call `screen.open(...)` or `screen.refresh()`; the screen body reads state and draws. Screen bodies must not mutate app state or lifecycle.

## Browser Simulator

When changing `simulator/browser`, verify the actual app behavior, not only unit
tests. `docs/browser_simulator.md` is the source for simulator design, workflow,
dev-server and browser-state debugging, the Firefox canvas caveat, grayscale
semantics, and target/rendering references. `docs/standards/verification-commands.md`
lists the per-area check commands and the Hello Menu proof.

## Test-Driven Development

- Default to TDD for implementation work: write or update the smallest meaningful failing test first, then implement the behavior, then run the relevant checks.
- For lifecycle, runtime, firmware storage, VM, compiler semantics, and CLI behavior changes, TDD is mandatory unless explicitly impossible: add or update the failing test before implementation, and do not wait for the user to remind you.
- If a change cannot reasonably be test-driven, state the concrete reason before implementation and use the narrowest practical verification instead.
- Keep tests honest. Do not add assertions for unsupported SquidScript syntax, simulator-only conveniences, or fake firmware behavior.
- For firmware work, separate host-testable logic from hardware-bound code so behavior can be driven by unit tests before flashing a device.
- When adding a new `BUILTIN_*` opcode to `compiler/rust/crates/squidvm-core/src/vm.rs`, add the constant to the `crate::bytecode::{...}` import list in the same change. Without the import, Rust treats the match arm as a wildcard binding that shadows every other builtin and the VM silently dispatches everything to the new opcode. Use the bytecode FFI dispatch tests as the canary: any unrelated builtin (wifi, indicator, app lifecycle) failing after a VM dispatch change points at a missing import. The planned long-term fix is to switch all `BUILTIN_*` match arms to fully-qualified `bytecode::BUILTIN_*` paths (see ROADMAP), which makes the shadowing impossible.
- Test any path that reads, copies, streams, or installs a file with a payload LARGER than the obvious scratch buffer, scratch register, or streaming chunk size. A test fixture that fits the scratch buffer is a no-op test; a test fixture that overflows the scratch buffer is the test that catches the truncation bug. Example: an install path backed by a 1 KiB scratch needs a 2 KiB or 4 KiB fixture, not a 685-byte hello-menu fixture.
- A slice is not done when its ztests pass on native_sim. A slice that exercises an end-to-end user flow (install + arm + push + verify) is done when that flow has been driven end-to-end on real hardware with a real payload, not just skipped with a clean exit code. Define the done criterion in the slice plan: "is hardware verification required for this slice?" If yes, do not mark the slice done until the hardware run completes the loop. A "clean skip" is information about the host, not evidence that the slice works.

## Script And Firmware Tooling Discipline

Exact build/flash/serial commands, Zephyr env + test wrappers, target names,
venv paths, and the hardware-test inventory live in
`docs/standards/firmware-tooling.md` — read it when doing firmware or hardware
work. The always-fire disciplines stay here:

- Treat known sandbox limitations as instructions to use escalated execution
  immediately, not as hypotheses to re-test. If AGENTS.md, saved memory, or the
  current conversation identifies a command category as sandbox-hostile, do not
  run it in the sandbox first just to confirm the failure. Request escalated
  execution up front, cite the known limitation, and continue with the actual
  task.
- This immediate-escalation rule covers, at minimum, `gh`/GitHub API commands
  that need the host credential store or network, `git add`/`git commit` and
  other commands that must update `.git/index`, hardware/serial/flashing
  commands, Zephyr/Twister/build wrappers documented in
  `docs/standards/firmware-tooling.md` as host-only, and
  commands that need host-visible USB devices, keyrings, sockets, or caches
  outside the workspace. Do not waste a turn on an expected sandbox failure.
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
- When firmware work changes behavior that has hardware coverage and a relevant
  hardware target is attached or reasonably available, run the relevant
  hardware target tests. Sandbox isolation is not a reason to skip them; use
  escalated command execution for serial visibility checks and the hardware
  test command, and report the result. The XIAO ESP32-C3 e-paper target is the
  default dev target; the ESP32-C3 Super Mini is a regression target, not the
  only SquidScript hardware target.
- Prefer `cargo run -p squidc -- hardware test --target <target-id>` when the
  target-aware wrapper covers the changed path. On the XIAO ESP32-C3 default
  dev target, this currently exercises portable app tests, BLE file-transfer
  install, BLE reconnect, radio concurrency, and AP-after-station; display and
  SD checks remain out of scope until those target capabilities are ready.
- For unattended XIAO e-paper smoke tests, the serial marker or documented BUSY
  activity evidence is sufficient. Require visual confirmation only when the
  task explicitly asks to verify rendered pixels or the user is present to
  inspect the panel.
- Treat changes under `firmware/zephyr/**`, generated Zephyr C includes,
  target metadata consumed by firmware, serial protocol behavior, app
  lifecycle/runtime callbacks, storage/runtime state, and hardware-facing
  scripts as firmware-impacting changes for hardware-test decisions. Generated
  firmware artifacts count the same as handwritten C; moving C wiring into a
  generated include still requires the same hardware-test decision.
- Native Zephyr ztests are the pre-hardware gate for firmware-impacting
  changes, not a replacement for relevant hardware checks when hardware is
  available. For low-risk firmware refactors, the minimum hardware check is
  build/flash/boot without hanging on an attached target that exercises the
  changed firmware path; run the broader hardware suite when runtime behavior
  paths changed or when the target test inventory indicates coverage.
- When a hardware command timeout produces empty protocol diagnostics
  (`device resources`, `device errors`, and `device lifecycle`), capture bounded
  raw serial evidence before calling the test flaky or assuming host timing.
  This is especially important at the reset boundary between XIAO
  `radio-concurrency` and `ap-after-station`, where firmware recovery, USB
  serial visibility, and Wi-Fi/BLE target state must be distinguished.
- If hardware tests are skipped for firmware-impacting work, explicitly report
  that hardware tests were not run, why they were skipped, what native/host
  checks were run instead, and whether the change still needs hardware
  confirmation.

## Git Workflow

- Work on the current branch and checkout unless the user specifically asks for
  a separate branch or worktree.
- For slice-based implementation work, commit and push each completed,
  verified slice before moving on to the next slice.
- Git index-writing commands must run outside the Codex sandbox. Sandboxed
  staging and commits cannot reliably create or update `.git/index.lock` in
  this environment, so use escalated command execution for `git add`,
  `git commit`, and similar index-writing commands instead of trying once in
  the sandbox.
- GitHub CLI commands that need authentication, repository API access, or
  network access must run outside the Codex sandbox. If `gh` works on the host
  but fails in the sandbox, treat that as expected environment isolation, not as
  an auth problem to debug.
- **Never use `git add -A` or `git add .`.** Always stage specific files by
  path. Unrelated working-tree files (scratch scripts, experiment apps, local
  test fixtures) must not be committed by accident. Use `git add <file>` for
  each file that belongs in the commit, and `git status` before committing to
  verify only intended files are staged.
- **Never amend, force-push, or rewrite history without explicit user request.**
  If a commit contains mistakes, ask the user how to handle it — do not
  silently reset, revert, or amend. Reverts and resets make history harder
  to follow and can destroy work.

## CLI Workflow Ergonomics

- Before 1.0, prefer developer-friendly CLI defaults when they are safe and
  unambiguous. For example, package creation may write a sensible default
  output in the current directory while still offering `--out` for explicit
  paths.

## Verification Commands

Which checks to run per change area (Rust, browser-sim, targets, docs), the
expected baseline checks, and the Hello Menu proof checklist live in
`docs/standards/verification-commands.md`.
