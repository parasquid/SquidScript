# SQBC Small-App Cache Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring repeated XTEINK X4 grid-cursor dispatch below 100 ms by caching SQBC files up to 4 KiB while preserving file-backed fallback and existing storage-object RAM budgets.

**Architecture:** `vm_fs_storage` owns one optional system-heap cache alongside its module-owned file handle. A storage session attempts cache population once; exact small-file reads attach the cache, allocation failure and files above the named limit use the reusable file handle. Lifecycle release frees a matching cache, and typed resource metrics expose cache activity and byte size.

**Tech Stack:** Zephyr C17 system heap and filesystem APIs, ztest/Twister, Rust device-protocol metric codec, XTEINK X4 hardware.

---

### Task 1: Cache behavior and fallback

**Files:**
- Modify: `firmware/zephyr/tests/protocol/prj.conf`
- Modify: `firmware/zephyr/src/vm_fs_storage.h`
- Modify: `firmware/zephyr/src/vm_fs_storage.c`
- Test: `firmware/zephyr/tests/protocol/src/main.c`

- [ ] **Step 1: Add failing small/large cache tests**

Configure an 8 KiB test system heap. Add a 2 KiB file test that performs two
bounded reads, asserts one physical open, active cache, exact cache size, and
successful cached reading after the file is unlinked. Release must clear the
cache. Add a file larger than `SQ_VM_FS_STORAGE_CACHE_MAX_BYTES` and assert it
uses the reusable file handle with no active cache.

- [ ] **Step 2: Run `scripts/zephyr-test-protocol.sh` and verify RED**

Expected: compilation fails because the cache limit and diagnostic APIs do not
exist.

- [ ] **Step 3: Implement one-attempt cache population**

Add the named 4 KiB limit and module cache state containing bytes, length,
owner/session, and attempted-session identity. On a new storage session,
`fs_stat` the SQBC path. For an exact non-empty file within the limit, allocate
with `k_malloc`, read the complete file once, close the file handle, and serve
logical reads with bounds-checked `memcpy`. Free partial allocations on error.
Allocation failure and oversized files fall through to the reusable handle and
must not retry allocation for every logical read.

- [ ] **Step 4: Add allocation-failure fallback coverage**

Exhaust the configured test system heap with bounded 256-byte allocations,
read a cache-eligible SQBC file, and assert the read succeeds through the file
handle with cache inactive. Free every test allocation.

- [ ] **Step 5: Run tests and require 144/144 passing**

Run `scripts/zephyr-test-protocol.sh`; require all protocol tests and the
fixed-buffer budget to pass.

- [ ] **Step 6: Commit cache behavior**

```sh
git add firmware/zephyr/tests/protocol/prj.conf \
  firmware/zephyr/src/vm_fs_storage.h \
  firmware/zephyr/src/vm_fs_storage.c \
  firmware/zephyr/tests/protocol/src/main.c
git commit -m "perf(firmware): cache small SQBC apps"
```

### Task 2: Typed cache diagnostics

**Files:**
- Modify: `firmware/zephyr/src/device_protocol.c`
- Modify: `compiler/rust/crates/squid-device-protocol/src/lib.rs`
- Test: `firmware/zephyr/tests/protocol/src/main.c`
- Test: `compiler/rust/crates/squid-device-protocol/src/lib.rs`

- [ ] **Step 1: Add failing metric-name tests**

Add typed metric IDs 53 and 54 for `sqbc_cache_active` and
`sqbc_cache_bytes`. Extend firmware resource decoding and Rust metric-name
tests to require both names.

- [ ] **Step 2: Run focused Rust tests and protocol ztests to verify RED**

Run `cargo test -p squid-device-protocol` and
`scripts/zephyr-test-protocol.sh`. Expected: new names/values are absent.

- [ ] **Step 3: Emit and decode the cache metrics**

Emit `sq_vm_fs_storage_cache_active()` as 0/1 and
`sq_vm_fs_storage_cache_size()` as bytes in `device resources`; add the same ID
mapping in the Rust host codec.

- [ ] **Step 4: Run focused tests and require GREEN**

Require `cargo test -p squid-device-protocol` and all 144 protocol tests to
pass.

- [ ] **Step 5: Commit typed diagnostics**

```sh
git add firmware/zephyr/src/device_protocol.c \
  firmware/zephyr/tests/protocol/src/main.c \
  compiler/rust/crates/squid-device-protocol/src/lib.rs
git commit -m "feat(diagnostics): report SQBC cache usage"
```

### Task 3: Documentation and hardware acceptance

**Files:**
- Modify: `docs/firmware_app_storage.md`
- Modify: `docs/specs/2026-06-20-sqbc-read-latency-design.md`
- Modify: `docs/plans/2026-06-20-sqbc-small-app-cache.md`

- [ ] **Step 1: Document the 4 KiB cache and fallback**

Describe system-heap ownership, one-attempt allocation, lifecycle invalidation,
large/allocation-failure fallback, and typed cache metrics as current facts.

- [ ] **Step 2: Run final native verification**

Require `cargo test -p squid-device-protocol`, 144/144 protocol tests, and
`git diff --check`.

- [ ] **Step 3: Build, flash, and measure hardware sequentially**

Build and flash `xteink-x4`, reinstall canonical grid-cursor, and measure at
least three valid transitions. Require cache active with 3,858 cached bytes,
all transitions below 100 ms, empty device errors, and preserved stack
guardrails. Record linker DRAM and heap headroom.

- [ ] **Step 4: Clean and commit**

Leave canonical grid-cursor launched, remove the nosave test app/scratch, mark
both execution plans complete, and commit only task-owned docs plus the
previously verified budget/roadmap changes. Preserve unrelated `.gitignore`
and `AGENTS.md` edits.
