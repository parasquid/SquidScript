# XTEINK X4 RAM Telemetry & Baseline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make both RAM-target conditions measurable and capture the X4 before-state so the compact display-op refactor (Plan 2) can be verified against a real baseline.

**Architecture:** Firmware emits `heap_largest_free_bytes` via a bounded per-heap `k_heap_alloc` binary-search probe and adds display-worker stack high-water metrics mirroring the existing VM work-stack accessors. The host codec, test mirror table, and shared RAM harness decode the new metrics. A static-buffer-report classifier fix and a new X4 RAM-workload script make before/after attribution clean. A `--stack-usage` build produces `.su` files for static stack estimation.

**Tech Stack:** Zephyr C17, ztest/Twister native_sim, Rust `squid-device-protocol` codec, Bash hardware harness, XTEINK X4 ESP32-C3 hardware.

**Design spec:** `docs/specs/2026-06-20-x4-ram-reduction-design.md`

---

### Task A: Heap largest-free-block metric

**Files:**
- Modify: `firmware/zephyr/tests/protocol/prj.conf`
- Modify: `firmware/zephyr/tests/protocol/src/main.c`
- Modify: `firmware/zephyr/src/device_protocol.c`

- [ ] **Step 1: Write the failing test**

Enable heap runtime stats and a system heap in the test config. Add to
`firmware/zephyr/tests/protocol/prj.conf`:

```conf
CONFIG_SYS_HEAP_RUNTIME_STATS=y
CONFIG_HEAP_MEM_POOL_SIZE=8192
```

In `firmware/zephyr/tests/protocol/src/main.c`, the resources test declares
`heap_largest_free_supported` and `heap_largest_free_bytes` near line 4074. Add a
`heap_free_bytes` variable and replace the `== 0` assertions at lines 4129-4134
with:

```c
	zassert_true(resource_value_for_key(&frame, "heap_free_bytes",
					    &heap_free_bytes));
	zassert_true(resource_value_for_key(&frame, "heap_largest_free_supported",
					    &heap_largest_free_supported));
	zassert_equal(heap_largest_free_supported, 1);
	zassert_true(resource_value_for_key(&frame, "heap_largest_free_bytes",
					    &heap_largest_free_bytes));
	zassert_true(heap_largest_free_bytes > 0,
		     "largest free block must be positive with a live heap");
	zassert_true(heap_largest_free_bytes <= heap_free_bytes,
		     "largest free block must not exceed total free bytes");
```

In `test_resources_request_accepts_heap_max_reset_option` (line 4243), replace
the `== 0` assertion with:

```c
	zassert_true(resource_value_equals(&frame, "heap_largest_free_supported", 1));
```

- [ ] **Step 2: Run the protocol ztests and verify RED**

Run:

```sh
scripts/zephyr-test-protocol.sh
```

Expected: the resources test and the reset-option test fail because
`heap_largest_free_supported` is still 0 and `heap_largest_free_bytes` is still 0
(the firmware computes neither). Other tests stay green.

- [ ] **Step 3: Implement the bounded per-heap probe**

In `firmware/zephyr/src/device_protocol.c`, add a static helper above
`resources_response` (before line 2571):

```c
static size_t heap_largest_free_block_probe(struct k_heap *heap, size_t free_bytes_cap)
{
	if (heap == NULL || free_bytes_cap == 0) {
		return 0;
	}
	size_t lo = 0;
	size_t hi = free_bytes_cap;

	for (int i = 0; i < 32 && lo < hi; i++) {
		size_t mid = lo + (hi - lo + 1) / 2;
		void *block = k_heap_alloc(heap, mid, K_NO_WAIT);

		if (block != NULL) {
			k_heap_free(heap, block);
			lo = mid;
		} else {
			hi = mid - 1;
		}
	}
	return lo;
}
```

In the heap stats loop inside `resources_response` (around line 2640-2657), set
`heap_largest_free_supported` and probe each heap. Replace the existing
`#ifdef CONFIG_SYS_HEAP_RUNTIME_STATS` block with:

