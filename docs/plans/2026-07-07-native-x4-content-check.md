# Native X4 Content Check Reliability Implementation Plan

> **For agentic workers:** Execute this plan task-by-task. Before editing Rust,
> read and use `omo:debugging`, `omo:programming`,
> `superpowers:test-driven-development`, and
> `superpowers:verification-before-completion`. Do not dispatch subagents unless
> the user explicitly asks for parallel agent work.

**Goal:** Make native XTEINK X4 `device content-check` reliably read the stored
SD file and return its exact size and CRC32 after a cold boot or cache
invalidation, then restore device-side CRC verification to the native BLE
transfer hardware gate.

**Architecture:** Diagnose the cold SD path before selecting a fix. The current
copy-time CRC cache is allowed as a temporary observation aid only; remove it
before acceptance so `content-check` verifies bytes currently stored on SD.
Fix the lowest layer proven responsible: native protocol scheduling, the X4
file adapter, the shared `embedded-sd-storage` implementation, or host framing.

**Tech Stack:** Rust `no_std`, SquidVM native runtime, SQDP serial protocol,
ESP32-C3 native firmware, SPI-mode SD/FAT via `embedded-sd-storage`, Bash
hardware scripts, XTEINK X4 hardware.

---

## Context The Implementer Must Know

### Current branch and working trees

- Repository: `/var/home/tristan/Documents/parasquid/SquidScript`
- Current branch when this plan was written: `native-firmware-x4`
- SquidScript has existing modified and untracked files. They are user-owned
  work. Do not reset, restore, stash, amend, or broadly format them.
- The native X4 crate uses a path dependency at
  `../binbook/crates/embedded-sd-storage`. The sibling BinBook repository also
  has uncommitted changes in its storage implementation. Preserve those edits.
- Before editing, run `git status --short` in both repositories and record the
  exact baseline in `.current_agent_work`.
- Stage only explicit paths. Never use `git add .` or `git add -A`.

### Verified behavior and evidence

- Native BLE upload completed for an 8,982-byte generated BinBook.
- The receiver app printed:

  ```text
  output=ble done 8982 8982
  output=ble copy true null 8982
  ```

- The payload is:

  ```text
  target/hardware-tests/xteink-x4-transfer-ble/ble-transfer-smoke.binbook
  size=8982
  crc32=5290be40
  ```

- Earlier forced checks produced `invalid protocol response frame: BadMagic`.
  The retained artifact is
  `target/hardware-tests/xteink-x4-transfer-ble/content-check.out`.
- A later direct check in the warm runtime state completed immediately and
  returned CRC `5290be40`. This proves the failure is state-dependent.
- The warm success is not proof that the SD read path works. The current
  `BoundedNativeFileBackend` stores a single copy-time name/size/CRC tuple and
  returns it from `content_check` without reading SD while valid.

### Relevant implementation flow

1. `scripts/xteink-x4-test-ble-transfer.sh` uploads the payload over BLE.
2. `tests/hardware/xteink-x4/ble-transfer-regression/main.squid` handles
   `ble.transfer.complete` and calls `file.copy` from `tmp/<name>` to
   `books/<name>`.
3. `BoundedNativeFileBackend::copy_file` in
   `firmware/native/crates/squidscript-fw-core/src/native_runtime.rs` copies in
   512-byte chunks and currently computes/caches CRC metadata.
4. `Opcode::ContentCheck` in
   `firmware/native/crates/squidscript-fw-x4/src/main.rs` calls
   `NativeRuntime::check_content` synchronously from the cooperative serial
   loop.
5. `X4SdFileStorage` in
   `firmware/native/crates/squidscript-fw-x4/src/lib.rs` maps `books/` and
   `tmp/` paths into the sibling `embedded-sd-storage::SdStorage` API.
6. `SdStorage::read_at_in_dir` in
   `../binbook/crates/embedded-sd-storage/src/sd_filesystem.rs` reconstructs and
   caches FAT extents, then reads file data through the SD block device.
7. `SerialDevice::read_protocol_frame` in
   `compiler/rust/crates/squidc-cli/src/serial.rs` waits up to 60 seconds and
   scans the stream for an SQDP frame.

### Non-negotiable constraints

- Do not raise the host timeout to conceal the defect.
- Do not accept copy-time byte counts or cached CRC metadata as stored-file
  verification.
- Do not weaken or skip the native BLE `content-check` gate.
- Do not run hardware/serial commands concurrently. One command owns the X4
  serial port at a time.
