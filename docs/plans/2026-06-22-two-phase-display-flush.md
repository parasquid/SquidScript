# Two-Phase Display Flush Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the FAST_1BPP composed differential partial display flush into two phases: a fast phase 1 that shows the cursor move in <800ms, and a deferred phase 2 that cleans up ghosting.

**Architecture:** Modify `sq_display_backend_flush()` to return a `needs_phase2` flag after phase 1 completes. The flush worker stores phase 2 state and schedules a delayed work item to run phase 2 when idle. Phase 2 is cancelable by new flushes.

**Tech Stack:** Zephyr RTOS, C, SPI display driver (SSD1677), k_delayed_work

---

## File Map

| File | Change |
|------|--------|
| `firmware/zephyr/src/vm_runtime_display_backend.h` | Add `bool *needs_phase2` parameter to `sq_display_backend_flush()` signature |
| `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c` | Implement phase 1/phase 2 split in composed fast1bpp path |
| `firmware/zephyr/src/vm_runtime.c` | Add phase 2 state, delayed work item, modify flush worker |
| `firmware/zephyr/src/vm_runtime_internal.h` | Declare phase 2 delayed work function (if needed) |

---

### Task 1: Update display backend header signature

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime_display_backend.h:6-8`

- [ ] **Step 1: Add `needs_phase2` parameter to header**

```c
int sq_display_backend_flush(const struct sq_vm_runtime_display_op *ops, size_t op_count,
			     enum sq_vm_runtime_display_refresh_mode refresh_mode,
			     const struct sq_vm_runtime_binbook_page *binbook_page,
			     bool *needs_phase2);
```

- [ ] **Step 2: Verify build compiles (will fail — caller not updated yet)**

Run: `cargo run -p squidc -- target build --target xteink-x4 2>&1 | head -20`
Expected: Compile error in `vm_runtime.c` — caller missing new parameter

- [ ] **Step 3: Commit**

```bash
git add firmware/zephyr/src/vm_runtime_display_backend.h
git commit -m "display: add needs_phase2 output param to flush signature"
```

---

### Task 2: Update flush worker caller to pass `needs_phase2`

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime.c:532-571` (flush worker)

- [ ] **Step 1: Add `needs_phase2` variable and pass to flush call**

In `runtime_display_flush_worker`, add a local `bool needs_phase2 = false;` and pass `&needs_phase2` as the new argument to `sq_display_backend_flush()`.

```c
static void runtime_display_flush_worker(void *arg1, void *arg2, void *arg3)
{
	ARG_UNUSED(arg1);
	ARG_UNUSED(arg2);
	ARG_UNUSED(arg3);

	while (true) {
		bool needs_phase2 = false;
		sq_debug_log_append("%lld:flush_start:ops=%d:mode=%d",
				    (long long)k_uptime_get(),
				    sq_vm_runtime_display_active_job.op_count,
				    (int)sq_vm_runtime_display_active_job.refresh_mode);
		uint64_t t0 = k_cycle_get_64();
		int result = sq_display_backend_flush(sq_vm_runtime_display_active_job.ops,
						      sq_vm_runtime_display_active_job.op_count,
						      sq_vm_runtime_display_active_job.refresh_mode,
						      sq_vm_runtime_display_active_job.binbook_page,
						      &needs_phase2);
		sq_vm_runtime_last_display_flush_us = k_cyc_to_us_floor64(k_cycle_get_64() - t0);
		sq_debug_log_append("%lld:flush_done:result=%d:us=%llu",
				    (long long)k_uptime_get(), result,
				    (unsigned long long)sq_vm_runtime_last_display_flush_us);
		runtime_display_record_flush_error(sq_vm_runtime_display_active_job.runtime, result);
		if (sq_vm_runtime_display_active_job.binbook_page != NULL) {
			k_free(sq_vm_runtime_display_active_job.binbook_page);
			sq_vm_runtime_display_active_job.binbook_page = NULL;
		}

		k_mutex_lock(&sq_vm_runtime_display_work_lock, K_FOREVER);
		if (sq_vm_runtime_display_pending) {
			sq_vm_runtime_display_active_job = sq_vm_runtime_display_pending_job;
			memset(&sq_vm_runtime_display_pending_job, 0,
			       sizeof(sq_vm_runtime_display_pending_job));
			sq_vm_runtime_display_pending = false;
			k_mutex_unlock(&sq_vm_runtime_display_work_lock);
			continue;
		}
		memset(&sq_vm_runtime_display_active_job, 0, sizeof(sq_vm_runtime_display_active_job));
		sq_vm_runtime_display_active = false;
		k_mutex_unlock(&sq_vm_runtime_display_work_lock);
		return;
	}
}
```