```c
#ifdef CONFIG_SYS_HEAP_RUNTIME_STATS
	struct k_heap *heaps = NULL;
	int heap_array_count = k_heap_array_get(&heaps);
	if (heap_array_count > 0 && heaps != NULL) {
		heap_count = (size_t)heap_array_count;
		heap_largest_free_supported = 1u;
		for (int i = 0; i < heap_array_count; i++) {
			struct sys_memory_stats stats;

			if (reset_heap_max) {
				(void)sys_heap_runtime_stats_reset_max(&heaps[i].heap);
			}
			if (sys_heap_runtime_stats_get(&heaps[i].heap, &stats) == 0) {
				heap_free_bytes += stats.free_bytes;
				heap_allocated_bytes += stats.allocated_bytes;
				heap_max_allocated_bytes += stats.max_allocated_bytes;
				size_t largest =
					heap_largest_free_block_probe(&heaps[i], stats.free_bytes);

				if (largest > heap_largest_free_bytes) {
					heap_largest_free_bytes = largest;
				}
			}
		}
	}
#endif
```

`heap_max_allocated_bytes` is captured via `sys_heap_runtime_stats_get` before
the probe runs for each heap, so the reported max is unaffected by the probe's
transient allocation.

- [ ] **Step 4: Run the protocol ztests and verify GREEN**

Run `scripts/zephyr-test-protocol.sh`.

Expected: the resources test and reset-option test pass; all 144/144 protocol
tests pass.

- [ ] **Step 5: Commit Task A**

```sh
git add firmware/zephyr/tests/protocol/prj.conf \
  firmware/zephyr/tests/protocol/src/main.c \
  firmware/zephyr/src/device_protocol.c
git commit -m "feat(diagnostics): report heap largest free block"
```

### Task B: Display-worker stack high-water metric

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime.c`
- Modify: `firmware/zephyr/src/vm_runtime.h`
- Modify: `firmware/zephyr/src/device_protocol.c`
- Modify: `firmware/zephyr/tests/protocol/src/main.c`
- Modify: `compiler/rust/crates/squid-device-protocol/src/lib.rs`

- [ ] **Step 1: Write the failing test**

In `firmware/zephyr/tests/protocol/src/main.c`, extend the metric name table
(around line 907) with three entries after `x4.input.power_error`:

```c
		{53, "display_stack_size_bytes"},
		{54, "display_stack_unused_bytes"},
		{55, "display_stack_used_bytes"},
```

In the resources test body (after the `vm_stack_used_bytes` assertions near line
4123), add:

```c
	zassert_true(resource_value_equals(&frame, "display_stack_size_bytes",
					   SQ_VM_RUNTIME_DISPLAY_WORK_STACK_SIZE));
	uint64_t display_stack_unused = 0;
	uint64_t display_stack_used = 0;

	zassert_true(resource_value_for_key(&frame, "display_stack_unused_bytes",
					    &display_stack_unused));
	zassert_true(resource_value_for_key(&frame, "display_stack_used_bytes",
					    &display_stack_used));
	zassert_equal(display_stack_unused + display_stack_used,
		      SQ_VM_RUNTIME_DISPLAY_WORK_STACK_SIZE);
```

In `compiler/rust/crates/squid-device-protocol/src/lib.rs`, add to
`RESOURCE_METRIC_NAMES` after `(52, "x4.input.power_error")`:

```rust
    (53, "display_stack_size_bytes"),
    (54, "display_stack_unused_bytes"),
    (55, "display_stack_used_bytes"),
```

- [ ] **Step 2: Run focused tests and verify RED**

Run:

```sh
cargo test -p squid-device-protocol
scripts/zephyr-test-protocol.sh
```

Expected: the Rust metric-name test fails for the new IDs, and the protocol
resources test fails because the metrics are absent from the firmware response.

- [ ] **Step 3: Add the display-worker stack accessors**

In `firmware/zephyr/src/vm_runtime.c`, after
`sq_vm_runtime_work_stack_unused` (line 684), add:

```c
size_t sq_vm_runtime_display_work_stack_size(void)
{
	return K_THREAD_STACK_SIZEOF(sq_vm_runtime_display_work_stack);
}

int sq_vm_runtime_display_work_stack_unused(size_t *unused)
{
	if (unused == NULL) {
		return -EINVAL;
	}

#if defined(CONFIG_INIT_STACKS) && defined(CONFIG_THREAD_STACK_INFO)
	if (!sq_vm_runtime_display_work_thread_started) {
		*unused = sq_vm_runtime_display_work_stack_size();
		return 0;
	}
	return k_thread_stack_space_get(&sq_vm_runtime_display_work_thread, unused);
#else
	*unused = 0;
	return -ENOTSUP;
#endif
}
```

In `firmware/zephyr/src/vm_runtime.h`, after
`int sq_vm_runtime_work_stack_unused(size_t *unused);` (line 594), add:

```c
size_t sq_vm_runtime_display_work_stack_size(void);
int sq_vm_runtime_display_work_stack_unused(size_t *unused);
```

- [ ] **Step 4: Emit the display-worker stack metrics**

In `firmware/zephyr/src/device_protocol.c`, add three metric IDs to
`enum sq_resource_metric_id` after `SQ_RESOURCE_METRIC_X4_INPUT_POWER_ERROR`
(line 140):

```c
	SQ_RESOURCE_METRIC_DISPLAY_STACK_SIZE_BYTES = 53,
	SQ_RESOURCE_METRIC_DISPLAY_STACK_UNUSED_BYTES = 54,
	SQ_RESOURCE_METRIC_DISPLAY_STACK_USED_BYTES = 55,
