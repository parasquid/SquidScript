# BLE Object Transfer — Implementation Plan

**Status**: implementation plan, follows the design spec
**Date**: 2026-06-05
**Depends on**: `docs/specs/2026-06-05-ble-object-transfer-design.md`

## Approach

TDD-first. Native ztest-driven for firmware, Rust unit-test-driven for compiler, pytest-driven for the host driver. Hardware verification last (XIAO ESP32-C3 e-paper default dev target).

Each slice is a self-contained commit. The slices are designed to keep the safety net green between slices: a slice that adds a failing test must also add the minimum implementation to make it pass, even if the implementation is incomplete.

The slice order is bottom-up: build the deepest pieces first (FFI exports, parsers, lifecycle hooks) and stack the higher-level integration last. This keeps the test surface small at each step.

## Slice 1: Design + spike (DONE)

`docs/specs/2026-06-05-ble-object-transfer-design.md`, `AGENTS.md` (spec-location rule), `ROADMAP.md` (deferred doc-parse entry), `docs/plans/2026-06-05-ble-object-transfer.md` (this doc). Committed as `79ab8b3` (amended).

## Slice 2: Compiler — FFI additions and `app.install` builtin

**Goal**: Rust FFI export `sqvm_app_install_file` + C-side callback in `vm_runtime_app_lifecycle.c`. The new builtin exists at the VM ABI level but the BLE object-transfer path is not yet wired (that's slice 8).

**Files**:
- `compiler/rust/crates/squidvm-ffi/src/lib.rs`: add `sqvm_app_install_file` C ABI
- `compiler/rust/crates/squidvm-ffi/abi/manifest.json`: add the new export
- `compiler/rust/crates/squidvm-ffi/tests/ffi_dispatch.rs`: Rust unit test
- `scripts/tests/test_zephyr_ffi_abi.py`: assert the new export is in the manifest
- `firmware/zephyr/src/squidvm_ffi.h`: declare `sqvm_app_install_file`
- `firmware/zephyr/src/squidvm_ffi.c`: implement `sqvm_app_install_file`
- `firmware/zephyr/src/vm_runtime_app_lifecycle.c`: add `runtime_app_install_file` callback
- `firmware/zephyr/src/vm_runtime_internal.h`: declare the callback
- `firmware/zephyr/src/generated_runtime_callbacks.inc`: add the callback to `runtime_callbacks`
- `firmware/zephyr/src/runtime_limits.h`: add `SQ_VM_RUNTIME_BLE_PROFILE_ARMED_MAX = 2`

**Failing test (Rust, written first)**:
- `tests/ffi_dispatch.rs`: assert that `sqvm_app_install_file` is exported with the right prototype
- Will fail because the symbol doesn't exist yet.

**Failing test (C ztest, written first; new file `firmware/zephyr/tests/ble-app-install/`)**:
- `install.test.c`: test that `runtime_app_install_file` rejects invalid app_id, rejects non-SQBC magic, succeeds on a known SQBC
- Will fail because the C callback doesn't exist yet.

**Implementation**:
- Add the FFI export and C callback with minimum logic (call `sq_app_store_install_app` with bytes from the file ref, return success/error)
- Update the manifest
- Update the generated callbacks
- `app_id[SQ_APP_STORE_APP_ID_MAX]` (no `+ 1`) per the existing `app_lifecycle.h:23` convention

**Verification**:
- `cargo test -p squidvm-ffi` (Rust unit test passes)
- `cargo test -p squidc` (manifest assertion passes)
- `west build -b native_sim -t run firmware/zephyr/tests/ble-app-install` (ztest passes)

## Slice 3: Zephyr firmware — `ble_ots.c` skeleton

**Goal**: New `ble_ots.c` file with the `bt_ots_cb` struct, the `obj_name_written` / `obj_created` / `obj_write` / `obj_cal_checksum` / `obj_read` callbacks stubbed out. The OTS service is registered; the GATT database advertises UUID 0x1825.

**Files**:
- `firmware/zephyr/src/ble_ots.h` (new): trigger table struct, lookup API, pending event slot
- `firmware/zephyr/src/ble_ots.c` (new): OTS callbacks (stubbed), registration
- `firmware/zephyr/src/CMakeLists.txt`: add `ble_ots.c` to the build
- `firmware/zephyr/prj.conf`: add `CONFIG_BT_OTS=y` and friends (per the spec's "Kconfig surface" section)
- `firmware/zephyr/tests/ble-ots-init/` (new): ztest that verifies the OTS service is registered

**Failing test (written first)**:
- `init.test.c`: register the OTS service in a ztest, assert that `bt_ots_svc_decl_get()` returns a non-NULL handle
- Will fail because `ble_ots.c` doesn't exist yet.

**Implementation**:
- Stub out the callbacks with `LOG_DBG` and return 0
- `bt_ots_init` with Feature bits `Create | Write | Execute | Abort`
- Register the service in the main init path

**Verification**:
- `west build -b native_sim -t run firmware/zephyr/tests/ble-ots-init`
- Manual check: GATT discover on a flashed XIAO target shows UUID 0x1825

## Slice 4: Zephyr firmware — `parse_ble_object_name`

**Goal**: Implement and unit-test the Object Name parser.

**Files**:
- `firmware/zephyr/src/ble_ots.c`: add `parse_ble_object_name` (per the spec)
- `firmware/zephyr/tests/ble-ots-parse/` (new): ztest for the parser

**Failing test (written first)**:
- `parse.test.c`: test cases for valid inputs, format errors, unsafe app_id, empty segments, extension not starting with `.`, name longer than buffer
- Will fail because the function doesn't exist yet.

**Implementation**: per the spec's `parse_ble_object_name` code.

**Verification**: ztest passes for all cases.

## Slice 5: Zephyr firmware — staging lifecycle

**Goal**: Implement and unit-test the staging file lifecycle. Open on `obj_created`, write on `obj_write`, close + `fs_unlink` on disconnect mid-stream and on OACP Abort. The pending-event slot is populated on `obj_write` finalization.

**Files**:
- `firmware/zephyr/src/ble_ots.c`: add `sq_ble_ots_reset_session`, `fs_unlink` plumbing
- `firmware/zephyr/src/ble_ots.h`: declare the helper
- `firmware/zephyr/tests/ble-ots-staging/` (new): ztest with a real LittleFS partition

**Failing test (written first)**:
- `lifecycle.test.c`: test that:
  - `obj_created` opens the staging file
  - `obj_write` writes chunks to the file
  - `sq_ble_ots_reset_session` `fs_unlink`s the staging file
  - OACP Abort path `fs_unlink`s and clears the in-flight session
  - Second `OACP Create` while busy returns `_OBJ_LOCKED`
- Will fail because the lifecycle hooks don't exist yet.

**Implementation**: per the spec's "Staging lifecycle" and "In-flight session policy" sections.

**Verification**: ztest passes on `native_sim` with a real LittleFS mount.

## Slice 6: Zephyr firmware — event dispatch handoff

**Goal**: Wire the OTS callbacks to the `sq_device_protocol_poll()` poll path. The pending-event slot is populated by `obj_write` (final chunk), drained by the poll, which calls into the existing `SQ_APP_LIFECYCLE_STEP_START_APP` mechanism to launch the armed app and dispatch the event.

**Files**:
- `firmware/zephyr/src/ble_ots.c`: add the producer side (populate slot on `obj_write` final)
- `firmware/zephyr/src/device_protocol.c`: add the consumer side (drain slot in `sq_device_protocol_poll`)
- `firmware/zephyr/src/ble_ots.h`: declare `sq_ble_ots_drain_pending_event()`
- `firmware/zephyr/tests/ble-ots-dispatch/` (new): ztest for the handoff

**Failing test (written first)**:
- `dispatch.test.c`: simulate an OTS write complete (call the producer side directly), then run the poll once, assert that the lifecycle step is `SQ_APP_LIFECYCLE_STEP_START_APP` with the right `app_id` and `event` (`ble.object.complete`), and that the staging file is `fs_unlink`d after the simulated event handler returns.
- Will fail because the consumer side doesn't exist yet.

**Implementation**: per the spec's "Producer → consumer handoff" section.

**Verification**: ztest passes.

## Slice 7: Trigger table wiring

**Goal**: `register_app_ble_profile_triggers` and `clear_app_ble_profile_triggers` in `device_protocol.c`, mirroring the existing `register_app_triggers` pattern at `device_protocol.c:1203-1253`.

**Files**:
- `firmware/zephyr/src/device_protocol.c`: add the new functions
- `firmware/zephyr/src/ble_ots.h`: declare the lookup API
- `firmware/zephyr/src/ble_ots.c`: implement `sq_ble_profile_lookup`
- `firmware/zephyr/tests/ble-trigger-table/` (new): ztest for add/remove/cap/all-or-nothing

**Failing test (written first)**:
- `trigger-table.test.c`: per the spec's "Native ztests" section. Add, remove, cap enforcement, all-or-nothing arming, lookup miss/hit, `Reset` clears table.
- Will fail because the functions don't exist yet.

**Implementation**: mirror the existing `register_app_triggers` pattern. Reject with `-EINVAL` on cap exceeded (matching the `trigger_count > SQ_VM_RUNTIME_ARMED_TIMER_MAX` hard-fail at `device_protocol.c:1243`).

**Verification**: ztest passes.

## Slice 8: `app.install(fileRef, appId)` end-to-end

**Goal**: Wire the new builtin to the actual install function. `sq_app_store_install_from_file_ref` reads the staging file, validates the SQBC magic, calls `sq_app_store_install_app`, updates the registry, `fs_unlink`s the staging file.

**Files**:
- `firmware/zephyr/src/app_store.h`: declare the new function
- `firmware/zephyr/src/app_store.c`: implement it (1 KiB caller-owned scratch buffer; SQBC magic check; calls `sq_app_store_install_app`)
- `firmware/zephyr/tests/ble-app-install/` (extended): ztest for the file-ref variant
- `firmware/zephyr/tests/ble-ots-dispatch/` (extended): ztest that drives a full BLE-shaped object transfer and verifies the file is delivered to the app, then `app.install` succeeds

**Failing test (written first)**:
- `install-from-file-ref.test.c`: ztest that creates a file at `/sq/tmp/ble-object-test.tmp` with a valid SQBC magic, calls `sq_app_store_install_from_file_ref("test", staging_path)`, asserts the app is registered; second test with bad magic asserts the call returns `-EINVAL` and the file is left for cleanup.
- End-to-end test in `ble-ots-dispatch`: pushes a known SQBC through the OTS callbacks (function-pointer stubs), polls, asserts the event fires, asserts `app.install` succeeds.

**Implementation**: per the spec's "`app.install(fileRef, appId)` builtin (new)" section.

**Verification**: all ztests pass.

## Slice 9: Examples and language-spec doc updates

**Goal**: Add `examples/ble-install/main.squid` and update the docs (language spec, runtime limits, etc.).

**Files**:
- `examples/ble-install/main.squid` (new): the ble-install example. Subscribes to `ble.object.complete`, calls `app.install(file_ref, "installed-app")` for each received file.
- `examples/ble-install/README.md` (new)
- `docs/language_spec.md` section 30: per the spec's "Language spec changes" — replace the "Firmware should validate" rule, add the ephemeral staging rule.
- `docs/language_spec.md` section 32: add `app.install(fileRef, appId)`.
- `docs/sqbc_binary_format.md` section 10: remove the `sink` field note, add the `file.*` ref note.
- `docs/runtime_limits.md`: add the `SQ_VM_RUNTIME_BLE_PROFILE_ARMED_MAX` row; tighten the `runtime_limits.md:36` fragment.
- `docs/zephyr_vm_host_abi_coverage.md`: add the BLE family to the table.
- `docs/firmware_app_storage.md`: document the BLE staging path family and `sq_app_store_install_from_file_ref`.
- `docs/hardware_target_tests.md`: add the new test wrapper, the skip pattern, the coverage boundary.
- `docs/firmware_state_machines.md`: add the BLE Object Transfer section.
- `docs/firmware_build_architecture.md`: add the BLE family to the C stack report and trigger-table diagram.

**No new tests**: the example is verified by the hardware test (slice 10). The doc changes are visible to native ztests only via the FFI manifest assertions.

## Slice 10: Host test driver + hardware test

**Goal**: Python `ots-push` driver and `scripts/zephyr-test-ble-object-transfer.sh` hardware wrapper.

**Files**:
- `tools/ots-push/` (new): the Python package
- `tools/ots-push/tests/` (new): pytest with a mock `bleak` backend
- `scripts/zephyr-test-ble-object-transfer.sh` (new): the hardware wrapper
- `docs/hardware_target_tests.md`: add the new test to the inventory (overlaps with slice 9)
- `docs/squidc_cli.md`: add any new CLI surface (if `squidc device ble-ots` is added; not in current spec)

**Failing test (Python, written first)**:
- `tests/test_ots_push.py`: mock `bleak` and assert that the driver calls the right GATT/CoC methods in the right order (discover, write object name, OACP Create, L2CAP CoC write, OACP Execute).
- Will fail because the package doesn't exist yet.

**Implementation**: per the spec's "Test driver" section. L2CAP CoC only; explicit skip messages for bleak unavailability and CoC unavailability.

**Verification**:
- `pytest tools/ots-push/tests/` (Python tests pass)
- `scripts/zephyr-test-ble-object-transfer.sh` (hardware test runs end-to-end on a connected XIAO, or skips cleanly)

## Slice 11: RAM verification

**Goal**: Build the firmware, measure linker DRAM, document the OTS additions to the baseline.

**Files**:
- `ROADMAP.md`: add the new OTS cost to the "ESP32-C3 RAM Hardening" section; remove the "Complete BLE object-transfer runtime support" item (the slice is done); add any follow-up items that emerged.
- `docs/firmware_build_architecture.md`: add the new components to the C stack report.

**Verification**:
- `cargo run -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd` shows the new linker DRAM
- Compare against the pre-slice baseline (31,616 bytes SquidScript-owned) and report the delta
- If the delta exceeds a sensible threshold (e.g., > 20 KiB total), open a follow-up ROADMAP item for further RAM reduction

## Slice 12: Final docs pass and ROADMAP cleanup

**Goal**: After all implementation slices land, mark the BLE object-transfer slice done in `ROADMAP.md` and clean up any deferred doc-parse work that surfaced.

**Files**:
- `ROADMAP.md`: remove the "Complete BLE object-transfer runtime support" item (if not done in slice 11)
- `ROADMAP.md`: add any follow-up items that emerged during implementation
- Address the deferred "Concepts" section in `docs/language_spec.md` if a follow-up agent gets to it; otherwise leave it as a follow-up

## Cross-cutting concerns

- **TDD discipline**: every slice writes its failing test first, then implements. Do not skip this even for "obvious" code like the parser.
- **Native ztest before hardware**: each firmware slice is verified on `native_sim` before the hardware test is updated. The hardware test is a thin wrapper that runs the same code paths.
- **Spec drift**: if implementation reveals a spec error, update the spec in the same slice's commit. Do not let spec and code drift.
- **ROADMAP follow-ups**: if a slice discovers a concrete follow-up, add it to `ROADMAP.md` in the same commit.
- **Slice granularity**: aim for slices that are 1-2 hours of focused work, with one verification command at the end. If a slice grows beyond that, split it.
