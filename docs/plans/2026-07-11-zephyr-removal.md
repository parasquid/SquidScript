# Zephyr Removal Implementation Plan

**Goal:** Delete the replaced Zephyr firmware and every repository surface that
exists only to build, test, configure, document, or select it. Native Rust
firmware remains the sole firmware implementation, with XTEINK X4 as the real
hardware target.

**Execution rule:** Do not keep Zephyr compiling during this work. Remove old
forms directly before 1.0; do not add backend aliases, compatibility readers,
deprecated flags, or migration diagnostics. Preserve only code with a proven
non-Zephyr owner.

## Acceptance

- `firmware/zephyr` and Zephyr-only hardware fixtures are absent.
- XIAO and ESP32-C3 Super Mini target definitions, generated target docs, and
  examples that exist only for those targets are absent.
- `squidvm-ffi`, its manifest/code generators, generated C headers, and C ABI
  tests are absent unless an audited non-Zephyr caller still requires them.
- `squidc target build|flash|monitor|doctor|inspect` model native firmware
  directly, with no backend enum, Zephyr planning branch, west argument escape,
  Kconfig generation, Zephyr status fields, or legacy toolchain checks.
- Current docs describe only native firmware. Historical design material is
  deleted when it only explains removed implementation; retained architecture
  docs contain no current Zephyr instructions.
- Repository searches find no active west, Twister, Kconfig, Zephyr SDK,
  `SQUID_ZEPHYR_*`, XIAO, Super Mini, or firmware-backend dependency outside
  explicitly retained historical records.
- The complete native software verification bundle and target-aware X4
  hardware inventory pass after removal.

## Task 1: Freeze Ownership And Deletion Inventory

- [x] Record the current commit and clean/dirty state in `.current_agent_work`.
- [x] Enumerate tracked paths under `firmware/zephyr`, Zephyr-only scripts,
  targets, tests, examples, generated docs, C headers, and build configuration.
- [x] Search every `squidvm-ffi` consumer. Classify each as Zephyr-only,
  independently owned, or ambiguous; resolve ambiguous owners from code/tests
  before deletion.
- [x] Search native firmware, compiler, simulator, CLI, and shared crates for
  imports from paths scheduled for deletion. Add any required native owner to
  this plan before removing its source.
- [x] Save the inventory in a temporary ignored file; do not turn the plan into
  an investigation transcript.

## Task 2: Remove Zephyr Firmware And Target Fixtures

- [x] Delete `firmware/zephyr` in one reviewable slice.
- [x] Delete Zephyr-only scripts, west/Twister setup, Kconfig generators, RAM
  audit helpers, overlays, and hardware fixtures.
- [x] Delete XIAO and ESP32-C3 Super Mini target JSON, runtime-limit metadata,
  examples, and generated target tables with no native owner.
- [x] Remove Zephyr metadata from `targets/xteink-x4.target.json`; retain only
  native image, partition, flashing, runtime, and capability facts.
- [x] Regenerate tracked target documentation from the remaining target JSON.
- [x] Search deleted basenames across docs, README, roadmap, tests, scripts, and
  related commit messages before committing the slice.

## Task 3: Remove Or Rehome The C FFI Surface

- [x] Delete `squidvm-ffi`, ABI manifests, generated headers, equivalence tests,
  and `scripts/check-squidvm-ffi-abi.py` when the ownership audit proves no
  non-Zephyr caller remains.
- [x] If a non-Zephyr caller remains, move only its required typed contract to
  the lowest shared Rust owner; do not retain C ABI machinery speculatively.
- [x] Remove workspace members, dependencies, CI jobs, docs, and generators
  that referenced the deleted ABI.
- [x] Run compiler, SQBC, VM, and native firmware tests after this slice to
  catch accidental shared-semantics deletion.

## Task 4: Collapse CLI And Target Modeling To Native

- [x] Remove `TargetBackend`, Zephyr target structs, backend switches, Zephyr
  command planners, environment discovery, Kconfig generation, and west args.
- [x] Make native build, image generation, flash, monitor, doctor, and inspect
  fields direct target operations rather than one branch of a backend model.
- [x] Remove CLI flags and JSON fields that exist only for Zephyr, including
  stack/pristine controls and Zephyr support summaries.
- [x] Update parser/plan tests to assert the native-only public command shape.
  Delete legacy-form tests rather than preserving removed flags.
- [x] Ensure `hardware test --target xteink-x4 --list` still selects only the
  complete native inventory and `--port` reaches recovery flash and all runners.

## Task 5: Remove Legacy Documentation And CI

- [x] Delete Zephyr-only reference docs and plans that have no durable language
  or architecture value after implementation removal.
- [x] Rewrite mixed docs around current native facts; remove legacy sections
  instead of labeling them as an alternative backend.
- [x] Remove Zephyr CI jobs, dependencies, caches, setup instructions, and
  release artifacts.
- [x] Remove stale roadmap items that ask for Zephyr parity or legacy-target
  expansion. Preserve unrelated native roadmap work.
- [x] Run a repository-wide active-reference search and classify every match;
  no unexplained active Zephyr dependency may remain.

## Task 6: Final Native-Only Verification

- [x] Run formatting and `git diff --check` on all changed files.
- [x] Run `cargo test -p squidc-core`, `squidvm-core`,
  `squid-device-protocol`, `squidscript-fw-core`,
  `squidscript-fw-x4 --features x4-binbook`, and `squidc --bin squidc`.
- [x] Build and recovery-flash `xteink-x4` through `squidc target` with no
  backend or legacy toolchain environment.
- [x] Run `squidc hardware test --target xteink-x4` sequentially on the final
  image, including serial OTA and radio coexistence.
- [x] Verify final lifecycle/resources/errors are idle and clean, and capture a
  fresh live panel image from a visible SquidScript app.
- [x] Commit in independently verified deletion slices. The final commit must
  state that native firmware is the sole remaining implementation.