```

In `resources_response`, add locals near the VM stack locals (after line 2581):

```c
	size_t display_work_stack_unused = 0;
	size_t display_work_stack_size = sq_vm_runtime_display_work_stack_size();
	size_t display_work_stack_used = 0;
```

After the VM stack measurement block (after line 2631), add:

```c
	if (sq_vm_runtime_display_work_stack_unused(&display_work_stack_unused) == 0 &&
	    display_work_stack_unused <= display_work_stack_size) {
		display_work_stack_used = display_work_stack_size - display_work_stack_unused;
	} else {
		display_work_stack_unused = 0;
		display_work_stack_used = 0;
	}
```

After the VM stack metric emissions (after line 2763), add:

```c
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_DISPLAY_STACK_SIZE_BYTES, display_work_stack_size);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_DISPLAY_STACK_UNUSED_BYTES, display_work_stack_unused);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_DISPLAY_STACK_USED_BYTES, display_work_stack_used);
```

- [ ] **Step 5: Run focused tests and verify GREEN**

Run:

```sh
cargo test -p squid-device-protocol
scripts/zephyr-test-protocol.sh
```

Expected: the Rust metric-name tests pass and all 144/144 protocol tests pass.

- [ ] **Step 6: Commit Task B**

```sh
git add firmware/zephyr/src/vm_runtime.c \
  firmware/zephyr/src/vm_runtime.h \
  firmware/zephyr/src/device_protocol.c \
  firmware/zephyr/tests/protocol/src/main.c \
  compiler/rust/crates/squid-device-protocol/src/lib.rs
git commit -m "feat(diagnostics): report display worker stack high-water"
```

### Task C: Static buffer report classifier fix

**Files:**
- Modify: `scripts/zephyr-static-buffer-report.sh`

- [ ] **Step 1: Extend the squidscript classifier**

In `scripts/zephyr-static-buffer-report.sh`, update the `classify` awk function
(lines 45-48) so display, BinBook, and HTTP symbols attribute to squidscript.
Replace the squidscript `if` branch with:

```awk
      if (name ~ /^(runtime|response|registry|install_session|temp_session|resource_session|protocol_scratch|launch_storage|trigger_storage|transport|sq_vm_runtime_work_stack|sq_vm_runtime_work_thread|sq_vm_runtime_display|sq_app_lfs_storage|sq_http_)(\.|$)/ ||
          name ~ /^(sq_ble_|binbook_)/ ||
          name ~ /^previous_composed_ops$/) {
        return "squidscript"
      }
```

- [ ] **Step 2: Verify the classifier against the X4 ELF**

Run (escalated; the ELF exists from the prior Stage 1 build):

```sh
scripts/zephyr-static-buffer-report.sh build/zephyr/xteink-x4/zephyr/zephyr.elf
```

Expected: `sq_vm_runtime_display_active_job`,
`sq_vm_runtime_display_pending_job`, `previous_composed_ops`,
`sq_vm_runtime_display_work_stack`, and `sq_http_chunk_buffer` now appear in the
`squidscript` group, and the `unknown` group shrinks accordingly. Record the
group totals as the classifier-corrected baseline.

- [ ] **Step 3: Commit Task C**

```sh
git add scripts/zephyr-static-buffer-report.sh
git commit -m "fix(scripts): attribute display/binbook/http symbols in static buffer report"
```

### Task D: X4 RAM workload script

**Files:**
- Create: `scripts/xteink-x4-measure-ram-workloads.sh`
- Modify: `scripts/lib/ram-workload-harness.sh`

- [ ] **Step 1: Extend the shared harness with display-stack columns**

In `scripts/lib/ram-workload-harness.sh`, extend `ram_init_summary` (line 134) to
add `display_stack_used_bytes` and `display_stack_unused_bytes` columns to the
header, and extend `ram_snapshot_resources` (line 148) to assert display-stack
accounting and append those columns. Add a `display` stack accounting call in
`ram_snapshot_resources` after the `vm` accounting (after line 154):

```bash
	ram_assert_stack_accounting "$file" display
