# SQBC Read Latency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce XTEINK X4 foreground dispatch latency below 100 ms by reusing one LittleFS SQBC file handle instead of reopening the file for every logical VM read.

**Architecture:** `vm_fs_storage` lazily opens one module-owned SQBC file handle and assigns it to a storage session, preserving the existing app-storage structure budget. It seeks for each caller-owned read and exposes an idempotent release function. App-store and protocol lifecycle owners release storage only while the VM is idle and before paths or files are replaced. If hardware remains above 100 ms, stop after recording evidence and create the separate Stage 2 small-app cache plan described by the approved design.

**Tech Stack:** Zephyr C17, Zephyr filesystem API, ztest/Twister native simulation, Rust `squidc` host CLI, XTEINK X4 ESP32-C3 hardware.

---

### Task 1: Reusable filesystem handle

**Files:**
- Modify: `firmware/zephyr/src/vm_fs_storage.h`
- Modify: `firmware/zephyr/src/vm_fs_storage.c`
- Test: `firmware/zephyr/tests/protocol/src/main.c`

- [x] **Step 1: Add a failing real-filesystem reuse test**

Extend `test_vm_fs_storage_reads_sqbc_and_persists_state` with a second SQBC
request and these assertions after both reads:

```c
zassert_equal(sq_vm_fs_storage_open_count(), sqbc_open_count + 1,
	      "one storage object must reuse one SQBC file handle");
zassert_true(sq_vm_fs_storage_is_open(&storage));
```

Then call the not-yet-implemented release API, overwrite the fixture, and read
again:

```c
zassert_equal(sq_vm_fs_storage_release(&storage), 0);
zassert_false(sq_vm_fs_storage_is_open(&storage));
zassert_equal(write_test_file(sqbc_path, replacement_sqbc,
			      sizeof(replacement_sqbc)), 0);
zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
zassert_mem_equal(completion.bytes, &replacement_sqbc[2], 3);
zassert_equal(sq_vm_fs_storage_open_count(), sqbc_open_count + 2);
```

- [x] **Step 2: Run the protocol ztests and verify RED**

Run:

```sh
scripts/zephyr-test-protocol.sh
```

Expected: compilation fails because the release/open diagnostic APIs do not
exist. The fresh pre-change native baseline is 144/144 passing tests.

- [x] **Step 3: Add handle state and the release API**

Keep the module-owned handle and diagnostics outside each storage object:

```c
static struct {
	struct fs_file_t file;
	struct sq_vm_fs_storage *owner;
	size_t owner_session_id;
	size_t open_count;
	size_t max_read_len;
} sqbc_open_file;
```

Include the Zephyr filesystem header and declare:

```c
int sq_vm_fs_storage_release(struct sq_vm_fs_storage *storage);
```

Reuse the existing maximum-read field as a storage session identifier so the
resident app-storage structure remains within its 168-byte budget. Release is
idempotent and closes only when both owner pointer and session identifier match.

```c
int sq_vm_fs_storage_release(struct sq_vm_fs_storage *storage)
{
	if (storage == NULL) {
		return -EINVAL;
	}
	if (!sq_vm_fs_storage_is_open(storage)) {
		return 0;
	}
	return release_open_sqbc_file();
}
```

- [x] **Step 4: Replace per-read open/close with lazy reuse**

In `fs_storage_read_sqbc`, initialize and open the module handle only when the
owner/session changes, increment the module open counter after a successful
open, then seek and read through that handle. On seek, read, or short-read
failure, call `sq_vm_fs_storage_release` but return the original operation
error. Keep per-storage read count and total length as logical successful-read
counters; keep maximum read length in module diagnostics.

- [x] **Step 5: Run the protocol ztests and verify GREEN for the new test**

Run `scripts/zephyr-test-protocol.sh`.

Expected: the extended filesystem test and fixed-buffer budget pass; all
144/144 native protocol tests pass.

- [x] **Step 6: Add and verify the read-error recovery test**

Add a request beyond the fixture end and assert `-EIO`, a closed handle, and a
successful later valid read with `sqbc_open_count` incremented. Run
`scripts/zephyr-test-protocol.sh` and require 144/144 passing tests.

- [x] **Step 7: Commit Task 1**

Stage only the storage implementation and protocol test:

```sh
git add firmware/zephyr/src/vm_fs_storage.h \
  firmware/zephyr/src/vm_fs_storage.c \
  firmware/zephyr/tests/protocol/src/main.c
git commit -m "perf(firmware): reuse SQBC filesystem handles"
```

### Task 2: Lifecycle invalidation

**Files:**
- Modify: `firmware/zephyr/src/app_store.h`
- Modify: `firmware/zephyr/src/app_store.c`
- Modify: `firmware/zephyr/src/device_protocol.c`
- Test: `firmware/zephyr/tests/protocol/src/main.c`

- [x] **Step 1: Add failing lifecycle tests**

