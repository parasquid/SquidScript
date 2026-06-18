# Imperative BLE Object Receive Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (chosen) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the declarative, armed-gated `service.ble.profile(...)` trigger with an imperative `service.ble.start(profile, config)` / `service.ble.stop()` service the app drives from event handlers, dissolving the armed-app `app.launch` → `-5` failure.

**Architecture:** The profile config (`id`, `accept`, `events`) stays a compile-time static literal encoded into the SQBC BLE-profile section — but it is now sourced from `ServiceBleStart` *statements* anywhere in app code, not from `app.triggers`. `service.ble.start` mirrors the `service.timer.every` lowering exactly: emit the profile `id` string, then `BUILTIN_SERVICE_BLE_START`. At runtime the firmware callback finds the matching static config by `id` (via the existing `ble_profile_*_from_reader` readers), registers it into the existing profile table for the *running* app, and starts advertising. `service.ble.stop` emits `BUILTIN_SERVICE_BLE_STOP`; the callback clears the running app's profile and stops advertising once the table is empty. This reuses the `SqvmBleProfileTrigger` struct, the section readers, and `sq_ble_profile_table_*` verbatim — only *when* registration happens (runtime opcode vs. arm-step) and advertising gating change.

**Config-passing decision (resolved):** id-string mechanism, not a new operand format and not runtime-marshalled records. Rationale: the config is a static literal (same shape as the old declaration), so compile-time encoding + id lookup reuses all tested machinery and matches the established push-string-then-builtin pattern. If you disagree, stop before Slice 1.

**Tech Stack:** Rust (`squidc-core`, `squidvm-core`, `squidvm-ffi`), Zephyr C (`firmware/zephyr`), Python ABI generator (`scripts/check-squidvm-ffi-abi.py`), SquidScript example + Markdown docs.

---

## File Structure

Files created or modified, by slice:

**Slice 1 — Compiler (`squidc-core`)**
- Modify: `compiler/rust/crates/squidc-core/src/ir.rs` — add `ServiceBleStart`/`ServiceBleStop` statement variants; remove `ServiceBleProfile`; remove `IrBleProfileTrigger` from `IrTrigger.ble` and the struct.
- Modify: `compiler/rust/crates/squidc-core/src/parser/statements.rs:197-248` — parse `start`/`stop`, remove `profile`.
- Modify: `compiler/rust/crates/squidc-core/src/compile.rs` — drop `ServiceBleProfile` → trigger lowering (`trigger_from_statement` ~1003-1041); add `collect_ble_profiles(ir)` walk.
- Modify: `compiler/rust/crates/squidc-core/src/sqbc.rs` — add `BUILTIN_SERVICE_BLE_START`/`_STOP`; emit them; re-source `encode_ble_triggers` from `ServiceBleStart` statements; arg-count validation.
- Modify: `compiler/rust/crates/squidc-core/src/semantic.rs` — move BLE validation out of `validate_trigger_statements`; validate `ServiceBleStart` config wherever it appears; remove `ServiceBleProfile` arms.
- Test: `compiler/rust/crates/squidc-core/src/tests.rs` — replace the four BLE trigger tests with start/stop tests.

**Slice 2 — VM (`squidvm-core`)**
- Modify: `compiler/rust/crates/squidvm-core/src/bytecode.rs` — add `BUILTIN_SERVICE_BLE_START`/`_STOP` constants.
- Modify: `compiler/rust/crates/squidvm-core/src/vm.rs` — add both to the `bytecode::{...}` import list (shadowing canary) and add dispatch arms.
- Modify: `compiler/rust/crates/squidvm-core/src/host.rs` — add `service_ble_start(&str)` / `service_ble_stop()` trait methods.
- Test: `compiler/rust/crates/squidvm-core/src/tests.rs` — `TestHost` records start/stop; source→VM→host dispatch test.

**Slice 3 — FFI (`squidvm-ffi`)**
- Modify: `compiler/rust/crates/squidvm-ffi/abi/manifest.json` — add `ble_start`/`ble_stop` callback entries.
- Regenerate: `compiler/rust/crates/squidvm-ffi/src/generated_callbacks.rs` and any generated result-default/C files via `scripts/check-squidvm-ffi-abi.py`.
- Modify: `compiler/rust/crates/squidvm-ffi/src/lib.rs` — wire the new callbacks into the `ChunkedVmHost` impl that bridges to function pointers.
- Test: `compiler/rust/crates/squidvm-ffi/tests/ffi_dispatch.rs` — add callbacks + dispatch equivalence tests.