- Use the native backend and the attached `xteink-x4` target. Super Mini or
  Zephyr evidence does not prove native X4 behavior.
- Never print `.env`, Wi-Fi credentials, USB serial IDs, MAC addresses, or
  other host identifiers.
- Durable, bounded instrumentation remains enabled in normal debug builds and
  is compiled out of explicit release builds. Remove only one-off diagnostics
  that expose payload data, alter behavior, or are not useful for future work.
- A hardware `ok` or protocol frame is transport evidence. For this storage
  task no webcam proof is required.

## Done Definition

The slice is complete only when all of the following are true:

- `content-check` performs a real stored-file read; the copy-time CRC cache is
  gone.
- Three cold checks of the 8,982-byte payload return size `8982` and CRC32
  `5290be40` in at most 10 seconds each.
- The checks produce no reset, watchdog output, `BadMagic`, protocol error,
  retained device error, or loss of subsequent lifecycle/resource responses.
- Small and boundary payloads still work.
- The native BLE transfer script again runs device-side `content-check` and
  passes end-to-end.
- Relevant host tests, native firmware build, flash, and hardware verification
  all pass.
- Only the minimal root-cause fix, its tests, the restored gate, and required
  current-state documentation remain in the diff.

## Task 1: Capture the Dirty Baseline and Reproduce the Cold Failure

**Files:**

- Update before commands: `.current_agent_work`
- Read: `docs/standards/firmware-tooling.md`
- Read: `docs/standards/verification-commands.md`
- Evidence directory: `/tmp/squidscript-content-check-<timestamp>/`

- [ ] Record both worktrees without changing them:

  ```bash
  git status --short
  git -C ../binbook status --short
  git diff -- firmware/native/crates/squidscript-fw-core/src/native_runtime.rs
  git -C ../binbook diff -- crates/embedded-sd-storage/src/sd_filesystem.rs
  ```

  Expected: existing changes are visible and preserved. Copy the command
  outputs to the evidence directory, redacting host identifiers only.

- [ ] Confirm the attached target and serial access using the local overlay.
  Do not paste the raw USB by-id value into chat or tracked files.

- [ ] Build and flash the current native firmware once. Flashing preserves the
  SD card and gives a cold runtime with an empty in-memory CRC cache:

  ```bash
  cargo run -p squidc -- target build --target xteink-x4 --backend native
  PATH=/var/home/tristan/codex-box/.cargo/bin:$PATH \
    cargo run -p squidc -- target flash --target xteink-x4 --backend native
  ```

  Expected: both commands exit zero. If the port differs from the local
  overlay, probe and use the actual port rather than hard-coding one.

- [ ] Run the first cold check with raw-response capture enabled:

  ```bash
  SQUID_SERIAL_DUMP_RESPONSE=1 \
    cargo run --quiet -p squidc -- device content-check \
      ble-transfer-smoke.binbook \
      --size 8982 \
      --crc32 5290be40 \
      --port "$PORT"
  ```

  Record wall-clock duration, exit status, and complete output. Do not rerun
  before collecting post-failure state because a second run may warm caches.

- [ ] Immediately run these commands sequentially and capture each result:

  ```bash
  cargo run --quiet -p squidc -- device lifecycle --port "$PORT"
  cargo run --quiet -p squidc -- device resources --port "$PORT"
  cargo run --quiet -p squidc -- device errors --port "$PORT"
  ```

- [ ] Classify the observed failure using exactly one of these evidence states:

  - `SLOW_COMPLETE`: correct response arrives, but after 10 seconds.
  - `NO_RESPONSE_ALIVE`: timeout occurs and subsequent commands work.
  - `RESET`: boot/reset/watchdog text is present or lifecycle state is lost.
  - `FRAMING`: a complete correct SQDP frame exists in captured bytes but the
    CLI rejects the surrounding stream.
  - `DATA_ERROR`: response arrives with wrong size or CRC.

  Do not infer the class from source. The captured runtime bytes decide it.

- [ ] Repeat the cold procedure for these existing fixtures, reflashing or
  otherwise resetting the firmware runtime before each first check while
  preserving SD contents:

  | File | Expected size | Expected CRC source |
  |---|---:|---|
  | `ble-small.binbook` | 26 | `c7138533` |
  | `ble-1024.binbook` | 1024 | `efb5af2e` |
  | `ble-transfer-smoke.binbook` | 8982 | `5290be40` |

  The evidence must show the smallest size that changes the behavior. Do not
  summarize timing from one sample.