Extend the app-store storage-path test to open the first app's SQBC, call
`sq_app_store_vm_storage_for_app` for a second app using the same zero-initialized
storage object, and assert the old handle is closed before the new path is
installed. Extend `test_storage_format_clears_runtime_before_erasing_files` so
an open `launch_storage.fs_storage` handle exists before format and is closed
after the runtime-clear phase.

- [x] **Step 2: Run the protocol ztests and verify RED**

Run `scripts/zephyr-test-protocol.sh`.

Expected: at least one new lifecycle assertion fails because current path
replacement uses `memset` without releasing the retained file.

- [x] **Step 3: Release app-store storage before path replacement**

At the start of `sq_app_store_vm_storage_for_app` and
`sq_app_store_vm_storage_for_app_bytes`, call
`sq_vm_fs_storage_release(&storage->fs_storage)` before `memset`. Propagate a
close error instead of replacing the path after a failed close. Document in
`app_store.h` that caller-provided storage must be zero-initialized before its
first setup.

- [x] **Step 4: Centralize foreground lifecycle release**

Add a `release_foreground_storage` helper in `device_protocol.c` that releases
both `context->launch_storage->fs_storage` and
`temp_foreground_storage.fs_storage`, returning the first error. Call it only
after `sq_vm_runtime_wait_idle` succeeds and before:

- clearing runtime context or formatting storage;
- replacing installed SQBC in direct or handler-deferred install paths;
- switching between installed, temp, or fallback foreground backends;
- clearing or replacing temp-run staging storage.

Do not close storage for ordinary key, timer, or refresh dispatches that reuse
the active `runtime->job_backend`.

```c
static int release_foreground_storage(const struct sq_device_protocol_context *context)
{
	int result = 0;

	if (context != NULL && context->launch_storage != NULL) {
		result = sq_vm_fs_storage_release(&context->launch_storage->fs_storage);
	}
	int temp_result =
		sq_vm_fs_storage_release(&temp_foreground_storage.fs_storage);
	return result != 0 ? result : temp_result;
}
```

- [x] **Step 5: Run the protocol ztests and verify GREEN**

Run `scripts/zephyr-test-protocol.sh`.

Expected: new lifecycle tests pass and all 144/144 protocol tests pass.

- [x] **Step 6: Commit Task 2**

```sh
git add firmware/zephyr/src/app_store.h \
  firmware/zephyr/src/app_store.c \
  firmware/zephyr/src/device_protocol.c \
  firmware/zephyr/tests/protocol/src/main.c
git commit -m "fix(firmware): invalidate SQBC handles at lifecycle boundaries"
```

### Task 3: Documentation and target verification

**Files:**
- Modify: `docs/firmware_app_storage.md`
- Modify: `docs/plans/2026-06-20-sqbc-read-latency.md`
- Existing uncommitted change: `firmware/zephyr/src/vm_runtime.c`
- Existing user-requested change: `ROADMAP.md`

- [ ] **Step 1: Update storage documentation**

Describe SQBC reads as file-backed, caller-buffered operations that reuse one
open filesystem handle until an explicit idle lifecycle boundary releases it.
Document that large apps remain streamed and no app-sized RAM buffer is
reserved. Remove any text that says each logical read opens and closes a file.

- [ ] **Step 2: Run final native verification**

Run `scripts/zephyr-test-protocol.sh` and compare every failure name with the
recorded inherited baseline. Run `git diff --check` for all modified files.

- [ ] **Step 3: Build and flash XTEINK X4 sequentially**

```sh
cargo run -p squidc -- target build --target xteink-x4
cargo run -p squidc -- target flash --target xteink-x4
```

Require successful build and flash. Record linker DRAM; do not weaken budgets
to make the build fit.

- [ ] **Step 4: Measure repeated valid grid-cursor transitions**

Install and launch the canonical `examples/grid-cursor` package. Drive at
least three valid cursor transitions sequentially. After each transition read
`device resources` and record `last_dispatch_us`, `last_sqbc_reads`,
`last_sqbc_bytes`, heap usage, protocol stack high-water, and VM stack
high-water. Also require an empty `device errors` response.

- [ ] **Step 5: Apply the acceptance decision**

If every repeated valid transition is below 100 ms, Stage 1 is complete and
Stage 2 is unnecessary. If any stable transition remains at or above 100 ms,
stop after preserving the measurements and create the separate bounded-cache
plan; do not add an unplanned heap allocation.

- [ ] **Step 6: Clean device and scratch state**

Leave canonical `grid-cursor` launched, uninstall `grid-cursor-nosave` if it
remains installed, and delete `/tmp/opencode/grid-cursor-nosave/` plus its
package. Do not format unrelated app storage.

- [ ] **Step 7: Commit documentation and the verified budget slice**

Stage only `docs/firmware_app_storage.md`, `ROADMAP.md`, the plan checklist,
and `firmware/zephyr/src/vm_runtime.c`. Confirm `.gitignore` and `AGENTS.md`
remain outside the commit, then commit with a message describing the verified
latency result.