- [ ] **Step 2: Update stub implementation in `#else` block**

In the `#else` block (line 1543), update the stub to match the new signature:

```c
int sq_display_backend_flush(const struct sq_vm_runtime_display_op *ops, size_t op_count,
			     enum sq_vm_runtime_display_refresh_mode refresh_mode,
			     const struct sq_vm_runtime_binbook_page *binbook_page,
			     bool *needs_phase2)
{
	ARG_UNUSED(ops);
	ARG_UNUSED(op_count);
	ARG_UNUSED(refresh_mode);
	ARG_UNUSED(binbook_page);
	if (needs_phase2 != NULL) {
		*needs_phase2 = false;
	}
	return 0;
}
```

- [ ] **Step 3: Verify build compiles**

Run: `cargo run -p squidc -- target build --target xteink-x4`
Expected: Build succeeds (driver returns `needs_phase2=false` always, no phase 2 logic yet)

- [ ] **Step 4: Commit**

```bash
git add firmware/zephyr/src/vm_runtime.c
git commit -m "display: pass needs_phase2 to flush worker (no phase 2 logic yet)"
```

---

### Task 3: Implement phase 1 logic in display driver

**Files:**
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c:1444-1471`

- [ ] **Step 1: Modify composed fast1bpp path for phase 1**

Replace the differential partial streaming block (lines 1456-1463) with phase 1 logic:

```c
	if (use_composed_fast_path) {
		composed_refresh = sq_ssd1677_composed_refresh_decide(&composed_refresh_state,
								      refresh_request);
		LOG_INF("composed decide: refresh=%d request=%d prev_valid=%d prev_ops=%d ops=%d",
			(int)composed_refresh, (int)refresh_request,
			composed_refresh_state.previous_ops_valid,
			(int)previous_composed_op_count, (int)op_count);
		sq_debug_log_append("%lld:composed_decide:%d:ops=%d:prev=%d",
				    (long long)k_uptime_get(),
				    (int)composed_refresh,
				    (int)op_count,
				    (int)composed_refresh_state.previous_ops_valid);
		if (composed_refresh == SQ_SSD1677_COMPOSED_REFRESH_BW_DIFFERENTIAL_PARTIAL) {
			/* Phase 1: stream current frame only to BW_RAM */
			ret = stream_composed_1bpp_frame(SSD1677_CMD_WRITE_RAM, ops,
							  op_count);
			if (ret == 0) {
				ret = refresh_partial_display(&observed_busy);
			}
			if (ret == 0 && previous_composed_op_count > 0 && needs_phase2 != NULL) {
				*needs_phase2 = true;
			}
			/* Save current ops as previous for phase 2 */
			composed_remember_previous_ops(ops, op_count);
		}
		if (composed_refresh == SQ_SSD1677_COMPOSED_REFRESH_FULL_SEED) {
			ret = set_full_window();
			if (ret == 0) {
				ret = stream_composed_1bpp_frame(SSD1677_CMD_WRITE_RAM, ops,
								 op_count);
			}
		}
		if (ret != 0) {
			LOG_ERR("display composed stream failed: %d", ret);
			return ret;
		}
	}
```

- [ ] **Step 2: Verify build compiles**

Run: `cargo run -p squidc -- target build --target xteink-x4`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add firmware/zephyr/src/ssd1677_gdeq0426t82_display.c
git commit -m "display: implement phase 1 of two-phase flush (current frame only)"
```

---