## Task 2: Add Failing-First Host Coverage for the Uncached Contract

**Files:**

- Modify: `firmware/native/crates/squidscript-fw-core/tests/native_runtime.rs`
- Modify only if its layer is implicated:
  `../binbook/crates/embedded-sd-storage/tests/fat_image.rs`

- [ ] Add a core backend test that publishes an 8,982-byte deterministic byte
  sequence, invalidates all optional metadata/cache state, calls
  `content_check`, and asserts exact size and CRC.

- [ ] Extend the fake storage with read counters and assert that uncached
  `content_check` actually invokes `file_size` and `read_at` until all bytes are
  consumed. The test must fail while `content_check` can return the copy-time
  metadata without reading storage.

- [ ] Add a mutation test: copy a file, mutate the stored destination through
  the fake storage, then call `content_check`. Assert the CRC reflects the
  mutated stored bytes, not the earlier copy-time CRC. This is the regression
  that makes deleting the shortcut mandatory.

- [ ] Run the focused test before implementation:

  ```bash
  cargo test -p squidscript-fw-core --test native_runtime \
    bounded_native_file_backend_content_check
  ```

  Expected: at least the mutation/read-path assertion fails for the current
  cached implementation. Save the failing output under `/tmp`.

- [ ] If Task 1 classified the problem below the X4 adapter, add an
  `embedded-sd-storage` regression that creates a fragmented file of at least
  8,982 bytes, drops the active read mapping, reads it in 512-byte chunks, and
  asserts byte equality plus a bounded block-read count.

- [ ] Run the sibling test before its implementation fix:

  ```bash
  cargo test --manifest-path ../binbook/Cargo.toml \
    -p embedded-sd-storage cold_fragmented_read
  ```

  Expected: fail only when Task 1 evidence identifies the sibling storage
  layer. Do not add a speculative failing test to the sibling repository when
  the failure is entirely in protocol scheduling or framing.

## Task 3: Instrument the Exact Runtime Boundary That Failed

**Files:**

- Temporary diagnostic edits may touch:
  `firmware/native/crates/squidscript-fw-x4/src/main.rs`
- Temporary diagnostic edits may touch:
  `firmware/native/crates/squidscript-fw-x4/src/lib.rs`
- Temporary diagnostic edits may touch:
  `../binbook/crates/embedded-sd-storage/src/sd_filesystem.rs`
- Temporary raw request harness: `/tmp/squidscript-content-check-*/`

- [ ] Before adding instrumentation, list each temporary edit and its removal
  command in a debug journal under `/tmp`. Do not use `git checkout` to clean
  up because both repositories contain user changes.

- [ ] Add diagnostic-only stage markers at these boundaries:

  1. content-check request parsed;
  2. content path mapped;
  3. file size obtained;
  4. FAT extent discovery started/completed, including extent count;
  5. each bounded read started/completed, including offset and length;
  6. CRC loop completed, including total bytes;
  7. response encoded;
  8. response write started/completed.

- [ ] Keep instrumentation allocation-free and bounded. Do not print file
  contents or host identifiers. Do not leave unconditional logging in release
  firmware.

- [ ] Because diagnostic text can share the USB stream with SQDP, use a single
  raw serial owner that records all incoming bytes and separately identifies:

  - native diagnostic text;
  - ESP ROM/boot/reset text;
  - complete SQDP frames and their sequence/opcode/status.

  Do not run `device monitor` concurrently with a CLI command.

- [ ] Rebuild, flash, and run exactly one cold 8,982-byte check. Use the final
  completed marker to localize the failure. Record observed timestamps; do not
  describe an unobserved call as the cause.

- [ ] Confirm the suspected cause by toggling only one variable:

  - For a read/extent issue, run the same file with the suspect operation
    bypassed or bounded while leaving protocol code unchanged.
  - For scheduling starvation, replace only the full CRC loop with one bounded
    step per cooperative iteration.
  - For framing, feed the exact captured byte stream into a host decoder test.
  - For reset/watchdog, reduce only the suspected blocking section enough to
    show the reset disappears.

  Root cause is confirmed only if toggling the suspected cause toggles the
  failure.

## Task 4: Implement the Minimal Root-Cause Fix

Use the branch selected by Task 1 and confirmed by Task 3. Do not combine
branches without independent evidence.

### Branch A: SD/FAT extent or block-read defect

**Files:**