**Slice 4 — Firmware (`firmware/zephyr`)**
- Modify: `firmware/zephyr/runtime_limits.json` + regenerated `src/runtime_limits.h` — rename `SQ_VM_RUNTIME_BLE_PROFILE_ARMED_MAX` → `SQ_VM_RUNTIME_BLE_PROFILE_MAX`.
- Modify: `firmware/zephyr/src/device_protocol.c` — remove `register_app_ble_profile_triggers` arm-step call (~1559); the registration moves to the start callback.
- Create/Modify: a `vm_runtime_ble.c` (mirroring `vm_runtime_wifi.c`) implementing the `ble_start`/`ble_stop` callbacks.
- Modify: `firmware/zephyr/src/ble_smoke.c` — gate advertising on profile-table non-empty; remove unconditional boot advertise (`main.c:152` / `ble_smoke.c:177`).
- Modify: `firmware/zephyr/src/squidvm_ffi.h` — add the generated BLE callback typedefs + struct fields.
- Test: `firmware/zephyr/tests/ble-trigger-table/` (rename macro) + a new ztest for register-on-start / clear-on-stop / advertising gate.

**Slice 5 — Example + docs**
- Modify: `examples/ble-install/main.squid` — `service.ble.start` in an `app.start` handler.
- Modify: `docs/language_spec.md` (~2390-2462), `docs/runtime_limits.md` (~35-47), `docs/hardware_target_tests.md`.
- Modify: `ROADMAP.md` (remove the foreground-gated item, ~31-54) and `ICEBOX.md` per AGENTS.md drop discipline.

---

## Slice 1 — Compiler (`squidc-core`)

Run all commands from repo root. Test command throughout: `cargo test -p squidc-core`.

### Task 1.1: IR — add start/stop variants, remove profile

**Files:**
- Modify: `compiler/rust/crates/squidc-core/src/ir.rs:68-126`
- Test: `compiler/rust/crates/squidc-core/src/tests.rs`

- [ ] **Step 1: Write the failing test** (in `tests.rs`, replacing `parses_ble_object_transfer_trigger_and_payload_handler` at ~1747)

```rust
#[test]
fn parses_service_ble_start_and_stop_statements() {
    let source = r#"app "ble-install"

event.on("app.start") {
  service.ble.start("object-transfer", {
    id: "sqbc-install",
    accept: [".sqbc"],
    events: { complete: "ble.object.complete", error: "ble.object.error" }
  })
}

event.on("teardown") {
  service.ble.stop()
}
"#;
    let program = compile_source(source).expect("compiles");
    let stmts = handler_statements(&program, "app.start");
    assert!(matches!(
        stmts.first(),
        Some(IrStatement::ServiceBleStart { profile, id, accept, events })
            if profile == "object-transfer"
                && id == "sqbc-install"
                && accept == &vec![".sqbc".to_string()]
                && events.get("complete").map(String::as_str) == Some("ble.object.complete")
    ));
    let teardown = handler_statements(&program, "teardown");
    assert!(matches!(teardown.first(), Some(IrStatement::ServiceBleStop)));
}
```