### Task 4: Add phase 2 state and delayed work to flush worker

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime.c:28-39` (state declarations)
- Modify: `firmware/zephyr/src/vm_runtime.c:532-571` (flush worker)

- [ ] **Step 1: Add phase 2 state declarations**

After the existing `sq_vm_runtime_display_pending` declaration (line 39), add:

```c
static bool sq_vm_runtime_display_phase2_pending;
static struct sq_vm_runtime_display_flush_job sq_vm_runtime_display_phase2_job;
static struct k_delayed_work sq_vm_runtime_display_phase2_work;
static bool sq_vm_runtime_display_phase2_work_initialized;
```

- [ ] **Step 2: Add phase 2 delayed work handler**

Before `runtime_display_flush_worker`, add:

```c
static void runtime_display_phase2_handler(struct k_work *work)
{
	ARG_UNUSED(work);
	if (!sq_vm_runtime_display_phase2_pending) {
		return;
	}
	sq_debug_log_append("%lld:phase2_start", (long long)k_uptime_get());
	runtime_flush_display_if_dirty(sq_vm_runtime_display_phase2_job.runtime);
}
```

- [ ] **Step 3: Modify flush worker to save phase 2 state**

In `runtime_display_flush_worker`, after the `needs_phase2` check and before the pending job check, add phase 2 state save:

```c
		if (needs_phase2 && result == 0) {
			sq_vm_runtime_display_phase2_job = sq_vm_runtime_display_active_job;
			/* Null out binbook_page — phase 2 doesn't need it */
			sq_vm_runtime_display_phase2_job.binbook_page = NULL;
			sq_vm_runtime_display_phase2_pending = true;
			if (!sq_vm_runtime_display_phase2_work_initialized) {
				k_delayed_work_init(&sq_vm_runtime_display_phase2_work,
						    runtime_display_phase2_handler);
				sq_vm_runtime_display_phase2_work_initialized = true;
			}
			k_delayed_work_submit(&sq_vm_runtime_display_phase2_work, K_MSEC(50));
		}
```

- [ ] **Step 4: Cancel phase 2 on new flush**

In `runtime_flush_display_if_dirty`, when a new flush arrives and phase 2 is pending, cancel it:

```c
	if (sq_vm_runtime_display_phase2_pending) {
		k_delayed_work_cancel(&sq_vm_runtime_display_phase2_work);
		sq_vm_runtime_display_phase2_pending = false;
	}
```

Add this at the start of `runtime_flush_display_if_dirty`, before the existing active/pending check.

- [ ] **Step 5: Verify build compiles**

Run: `cargo run -p squidc -- target build --target xteink-x4`
Expected: Build succeeds

- [ ] **Step 6: Commit**

```bash
git add firmware/zephyr/src/vm_runtime.c
git commit -m "display: add phase 2 state and delayed work to flush worker"
```

---

### Task 5: Implement phase 2 logic in display driver

**Files:**
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c:1444-1471`

- [ ] **Step 1: Add phase 2 detection and execution**

The phase 2 call comes through `sq_display_backend_flush()` with the same ops. We need to detect when this is a phase 2 call. Since phase 2 uses the saved previous ops (not the current ops), we need a way to distinguish phase 1 from phase 2.

**Approach:** Use the `previous_composed_op_count` state. After phase 1, `previous_composed_ops` contains the current frame. When phase 2 is called, the ops passed are the same as phase 1 (current frame), but `previous_composed_ops` already contains them. So phase 2 will detect `previous_composed_op_count > 0` and the ops match — but we need to avoid infinite recursion.

**Better approach:** Add a static `bool in_phase2` flag in the display driver:

```c
static bool in_phase2 = false;
```

In the composed fast1bpp path:

```c
	if (composed_refresh == SQ_SSD1677_COMPOSED_REFRESH_BW_DIFFERENTIAL_PARTIAL) {
		if (!in_phase2) {
			/* Phase 1: stream current frame only to BW_RAM */
			ret = stream_composed_1bpp_frame(SSD1677_CMD_WRITE_RAM, ops,
							  op_count);
			if (ret == 0) {
				ret = refresh_partial_display(&observed_busy);
			}
			if (ret == 0 && previous_composed_op_count > 0 && needs_phase2 != NULL) {
				*needs_phase2 = true;
			}
			composed_remember_previous_ops(ops, op_count);
		} else {
			/* Phase 2: stream previous + current for differential cleanup */
			ret = stream_composed_1bpp_frame(SSD1677_CMD_WRITE_RED_RAM,
							  previous_composed_ops,
							  previous_composed_op_count);
			if (ret == 0) {
				ret = stream_composed_1bpp_frame(SSD1677_CMD_WRITE_RAM, ops,
								  op_count);
			}
			if (ret == 0) {
				ret = refresh_partial_display(&observed_busy);
			}
			composed_remember_previous_ops(ops, op_count);
			in_phase2 = false;
		}
	}
```