```

Add the two column values to the `printf` row (after the `vm_stack_unused_bytes`
column) and the two header names.

- [ ] **Step 2: Create the X4 workload script**

Create `scripts/xteink-x4-measure-ram-workloads.sh` modeled on
`scripts/xiao-esp32c3-measure-ram-workloads.sh`. It sources
`scripts/lib/serial-port.sh` and `scripts/lib/ram-workload-harness.sh`, flashes
`xteink-x4` once (unless `--skip-flash`), then runs these workloads serially,
each bracketed by `ram_reset_runtime_between_workloads` +
`ram_reset_heap_max_attribution` + `ram_snapshot_resources`:

1. `storage-format` baseline (`device reset` + `device storage-format`).
2. `grid-cursor`: package/install/launch `examples/grid-cursor`, drive
   `device key DOWN`, wait for `device output` cursor line.
3. `binbook-reader`: package/install/launch `examples/binbook-reader`, drive a
   page-turn key, wait for `device drawlog` `draw=binbook`.
4. `wifi-ap`: install/launch a Wi-Fi AP summary app, wait for
   `output=wifi start true null`, snapshot start; `device key SELECT`, wait for
   `output=wifi stop true null`, snapshot stop.
5. `ble-transfer`: install/launch the BLE file-transfer regression app, wait for
   `transfer ready`, `device ble-put` a payload, wait for `ble copy true null`.
6. `http-transfer`: install/launch the HTTP file-transfer regression app, wait
   for `transfer ready`, connect host to the device AP and `curl --upload-file`,
   wait for `http copy true null`.

Write `summary.tsv` under `target/hardware-tests/x4-ram-workloads/`. Run all
hardware commands sequentially (single USB serial device).

- [ ] **Step 3: Lint the script**

Run:

```sh
bash -n scripts/xteink-x4-measure-ram-workloads.sh
bash -n scripts/lib/ram-workload-harness.sh
```

Expected: no syntax errors.

- [ ] **Step 4: Commit Task D**

```sh
git add scripts/xteink-x4-measure-ram-workloads.sh scripts/lib/ram-workload-harness.sh
git commit -m "feat(scripts): add X4 RAM workload measurement script"
```

### Task E: Stack-usage build and baseline capture

**Files:**
- None (evidence capture; record results in `.current_agent_work`)

- [ ] **Step 1: Build X4 with stack-usage instrumentation (escalated)**

Run:

```sh
cargo run -p squidc -- target build --target xteink-x4 --stack-usage --pristine
```

Expected: successful build. Confirm `.su` files exist under
`build/zephyr/xteink-x4/CMakeFiles/app.dir/src/*.c.su`.

- [ ] **Step 2: Run the stack-usage report (escalated)**

Run:

```sh
ZEPHYR_BUILD_DIR=build/zephyr/xteink-x4 scripts/c3-supermini-stack-usage-report.sh
```

Expected: top per-function stack-usage rows and cumulative call-chain estimates
for the firmware C sources. Record the output.

- [ ] **Step 3: Run the X4 RAM workload script (escalated)**

Run:

```sh
scripts/xteink-x4-measure-ram-workloads.sh
```

Expected: `summary.tsv` with per-workload heap/stack high-water, including
non-zero `heap_largest_free_bytes` and display-stack accounting. Record the TSV
and the static buffer report in `.current_agent_work` as the pre-Plan-2 baseline.

- [ ] **Step 4: Record the baseline**

Append the captured linker DRAM, static buffer group totals, stack-usage top
rows, and the `summary.tsv` contents to `.current_agent_work` under a
"Plan 1 baseline captured" header.

### Task F: Finalize and push

- [ ] **Step 1: Run final native verification**

Run:

```sh
cargo test -p squid-device-protocol
scripts/zephyr-test-protocol.sh
```

Expected: `cargo test -p squid-device-protocol` green and 144/144 protocol tests
pass.

- [ ] **Step 2: Push (escalated)**

Confirm `.gitignore` and `AGENTS.md` remain outside the staged set, then push
`main` to `origin/main`.

- [ ] **Step 3: Update tracking**

Mark Plan 1 complete in `.current_agent_work` and note that Plan 2 (compact
display-op) is the next slice.