- Modify: `../binbook/crates/embedded-sd-storage/src/sd_filesystem.rs`
- Test: `../binbook/crates/embedded-sd-storage/tests/fat_image.rs`

- [ ] Fix the confirmed unbounded, repeated, or incorrect extent/block
  operation. Maintain the existing `SdStorage` public API and `no_std`
  behavior.
- [ ] Preserve the directory-aware `BOOKS`/`TMP` changes already present in
  the sibling working tree.
- [ ] Keep read memory bounded by the existing extent vector and caller-owned
  output buffer. Do not read the entire file into RAM.
- [ ] Make the failing fragmented cold-read test pass.

### Branch B: Native cooperative-loop starvation

**Files:**

- Modify: `firmware/native/crates/squidscript-fw-x4/src/main.rs`
- Modify: `firmware/native/crates/squidscript-fw-core/src/native_runtime.rs`
- Modify if required: `compiler/rust/crates/squidc-cli/src/serial.rs`

- [ ] Introduce a bounded native content-check state containing name, total
  size, next offset, CRC state, and terminal result.
- [ ] Process at most one 512-byte read per cooperative loop iteration so BLE,
  timers, display flushing, and serial polling continue to advance.
- [ ] If SQDP must expose progress, reuse `Status::Pending` and have the CLI
  poll the same logical operation until `Ok` or `Error`. Do not add a new
  public command or a compatibility/version field.
- [ ] Reject a second content-check while one is active with the existing
  ordinary protocol error path; do not silently replace in-flight state.
- [ ] Clear in-flight state on reset, storage format, terminal error, and
  successful completion.

### Branch C: Serial framing defect

**Files:**

- Modify: `compiler/rust/crates/squidc-cli/src/serial.rs`
- Test: the nearest existing `squidc-cli` serial/protocol test module

- [ ] Add the exact captured stream as a test fixture or byte literal, with
  sensitive identifiers removed.
- [ ] Make frame extraction ignore non-frame prefixes and incomplete unrelated
  bytes while still requiring the matching response opcode and sequence.
- [ ] Do not interpret boot/reset text as a successful response. A reset must
  remain an explicit error even if a stale frame appears elsewhere in the
  stream.
- [ ] Keep the default 60-second timeout unchanged.

### Branch D: Data corruption

**Files:**

- Modify only the layer where Task 3 first observes bytes diverging.
- Add a failing fixture at that layer before the fix.

- [ ] Compare source, tmp upload, copied destination, and content-check data by
  offset and first divergent block.
- [ ] Fix the first writer/reader boundary that changes bytes. Do not patch the
  final CRC or expected value.
- [ ] Prove the copied destination equals the host payload before proceeding.

## Task 5: Remove the CRC Cache Correctness Shortcut

**Files:**

- Modify: `firmware/native/crates/squidscript-fw-core/src/native_runtime.rs`
- Modify: `firmware/native/crates/squidscript-fw-core/tests/native_runtime.rs`

- [ ] Delete these fields and their associated methods from
  `BoundedNativeFileBackend`:

  ```text
  cached_content_name
  cached_content_name_len
  cached_content_size
  cached_content_crc32
  cached_content_valid
  cache_content_check
  cached_content_check
  invalidate_cached_content
  ```

- [ ] Remove copy-time CRC accumulation if it has no remaining user-visible
  purpose. `file.copy` must still report `bytesWritten` and preserve its
  existing result shape.
- [ ] Ensure `content_check` always obtains current size and CRC from storage.
- [ ] Keep the mutation/read-path regression from Task 2. Delete or rewrite the
  current test named
  `bounded_native_file_backend_reuses_copy_metadata_for_content_check`; a test
  protecting the masking behavior is not a durable contract.
- [ ] Run focused tests and confirm the previous red test is green.

## Task 6: Restore the Native BLE Device-Side Verification Gate

**Files:**

- Modify: `scripts/xteink-x4-test-ble-transfer.sh`
- Test: `scripts/tests/test_zephyr_hardware_suite.py` or the nearest script
  contract test that owns this wrapper

- [ ] Remove the conditional that skips `device content-check` when
  `TARGET_BACKEND=native`.
- [ ] Keep both app-level assertions:

  ```text
  ble done <size> <size>
  ble copy true null <size>
  ```

- [ ] Require the subsequent device check to match the host-derived size and
  CRC for every backend.
- [ ] Keep command timeout plumbing unchanged. Do not add a native-only larger
  timeout.