- [ ] **Step 2: Set `in_phase2` flag before phase 2 call**

In `runtime_display_phase2_handler`:

```c
static void runtime_display_phase2_handler(struct k_work *work)
{
	ARG_UNUSED(work);
	if (!sq_vm_runtime_display_phase2_pending) {
		return;
	}
	sq_debug_log_append("%lld:phase2_start", (long long)k_uptime_get());
	sq_display_backend_set_phase2(true);
	runtime_flush_display_if_dirty(sq_vm_runtime_display_phase2_job.runtime);
	sq_display_backend_set_phase2(false);
}
```

Add `sq_display_backend_set_phase2()` to the display driver:

```c
void sq_display_backend_set_phase2(bool phase2)
{
	in_phase2 = phase2;
}
```

And to the header:

```c
void sq_display_backend_set_phase2(bool phase2);
```

- [ ] **Step 3: Verify build compiles**

Run: `cargo run -p squidc -- target build --target xteink-x4`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add firmware/zephyr/src/ssd1677_gdeq0426t82_display.c firmware/zephyr/src/vm_runtime_display_backend.h firmware/zephyr/src/vm_runtime.c
git commit -m "display: implement phase 2 differential cleanup"
```

---

### Task 6: Update debug instrumentation

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime.c:539-542` (flush_start log)

- [ ] **Step 1: Add phase2 flag to flush_start log**

```c
		sq_debug_log_append("%lld:flush_start:ops=%d:mode=%d:phase2=%d",
				    (long long)k_uptime_get(),
				    sq_vm_runtime_display_active_job.op_count,
				    (int)sq_vm_runtime_display_active_job.refresh_mode,
				    (int)sq_vm_runtime_display_phase2_pending);
```

- [ ] **Step 2: Verify build compiles**

Run: `cargo run -p squidc -- target build --target xteink-x4`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add firmware/zephyr/src/vm_runtime.c
git commit -m "display: add phase2 flag to flush_start debug log"
```

---

### Task 7: Flash and test on hardware

**Files:**
- None (hardware verification)

- [ ] **Step 1: Flash firmware**

Run: `cargo run -p squidc -- target flash --target xteink-x4`
Expected: Flash succeeds, device boots

- [ ] **Step 2: Launch grid-cursor app**

Run: `cargo run -p squidc -- app install --target xteink-x4 examples/grid-cursor && cargo run -p squidc -- app launch --target xteink-x4 grid-cursor`
Expected: App launches, display shows grid

- [ ] **Step 3: Send LEFT key and capture timing**

Run: `cargo run -p squidc -- device key LEFT && sleep 3 && cargo run -p squidc -- device debug-log`
Expected: Debug log shows:
- `flush_start:ops=32:mode=1:phase2=0` (phase 1)
- `flush_done:result=0:us=<phase1_time>` (should be <800ms)
- `phase2_start` (after ~50ms delay)
- `flush_start:ops=32:mode=1:phase2=1` (phase 2)
- `flush_done:result=0:us=<phase2_time>`

- [ ] **Step 4: Verify phase 1 timing**

Phase 1 flush_done us should be <800000 (800ms). Current baseline is ~1,900,000us.

- [ ] **Step 5: Test rapid key presses**

Run: `cargo run -p squidc -- device key LEFT && sleep 0.2 && cargo run -p squidc -- device key LEFT && sleep 0.2 && cargo run -p squidc -- device key LEFT && sleep 3 && cargo run -p squidc -- device debug-log`
Expected: Phase 2 is canceled by subsequent flushes. Only one phase 2 runs after the last key press.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "display: two-phase flush verified on hardware"
```

---

### Task 8: Run native ztests

**Files:**
- None (verification)

- [ ] **Step 1: Run native ztests**

Run: `cargo run -p squidc -- target test --target native_posix 2>&1 | tail -20`
Expected: Tests pass (111/144 or better, no regressions)

- [ ] **Step 2: Commit if test fixes needed**

```bash
git add -A
git commit -m "test: fix ztests for two-phase flush changes"
```