(Use the existing test-helper conventions in `tests.rs` for `compile_source`/locating handler statements; if no `handler_statements` helper exists, inline the lookup the way the neighbouring trigger tests do.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p squidc-core parses_service_ble_start_and_stop_statements`
Expected: FAIL — `ServiceBleStart` / `ServiceBleStop` not a variant.

- [ ] **Step 3: Edit `ir.rs`**

Remove the `ble: Option<IrBleProfileTrigger>` field from `IrTrigger` (lines ~68-75) and delete the `IrBleProfileTrigger` struct (lines ~77-84). Remove the `ServiceBleProfile` statement variant (lines ~119-126) and add:

```rust
#[serde(rename = "service.ble.start")]
ServiceBleStart {
    profile: String,
    id: String,
    accept: Vec<String>,
    events: BTreeMap<String, String>,
},
#[serde(rename = "service.ble.stop")]
ServiceBleStop,
```

(`role` is dropped — server is the only role; the old validator already required `role == "server"`.)

- [ ] **Step 4: Run** `cargo test -p squidc-core` — expect compile errors in `parser/statements.rs`, `compile.rs`, `sqbc.rs`, `semantic.rs` referencing removed variants. That is expected; the next tasks fix them. Re-run the single test once those compile.

- [ ] **Step 5: Commit** (after Slice 1 fully compiles & passes — do not commit a non-compiling tree). See Task 1.6.

### Task 1.2: Parser — parse start/stop, remove profile

**Files:**
- Modify: `compiler/rust/crates/squidc-core/src/parser/statements.rs:197-248`

- [ ] **Step 1:** The failing test from Task 1.1 already covers this. Keep it red.

- [ ] **Step 2: Replace the `if method == "ble"` block** (lines 197-248) with:

```rust
if method == "ble" {
    return match action.as_str() {
        "start" => {
            let profile = self.consume_string(builder).unwrap_or_default();
            self.consume_comma(builder);
            let options = self.parse_static_options_object(builder);
            self.consume_call_tail(builder);
            let id = options
                .get("id")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let accept = options
                .get("accept")
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let events = options
                .get("events")
                .and_then(|value| value.as_object())
                .map(|events| {
                    events
                        .iter()
                        .filter_map(|(key, value)| {
                            value.as_str().map(|value| (key.clone(), value.to_string()))
                        })
                        .collect::<BTreeMap<_, _>>()
                })
                .unwrap_or_default();
            Some(IrStatement::ServiceBleStart {
                profile,
                id,
                accept,
                events,
            })
        }
        "stop" => {
            self.consume_call_tail(builder);
            Some(IrStatement::ServiceBleStop)
        }
        _ => {
            self.consume_call_tail(builder);
            None
        }
    };
}
```

- [ ] **Step 3: Run** `cargo test -p squidc-core parses_service_ble_start_and_stop_statements` — still red until lowering/encoding compile (Tasks 1.3-1.4), but the parser logic is now correct.

### Task 1.3: Compile/lowering — drop trigger lowering, add profile collection

**Files:**
- Modify: `compiler/rust/crates/squidc-core/src/compile.rs` (`trigger_from_statement` ~1003-1041 and the trigger-collection loop ~133-182)

- [ ] **Step 1:** Remove the `IrStatement::ServiceBleProfile { .. } => Some(IrTrigger { ble: Some(...) })` arm from `trigger_from_statement`; `ServiceBleStart`/`ServiceBleStop` are **not** triggers, so `trigger_from_statement` returns `None` for them (they fall through the existing catch-all). Remove any remaining references to `IrTrigger.ble` / `IrBleProfileTrigger`.

- [ ] **Step 2: Add a deterministic profile-collection helper** used by the SQBC encoder (Task 1.4). It walks every statement body in the program (the same bodies the string table already interns over) and collects unique `ServiceBleStart` configs in document order, keyed by `id`:

```rust
pub(crate) struct BleProfile {
    pub profile: String,
    pub id: String,
    pub accept: Vec<String>,
    pub events: std::collections::BTreeMap<String, String>,
}

pub(crate) fn collect_ble_profiles(ir: &IrProgram) -> Vec<BleProfile> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    visit_statements(ir, &mut |stmt| {
        if let IrStatement::ServiceBleStart { profile, id, accept, events } = stmt {
            if seen.insert(id.clone()) {
                out.push(BleProfile {
                    profile: profile.clone(),
                    id: id.clone(),
                    accept: accept.clone(),
                    events: events.clone(),
                });
            }
        }
    });
    out
}
```

Implement `visit_statements` against the IR program's actual handler/function/screen body structure (mirror however the string-table pass enumerates statements — reuse that traversal rather than writing a second one if one exists).

- [ ] **Step 3: Run** `cargo build -p squidc-core` — expect remaining errors only in `sqbc.rs`/`semantic.rs`.

### Task 1.4: SQBC — builtins, emit, re-source section

**Files:**
- Modify: `compiler/rust/crates/squidc-core/src/sqbc.rs` (constants ~67-95; emit ~808; `encode_ble_triggers` ~1389-1423; `builtin_for_call`/`validate_builtin_arg_count`)

- [ ] **Step 1: Write the failing SQBC test** (replace `encodes_ble_object_transfer_trigger_metadata_in_sqbc` ~1842):

```rust
#[test]
fn encodes_ble_profile_metadata_from_start_statement() {
    let source = r#"app "ble-install"

event.on("app.start") {
  service.ble.start("object-transfer", {
    id: "sqbc-install",
    accept: [".sqbc"],
    events: { complete: "ble.object.complete" }
  })
}
"#;
    let sqbc = compile_to_sqbc(source).expect("compiles");
    let section = read_section(&sqbc, SECTION_BLE_PROFILES).expect("section present");
    // count == 1
    assert_eq!(u16::from_le_bytes([section[0], section[1]]), 1);
}
```

(Match the existing SQBC-reading test helpers; keep the section-count assertion minimal and string-id-agnostic so it survives string-table reordering.)

- [ ] **Step 2: Run** → FAIL (no `SECTION_BLE_PROFILES`, statement encodes to nothing).

- [ ] **Step 3: Add builtin constants** near the other service builtins (after `BUILTIN_SERVICE_POWER_SLEEP`, next free opcode — verify the highest existing value and pick the next two, e.g.):

```rust
const BUILTIN_SERVICE_BLE_START: u8 = 0xc1;
const BUILTIN_SERVICE_BLE_STOP: u8 = 0xc2;
```

- [ ] **Step 4: Replace the emit arm** (line 808):

```rust
IrStatement::ServiceBleStart { id, .. } => {
    emit_string(unit, id)?;
    emit_builtin(&mut unit.code, BUILTIN_SERVICE_BLE_START);
}
IrStatement::ServiceBleStop => {
    emit_builtin(&mut unit.code, BUILTIN_SERVICE_BLE_STOP);
}
```

- [ ] **Step 5: Re-source the section encoder.** Rename `SECTION_BLE_TRIGGERS` → `SECTION_BLE_PROFILES` (keep the same numeric id `10`). Rewrite `encode_ble_triggers` (rename to `encode_ble_profiles`) to take the `collect_ble_profiles(ir)` output instead of `ir.triggers[].ble`, writing the same wire layout the FFI reader expects but **without the `role` field** if you also update the reader — OR keep writing a constant `"server"` role id to avoid touching the reader. **Decision: keep writing `"server"`** so the FFI reader (`ble_profile_read_from_reader`) is untouched in Slice 1:

```rust
fn encode_ble_profiles(profiles: &[BleProfile], strings: &StringTable) -> Result<Vec<u8>, SqbcError> {
    let mut out = Vec::new();
    write_u16(&mut out, u16::try_from(profiles.len()).map_err(|_| SqbcError::new("too many BLE profiles"))?);
    for p in profiles {
        write_u16(&mut out, string_id(strings, &p.profile)?);
        write_u16(&mut out, string_id(strings, &p.id)?);
        write_u16(&mut out, string_id(strings, "server")?); // role, fixed
        write_u16(&mut out, u16::try_from(p.accept.len()).map_err(|_| SqbcError::new("too many BLE accept extensions"))?);
        for ext in &p.accept { write_u16(&mut out, string_id(strings, ext)?); }
        write_u16(&mut out, u16::try_from(p.events.len()).map_err(|_| SqbcError::new("too many BLE event routes"))?);
        for (kind, event) in &p.events {
            write_u16(&mut out, string_id(strings, kind)?);
            write_u16(&mut out, string_id(strings, event)?);
        }
    }
    Ok(out)
}
```

Ensure `"server"` is interned into the string table (the string-collection pass must add it; add it explicitly alongside the profile strings if collection is driven by `collect_ble_profiles`). Update the call site that previously fed `ir` to feed `collect_ble_profiles(ir)`.

- [ ] **Step 6: Add arg-count validation** if `validate_builtin_arg_count` is keyed by builtin: `BUILTIN_SERVICE_BLE_START` consumes 1 stack string (the id) it emits itself, `BUILTIN_SERVICE_BLE_STOP` 0. Since these are emitted as statement variants (not `Call`), they likely bypass `validate_builtin_arg_count`; confirm and skip if so.

- [ ] **Step 7: Run** `cargo test -p squidc-core encodes_ble_profile_metadata_from_start_statement` — expect PASS.

### Task 1.5: Semantic — validate start config, reject in triggers

**Files:**
- Modify: `compiler/rust/crates/squidc-core/src/semantic.rs` (`validate_trigger_statements` ~193-235, `validate_ble_profile_trigger` ~237-275, and the BLE arms at ~495/729/934/1040)

- [ ] **Step 1: Write failing tests** (replace `rejects_ble_object_transfer_trigger_without_id` and `rejects_duplicate_ble_object_transfer_profile_ids`):

```rust
#[test]
fn rejects_service_ble_start_without_id() {
    let source = r#"app "x"
event.on("app.start") {
  service.ble.start("object-transfer", { accept: [".sqbc"], events: { complete: "e" } })
}
"#;
    let diags = compile_diagnostics(source);
    assert!(diags.iter().any(|d| d.code == "E_BLE_PROFILE"));
}