- [ ] Add/update the script contract test so it fails if native mode can skip
  `content-check` again.
- [ ] Validate shell syntax:

  ```bash
  bash -n scripts/xteink-x4-test-ble-transfer.sh
  ```

  Expected: exit zero.

## Task 7: Retain Durable Instrumentation and Run Host Verification

**Files:**

- Every file changed in Tasks 2-6
- Related docs: `docs/squidc_cli.md` only if protocol behavior changed

- [ ] Keep bounded, allocation-free diagnostics that remain useful for future
  investigations and gate them on the normal debug-build profile. Remove only
  raw or one-off helpers that expose payload data or alter behavior.
- [ ] If cooperative `Pending` behavior was added, document it as internal
  command behavior without changing the public invocation.
- [ ] Run formatting only on changed Rust files using the repository toolchain.
  Keep formatter-produced changes in touched files; do not format unrelated
  user work.
- [ ] Run the focused host suites:

  ```bash
  cargo test -p squidscript-fw-core --test native_runtime
  cargo test -p squidscript-fw-x4
  cargo test -p squidc-cli
  python3 -m unittest scripts.tests.test_zephyr_hardware_suite
  ```

- [ ] If the sibling storage crate changed, also run:

  ```bash
  cargo test --manifest-path ../binbook/Cargo.toml -p embedded-sd-storage
  ```

- [ ] Run the native X4 build:

  ```bash
  cargo run -p squidc -- target build --target xteink-x4 --backend native
  ```

  Expected: all commands exit zero. Report any pre-existing failure separately;
  do not weaken tests to obtain green output.

## Task 8: Hardware Acceptance on the Attached XTEINK X4

**Evidence directory:**

- `/tmp/squidscript-content-check-final-<timestamp>/`

- [ ] Flash the verified native build once using the documented native target
  command and wait for USB re-enumeration.
- [ ] For each cold trial, reboot/reflash without formatting SD, then run the
  8,982-byte check exactly once before any operation that can warm file caches:

  ```bash
  cargo run --quiet -p squidc -- device content-check \
    ble-transfer-smoke.binbook \
    --size 8982 \
    --crc32 5290be40 \
    --port "$PORT"
  ```

- [ ] Complete three cold trials. Each must finish within 10 seconds and print:

  ```text
  content-check ble-transfer-smoke.binbook size=8982 crc32=5290be40
  ```

- [ ] After every trial, sequentially run lifecycle, resources, and errors.
  Require a responsive device, no reset evidence, and empty retained errors.
- [ ] Run cold checks for the 26-byte fixture with CRC `c7138533` and the
  1,024-byte fixture with CRC `efb5af2e`.
- [ ] Run the full restored BLE transfer gate:

  ```bash
  scripts/xteink-x4-test-ble-transfer.sh \
    --target xteink-x4 \
    --backend native \
    --port "$PORT" \
    --skip-flash
  ```

  Expected final line:

  ```text
  OK XTEINK X4 BLE transfer size=8982 crc32=5290be40
  ```

- [ ] Inspect the script's captured `content-check.out` and `errors.out`
  directly. A wrapper exit code alone is insufficient evidence.

## Task 9: Final Diff and Handoff

- [ ] Re-read this plan and map every Done Definition item to command evidence.
- [ ] Inspect both repository diffs. Confirm durable instrumentation is
  debug-profile gated, one-off instrumentation is gone, and unrelated user
  changes are untouched:

  ```bash
  git diff --check
  git status --short
  git -C ../binbook diff --check
  git -C ../binbook status --short
  ```

- [ ] Update `.current_agent_work` with the confirmed root cause, exact fix,
  test results, hardware results, remaining blockers, and the next parity task.
  Keep it current-state only.
- [ ] Update the active native X4 parity plan status without turning it into an
  investigation diary.
- [ ] Commit only after all required verification passes. If the fix changes
  the sibling BinBook crate, keep its commit separate from the SquidScript
  integration commit and record the required sibling revision/worktree state.

## Must Not Have

- A larger serial timeout presented as the fix.
- A CRC result served solely from copy-time metadata.
- A native-only skip in the BLE transfer script.
- Full-file buffering added to `content-check`.
- Concurrent serial, BLE, flash, monitor, or diagnostic commands.
- Tests that assert old cache field names or source layout instead of behavior.
- Debug markers, `/tmp` paths, raw USB identifiers, or environment secrets in
  committed code/docs.
- Cleanup operations that hide or destroy existing worktree changes.