#[test]
fn rejects_service_ble_in_app_triggers() {
    let source = r#"app "x"
app.triggers {
  service.ble.start("object-transfer", { id: "i", accept: [".sqbc"], events: { complete: "e" } })
}
"#;
    let diags = compile_diagnostics(source);
    assert!(diags.iter().any(|d| d.code == "E_APP_TRIGGER_STATEMENT"));
}
```

- [ ] **Step 2: Run** → FAIL.

- [ ] **Step 3: In `validate_trigger_statements`**, remove the `ServiceBleProfile` arm and the `ble_profile_ids` parameter/dedup; `ServiceBleStart`/`ServiceBleStop` now fall to the catch-all `_ =>` arm that emits `E_APP_TRIGGER_STATEMENT` — exactly the desired rejection. Update the caller (~140-149) to drop the `ble_profile_ids` set.

- [ ] **Step 4: Add config validation where statements live.** Rename `validate_ble_profile_trigger` → `validate_ble_start` (drop the `role` and dedup-set params); call it from the statement-visitor pass that walks handler bodies (the pass at ~495/729/934/1040 that currently has BLE arms). Validation rules: `profile == "object-transfer"`, non-empty `id`, non-empty `accept` all starting with `.`, non-empty `events` with non-empty values. Emit `E_BLE_PROFILE` on violation. Update the other three BLE match arms to reference `ServiceBleStart`/`ServiceBleStop` instead of `ServiceBleProfile` (e.g. mark them as not affecting fallible-result tracking, same as before).

- [ ] **Step 5: Run** both new tests — expect PASS.

### Task 1.6: Slice 1 green + commit

- [ ] **Step 1: Run** `cargo test -p squidc-core` — expect all PASS (no lingering `ServiceBleProfile`/`IrBleProfileTrigger`/`SECTION_BLE_TRIGGERS` references).
- [ ] **Step 2: Run** `cargo test` from repo root — expect green (catches downstream crates that referenced the old IR; if `squidvm-ffi` reads the section it still works because the wire layout is unchanged).
- [ ] **Step 3: Commit**

```bash
git add compiler/rust/crates/squidc-core
git commit -m "feat(compiler): imperative service.ble.start/stop replacing app.triggers ble profile"
```

---

## Slice 2 — VM (`squidvm-core`)

Test command: `cargo test -p squidvm-core`.

### Task 2.1: Builtin constants

**Files:**
- Modify: `compiler/rust/crates/squidvm-core/src/bytecode.rs:97` (after `BUILTIN_SERVICE_POWER_SLEEP`)

- [ ] **Step 1:** Add (must match the `squidc-core` values from Task 1.4 exactly):

```rust
pub(crate) const BUILTIN_SERVICE_BLE_START: u8 = 0xc1;
pub(crate) const BUILTIN_SERVICE_BLE_STOP: u8 = 0xc2;
```

- [ ] **Step 2: Run** `cargo build -p squidvm-core` — expect PASS (unused-const warnings ok for now).

### Task 2.2: Host trait methods

**Files:**
- Modify: `compiler/rust/crates/squidvm-core/src/host.rs` (near the timer methods ~154-159)

- [ ] **Step 1:** Add default-erroring trait methods (mirror `service_timer_every`):

```rust
fn service_ble_start(&mut self, _id: &str) -> Result<(), VmError> {
    Err(VmError::InvalidOperand)
}
fn service_ble_stop(&mut self) -> Result<(), VmError> {
    Err(VmError::InvalidOperand)
}
```

- [ ] **Step 2: Run** `cargo build -p squidvm-core` — PASS.

### Task 2.3: VM dispatch + shadowing canary

**Files:**
- Modify: `compiler/rust/crates/squidvm-core/src/vm.rs` (import list ~3-27; `call_builtin` match ~1508)
- Test: `compiler/rust/crates/squidvm-core/src/tests.rs`

- [ ] **Step 1: Write the failing dispatch test** (in `tests.rs`; extend `TestHost` with `ble_start: Vec<String>` and `ble_stop: u32`, implement the two trait methods to record):

```rust
#[test]
fn dispatch_handles_service_ble_start_and_stop() {
    let sqbc = compile_ble_install_fixture(); // source with service.ble.start("object-transfer",{id:"sqbc-install",...}) in app.start and service.ble.stop() in another handler
    let mut host = TestHost::new();
    run_handler(&sqbc, &mut host, "app.start");
    assert_eq!(host.ble_start, vec!["sqbc-install".to_string()]);
    run_handler(&sqbc, &mut host, "teardown");
    assert_eq!(host.ble_stop, 1);
}
```

(Use the existing fixture/run helpers in `squidvm-core/src/tests.rs` — mirror `dispatch_handles_timer_every_builtin` style.)

- [ ] **Step 2: Run** → FAIL (`InvalidOperand` / no dispatch).

- [ ] **Step 3: Add both constants to the `use crate::bytecode::{...}` import list** (CRITICAL — per AGENTS.md, an unimported `BUILTIN_*` becomes a wildcard binding that shadows every builtin). Add `BUILTIN_SERVICE_BLE_START, BUILTIN_SERVICE_BLE_STOP` to the alphabetical list.

- [ ] **Step 4: Add dispatch arms** in `call_builtin` (after the timer arms ~1517):

```rust
BUILTIN_SERVICE_BLE_START => {
    let id = self.pop_sqbc_string_id()?;
    host.service_ble_start(self.index.string(id)?)?;
}
BUILTIN_SERVICE_BLE_STOP => {
    host.service_ble_stop()?;
}
```

- [ ] **Step 5: Run** `cargo test -p squidvm-core dispatch_handles_service_ble_start_and_stop` — PASS.

- [ ] **Step 6: Run** `cargo test -p squidvm-core` — full suite PASS (the shadowing canary: any unrelated wifi/indicator/timer dispatch test failing means the import in Step 3 was missed).

### Task 2.4: Commit

- [ ] **Step 1: Run** `cargo test` from repo root — green.
- [ ] **Step 2: Commit**

```bash
git add compiler/rust/crates/squidvm-core
git commit -m "feat(vm): BUILTIN_SERVICE_BLE_START/STOP dispatch to host"
```

---

## Slice 3 — FFI (`squidvm-ffi`)

Test command: `cargo test -p squidvm-ffi`. ABI gen: `python3 scripts/check-squidvm-ffi-abi.py` (see its `--help`/`--write` flag for regeneration mode).

### Task 3.1: ABI manifest entries

**Files:**
- Modify: `compiler/rust/crates/squidvm-ffi/abi/manifest.json`

- [ ] **Step 1:** Add two entries modelled on `timer_every`/`timer_after` (no output struct; `required_vm_error` policy):

```json
{
  "field": "ble_start",
  "typedef": "SqvmBleStartCallback",
  "family": "ble",
  "rust_type": "Option< unsafe extern \"C\" fn( user_data: *mut c_void, id: *const u8, id_len: usize, ) -> i32, >",
  "missing_policy": "required_vm_error",
  "test_fixture": "compile_ble_install_sqbc",
  "failing_callback": "failing_ble_start"
},
{
  "field": "ble_stop",
  "typedef": "SqvmBleStopCallback",
  "family": "ble",
  "rust_type": "Option< unsafe extern \"C\" fn( user_data: *mut c_void, ) -> i32, >",
  "missing_policy": "required_vm_error",
  "test_fixture": "compile_ble_install_sqbc",
  "failing_callback": "failing_ble_stop"
}
```

- [ ] **Step 2: Run the ABI checker in check mode** `python3 scripts/check-squidvm-ffi-abi.py` — expect it to report the generated files are now stale.

### Task 3.2: Regenerate generated files

**Files:**
- Regenerate: `compiler/rust/crates/squidvm-ffi/src/generated_callbacks.rs` (+ any generated C header / result-defaults the script owns)

- [ ] **Step 1:** Run the generator in write mode (confirm the exact flag from `--help`; the script generates the `SqvmCallbacks` struct fields, C prototypes, and dispatch test cases). 
- [ ] **Step 2: Run** `python3 scripts/check-squidvm-ffi-abi.py` again — expect clean (no drift).
- [ ] **Step 3: Inspect** `git diff` on generated files — confirm only the two new `ble_start`/`ble_stop` fields/prototypes were added.

### Task 3.3: Bridge callbacks into the host impl

**Files:**
- Modify: `compiler/rust/crates/squidvm-ffi/src/lib.rs` (the `ChunkedVmHost`/`TraceSink` impl that forwards to the `SqvmCallbacks` function pointers — find the `service_timer_every` forwarding impl as the template)

- [ ] **Step 1: Write the failing FFI dispatch test** in `tests/ffi_dispatch.rs` (add `ble_start: Vec<String>`, `ble_stop: u32` to its `TestHost`; add `unsafe extern "C"` callbacks mirroring `timer_every`; add `compile_ble_install_sqbc()` fixture):

```rust
#[test]
fn dispatch_handles_ble_start_and_stop_builtins() {
    let mut host = TestHost::new();
    // install callbacks incl. ble_start/ble_stop, dispatch app.start then teardown
    // ... mirror dispatch_handles_timer_every_builtin ...
    assert_eq!(host.ble_start, vec!["sqbc-install".to_string()]);
    assert_eq!(host.ble_stop, 1);
}
```

- [ ] **Step 2: Run** → FAIL.

- [ ] **Step 3: Implement the forwarding** in `lib.rs` — for `service_ble_start(id)`, call the `ble_start` fn pointer with `(user_data, id.as_ptr(), id.len())`, mapping a non-`Ok` status to `VmError`; honour `required_vm_error` when the pointer is `None` (mirror exactly how `service_timer_every` forwards). `service_ble_stop()` calls `ble_stop(user_data)`.

- [ ] **Step 4: Run** `cargo test -p squidvm-ffi dispatch_handles_ble_start_and_stop_builtins` — PASS.

- [ ] **Step 5: Run** `cargo test` from repo root — green.

### Task 3.4: Commit

```bash
git add compiler/rust/crates/squidvm-ffi scripts
git commit -m "feat(ffi): ble_start/ble_stop callbacks + regenerated ABI helpers"
```

---

## Slice 4 — Firmware (`firmware/zephyr`)

Native ztests via `scripts/zephyr-test-protocol.sh` and the ble-trigger-table test (run outside the sandbox — index-writing/Twister/serial are host-only per AGENTS.md). Build: `cargo run -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd`.

### Task 4.1: Rename the cap macro

**Files:**
- Modify: `firmware/zephyr/runtime_limits.json` → regenerate `firmware/zephyr/src/runtime_limits.h`
- Modify: every reference to `SQ_VM_RUNTIME_BLE_PROFILE_ARMED_MAX` (`ble_profile_table.c:8`, `device_protocol.c:1305-1307`, `docs/runtime_limits.md`, `firmware/zephyr/tests/ble-trigger-table/src/main.c`)

- [ ] **Step 1:** Rename to `SQ_VM_RUNTIME_BLE_PROFILE_MAX` in `runtime_limits.json`; regenerate the header via the documented generator (do not hand-edit the generated `.h`). 
- [ ] **Step 2:** `grep -rn SQ_VM_RUNTIME_BLE_PROFILE_ARMED_MAX firmware docs` → fix every hit. Expect zero remaining.
- [ ] **Step 3: Run** `bash -n` is N/A; build-compile the firmware (Task 4.5 gate) — defer full build to end of slice. For now `grep` clean is the check.

### Task 4.2: BLE start/stop callbacks (`vm_runtime_ble.c`)

**Files:**
- Create: `firmware/zephyr/src/vm_runtime_ble.c` (+ CMakeLists registration, mirroring `vm_runtime_wifi.c`)
- Modify: `firmware/zephyr/src/squidvm_ffi.h` — add generated `SqvmBleStartCallback`/`SqvmBleStopCallback` typedefs + struct fields (mirror the Slice 3 generated C output).

- [ ] **Step 1: Write the failing ztest** — new `firmware/zephyr/tests/ble-imperative-receive/` (mirror `ble-trigger-table`): given a compiled `ble-install` fixture loaded as the running app, calling the `ble_start` callback with `id="sqbc-install"` adds a profile-table entry for that app; `ble_stop` removes it. Assert `sq_ble_profile_table_count()` goes 0→1→0 and `sq_ble_profile_lookup(app, "sqbc-install")` hits after start.

- [ ] **Step 2: Run** the ztest via `scripts/zephyr-test-protocol.sh` (or the wrapper for that test dir) — FAIL (callback unimplemented).

- [ ] **Step 3: Implement `ble_start`**: it receives `id`; it must (a) resolve the running app's id from the runtime context (the same source `register_app_ble_profile_triggers` used for `app_id`); (b) locate the matching static config by scanning `sqvm_trigger_ble_profile_count_from_reader` + `..._read_from_reader` for the profile whose `id` matches; (c) `sq_ble_profile_table_remove_app(app_id)` then `sq_ble_profile_table_add(...)` (idempotent set/replace per spec); (d) start advertising via the gated entry point (Task 4.4). Return `0`/`-EINVAL` with legible errno names per AGENTS.md.

- [ ] **Step 4: Implement `ble_stop`**: `sq_ble_profile_table_remove_app(app_id)`; call `sq_ble_transfer_abort()` to drop any in-flight transfer; stop advertising if `sq_ble_profile_table_count() == 0` (Task 4.4).

- [ ] **Step 5: Run** the ztest — PASS.

### Task 4.3: Remove arm-step registration

**Files:**
- Modify: `firmware/zephyr/src/device_protocol.c` (delete the `register_app_ble_profile_triggers` call at ~1559 and the function ~1258-1345 if now unused; keep `sq_ble_profile_lookup`/routing intact)

- [ ] **Step 1:** Remove the `STEP_ARM` BLE registration call. Confirm the pending-event→drain→START_APP path (~1465-1519) is untouched — routing still works via the table populated by `ble_start`.
- [ ] **Step 2: Run** existing ble-ots-dispatch ztests — expect still green (drain/install/handoff unchanged).

### Task 4.4: Gate advertising on active profiles

**Files:**
- Modify: `firmware/zephyr/src/main.c:152` (remove unconditional `sq_ble_smoke_start()` boot advertise) and `firmware/zephyr/src/ble_smoke.c:162-188` (split radio-enable from advertise-start)

- [ ] **Step 1: Write the failing ztest** (in the new test dir): after boot with empty table, advertising is stopped; after `ble_start`, advertising is active; after `ble_stop` clears the last profile, advertising stops. Assert via a seam on the advertising state (mirror `sq_ble_smoke_real_api` install used in existing ble_smoke tests).
- [ ] **Step 2: Run** → FAIL (boot advertises unconditionally).
- [ ] **Step 3:** Keep `bt_enable` at boot (radio stack up) but do **not** auto `begin_advertising`. Add `sq_ble_advertising_sync()` that starts advertising iff `sq_ble_profile_table_count() > 0` and stops otherwise; call it at the end of `ble_start`/`ble_stop`. Update `main.c` to enable BT without advertising.
- [ ] **Step 4: Run** → PASS.

### Task 4.5: Firmware build + native ztest gate

- [ ] **Step 1: Run** `cargo run -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd` — compiles.
- [ ] **Step 2: Run** the protocol/ble ztests via `scripts/zephyr-test-protocol.sh` and the new + existing ble test dirs — all PASS.
- [ ] **Step 3: Commit**

```bash
git add firmware/zephyr docs/runtime_limits.md
git commit -m "feat(zephyr): imperative ble start/stop callbacks; gate advertising on active profiles"
```

---

## Slice 5 — Example, docs, hardware run

### Task 5.1: Example app

**Files:**
- Modify: `examples/ble-install/main.squid`

- [ ] **Step 1:** Replace the `app.triggers { service.ble.profile(...) }` block with an `app.start` handler calling `service.ble.start("object-transfer", { id: "sqbc-install", accept: [".sqbc"], events: { complete: "ble.object.complete", error: "ble.object.error" } })`. Keep the `ble.object.complete` handler (`app.install` + `app.launch`). Optionally add a teardown `service.ble.stop()`.
- [ ] **Step 2: Run** `cargo run -p squidc -- compile examples/ble-install/main.squid` (or the example's CLI test) — compiles clean.
- [ ] **Step 3: Commit** (with docs, Task 5.2).

### Task 5.2: Docs + roadmap/icebox

**Files:**
- Modify: `docs/language_spec.md:2390-2462`, `docs/runtime_limits.md:35-47`, `docs/hardware_target_tests.md`, `ROADMAP.md:31-54`, `ICEBOX.md`

- [ ] **Step 1:** Rewrite the language-spec BLE section as current-state: `service.ble.start(profile, config)` / `service.ble.stop()` imperative API, valid in handlers (not `app.triggers`); activation requires running the app once; persist-across-exit + app-decides-cleanup semantics; advertising gated on active profiles. Remove all `service.ble.profile` / `app.triggers` BLE references.
- [ ] **Step 2:** `runtime_limits.md` — rename macro to `SQ_VM_RUNTIME_BLE_PROFILE_MAX`, drop "armed" framing.
- [ ] **Step 3:** `hardware_target_tests.md` — update the BLE object-transfer test description to the imperative flow (launch app → start → push).
- [ ] **Step 4:** Per AGENTS.md drop discipline — **surface the ROADMAP/ICEBOX wording to the user and wait for confirmation** before committing. Remove the foreground-gated roadmap item; ICEBOX gets rationale / revival conditions / surviving parts only if anything is genuinely dropped (here it's superseded, not dropped — likely just delete the roadmap item).
- [ ] **Step 5:** `grep -rn "service.ble.profile\|BLE_PROFILE_ARMED\|app.triggers.*ble" docs examples` → zero hits.
- [ ] **Step 6: Run** `cargo test` + `cargo run -p squidc -- compile examples/ble-install/main.squid` — green. Commit.

```bash
git add examples/ble-install docs ROADMAP.md ICEBOX.md
git commit -m "docs(ble): imperative service.ble.start/stop spec, example, roadmap cleanup"
```

### Task 5.3: Hardware run (DoD #6) — XIAO on /dev/ttyACM0

This is the done criterion. Run sequentially, never in parallel; serial commands outside the sandbox.

- [ ] **Step 1:** Build + flash: `cargo run -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd` then `west flash -d build/zephyr/xiao-esp32c3-gdeq0426t82-sd`.
- [ ] **Step 2:** Install + launch the `ble-install` app over serial (`squidc app install` / `app launch`) so `app.start` runs `service.ble.start` (activation-requires-running-once).
- [ ] **Step 3:** Confirm advertising came up only after launch (monitor / `bluetoothctl`), then push a `.sqbc` payload **larger than the install scratch buffer** (≥2 KiB, not the 685-byte hello-menu) via `tools/ots-push` (bleak in the Zephyr venv per AGENTS.md).
- [ ] **Step 4: Verify DoD #6:** the pushed `.sqbc` installs byte-exact AND the installed app **launches with no `-5`** (foreground→foreground, not armed). Check `squidc app list` + device output for the launch and absence of `-5 (EIO)`.
- [ ] **Step 5:** Trigger `service.ble.stop` (or app exit) and confirm advertising stops.
- [ ] **Step 6:** Report the hardware result explicitly (pass/fail, payload size, errno observed). Do not mark the slice done on a clean-skip — the loop must complete on real hardware.

---

## Self-Review

**Spec coverage:**
- Proposed API `service.ble.start`/`stop` → Tasks 1.2, 2.3, 3.3, 4.2. ✓
- Remove `service.ble.profile` from `app.triggers` → Tasks 1.1, 1.5 (incl. `E_APP_TRIGGER_STATEMENT` rejection test). ✓
- Idempotent set/replace → Task 4.2 Step 3 (`remove_app` then `add`). ✓
- `stop` aborts in-flight + cleans staging → Task 4.2 Step 4 (`sq_ble_transfer_abort`). ✓
- Advertising gated on active profiles; no boot advertise → Task 4.4. ✓
- Routing/dispatch unchanged → Task 4.3 Step 1 (explicitly verify). ✓
- Cap macro rename → Task 4.1. ✓
- Re-launch idempotency / activation-requires-running-once → exercised by Task 5.3 Steps 2-3. ✓
- Dissolves `-5` → Task 5.3 Step 4 (the DoD). ✓
- Slices 1-5 layer mapping → matches spec §"Layers touched". ✓
- Verification list (compiler/VM/firmware/hardware) → Tasks 1.6, 2.3, 4.5, 5.3. ✓

**Type consistency:** `ServiceBleStart { profile, id, accept, events }` / `ServiceBleStop` used identically in ir.rs, parser, compile.rs, sqbc.rs, semantic.rs. `BUILTIN_SERVICE_BLE_START = 0xc1` / `_STOP = 0xc2` identical in `squidc-core/sqbc.rs` and `squidvm-core/bytecode.rs` (Task 2.1 Step 1 calls this out). Callback fields `ble_start`/`ble_stop` consistent across manifest, generated files, lib.rs bridge, firmware struct. `SECTION_BLE_PROFILES` (id 10) replaces `SECTION_BLE_TRIGGERS`.

**Open risks to watch during execution:**
- The next free builtin opcode (0xc1/0xc2) must be confirmed unused before claiming it (Task 1.4 Step 3 / 2.1).
- `collect_ble_profiles` traversal must cover every statement-bearing body (handlers, screens, functions) — reuse the string-table pass's traversal.
- ABI generator write-flag name must be confirmed from `--help` (Task 3.2).
- Firmware "running app id" source for `ble_start` must match what `register_app_ble_profile_triggers` used.
