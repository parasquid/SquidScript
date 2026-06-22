# Two-Phase Display Flush Design

## Problem

Grid-cursor app has ~1.9s latency between key press and display update. The bottleneck is the composed differential partial refresh path, which streams 96KB (two full frames) over SPI and waits for the e-paper panel to complete a physical refresh.

## Goal

Split the display flush into two phases:
1. **Phase 1 (fast):** Stream only the current frame, do a quick partial refresh. User sees cursor move in <800ms.
2. **Phase 2 (cleanup):** Stream previous frame + current frame, do differential partial to eliminate ghosting. Deferred until idle, cancelable by new flush.

## Scope

Only applies to the FAST_1BPP composed differential partial path in `sq_display_backend_flush()`. Full refresh, AUTO mode, and binbook paths are unaffected.

## High-Level Flow

### Current Flow
```
dispatch → flush worker → sq_display_backend_flush()
  → stream previous frame (48KB) → RED_RAM
  → stream current frame (48KB) → BW_RAM
  → wait_ready (500-1200ms)
  → flush worker returns
total: ~1.9s, user sees result after full completion
```

### Proposed Flow
```
dispatch → flush worker → sq_display_backend_flush()
  → stream current frame (48KB) → BW_RAM
  → wait_ready (500-800ms)  ← PHASE 1 COMPLETE, user sees cursor move
  → return needs_phase2=true, save previous ops
  → flush worker stores phase 2 state, returns (display idle)

... next idle cycle (no new flush pending) ...

flush worker → sq_display_backend_flush()
  → stream previous frame (48KB) → RED_RAM
  → stream current frame (48KB) → BW_RAM
  → wait_ready (500-800ms)  ← PHASE 2 COMPLETE, ghosting cleaned up
  → flush worker returns
```

## Display Driver Changes

### `sq_display_backend_flush()` Signature

```c
int sq_display_backend_flush(const struct sq_vm_runtime_display_op *ops, size_t op_count,
                             enum sq_vm_runtime_display_refresh_mode refresh_request,
                             const struct sq_vm_runtime_binbook_page *binbook_page,
                             bool *needs_phase2);
```

New output parameter `needs_phase2` signals that a cleanup pass is needed.

### Phase 1 Logic (composed fast1bpp path)

```c
if (composed_refresh == SQ_SSD1677_COMPOSED_REFRESH_BW_DIFFERENTIAL_PARTIAL) {
    // Phase 1: stream current frame only to BW_RAM
    ret = stream_composed_1bpp_frame(SSD1677_CMD_WRITE_RAM, ops, op_count);
    if (ret == 0) {
        ret = refresh_partial_display(&observed_busy);  // BW-only partial
    }
    if (ret == 0 && previous_composed_op_count > 0) {
        *needs_phase2 = true;  // signal cleanup needed
    }
    // Save current ops as "previous" for phase 2
    composed_remember_previous_ops(ops, op_count);
}
```

### Phase 2 Logic

Phase 2 is a separate call to `sq_display_backend_flush()` that performs the full differential partial:

```c
if (composed_refresh == SQ_SSD1677_COMPOSED_REFRESH_BW_DIFFERENTIAL_PARTIAL) {
    // Stream previous frame → RED_RAM
    ret = stream_composed_1bpp_frame(SSD1677_CMD_WRITE_RED_RAM,
                                     previous_composed_ops,
                                     previous_composed_op_count);
    if (ret == 0) {
        // Stream current frame → BW_RAM
        ret = stream_composed_1bpp_frame(SSD1677_CMD_WRITE_RAM, ops, op_count);
    }
    if (ret == 0) {
        ret = refresh_partial_display(&observed_busy);  // differential cleanup
    }
}
```

## Flush Worker Changes

### New State

```c
static bool sq_vm_runtime_display_phase2_pending = false;
static struct sq_vm_runtime_display_op sq_vm_runtime_display_phase2_ops[
    SQ_VM_RUNTIME_DISPLAY_OPS_MAX];
static size_t sq_vm_runtime_display_phase2_op_count;
```

### Modified Flush Worker Loop

```c
static void runtime_display_flush_worker(void *arg1, void *arg2, void *arg3)
{
    while (true) {
        bool needs_phase2 = false;

        // ... existing flush_start logging ...
        int result = sq_display_backend_flush(active_job.ops, active_job.op_count,
                                              active_job.refresh_mode, active_job.binbook_page,
                                              &needs_phase2);
        // ... existing flush_done logging ...

        if (needs_phase2 && result == 0) {
            // Save phase 2 state
            memcpy(sq_vm_runtime_display_phase2_ops, active_job.ops,
                   sizeof(active_job.ops[0]) * active_job.op_count);
            sq_vm_runtime_display_phase2_op_count = active_job.op_count;
            sq_vm_runtime_display_phase2_pending = true;
            // Schedule phase 2 to run after a short delay
            k_delayed_work_submit(&sq_vm_runtime_display_phase2_work, K_MSEC(50));
        }

        // ... existing pending job check ...
        // Phase 2 is picked up like any other pending job
    }
}
```

### Phase 2 Delayed Work Item

```c
static void runtime_display_phase2_worker(struct k_work *work)
{
    ARG_UNUSED(work);
    if (!sq_vm_runtime_display_phase2_pending) {
        return;  // canceled by new flush
    }
    // Start flush worker for phase 2
    runtime_flush_display_if_dirty_phase2();
}
```

### Phase 2 Integration

Phase 2 uses a deferred work item to trigger after phase 1 completes:

1. Phase 1 completes → flush worker stores phase 2 state → returns (display idle)
2. Flush worker schedules a delayed work item (Zephyr `k_delayed_work`) to trigger phase 2 after a short delay (e.g., 50ms)
3. Delayed work item fires → flush worker starts for phase 2
4. If a new flush arrives before phase 2 starts, phase 2 is canceled (new flush takes priority, delayed work is cancelled)

**Why deferred work:** Without it, phase 2 would only run if the user presses another key. With deferred work, phase 2 runs automatically after phase 1, giving the user the cleanup benefit even without further interaction.

## Error Handling

### Phase 1 Failure

If phase 1 fails (SPI error, timeout):
- `needs_phase2` is never set
- Error is logged and flush worker returns
- No cleanup attempt
- `previous_composed_ops` is NOT updated (state remains consistent)

### Phase 2 Failure

If phase 2 fails:
- Error is logged
- Display may show ghosting but is still usable
- No retry
- State remains consistent for next flush

### Cancellation

If a new flush arrives while phase 2 is pending:
- New flush takes priority (existing pending job coalescing)
- Phase 2 ops are overwritten by the new flush
- Delayed work item is cancelled via `k_delayed_work_cancel()`
- This is desired behavior — user input responsiveness > cleanup

## Debug Instrumentation

Add `phase2_pending` flag to flush_start log entry:

```c
sq_debug_log_append("%lld:flush_start:ops=%d:mode=%d:phase2=%d",
                    (long long)k_uptime_get(),
                    sq_vm_runtime_display_active_job.op_count,
                    (int)sq_vm_runtime_display_active_job.refresh_mode,
                    (int)sq_vm_runtime_display_phase2_pending);
```

## Testing

### Unit Tests

- Mock `sq_display_backend_flush()` to return `needs_phase2=true`
- Verify flush worker stores phase 2 state correctly
- Verify delayed work item is submitted after phase 1
- Verify phase 2 is canceled (delayed work cancelled) when new flush arrives

### Hardware Verification

- Grid-cursor: press LEFT/RIGHT, measure phase 1 time (should be ~500-800ms vs current ~1.9s)
- Verify display shows cursor move quickly, then settles
- Verify rapid key presses don't queue up phase 2 flushes
- Debug log shows phase 1 flush_done, then phase 2 flush_start/flush_done later

### Benchmarks

- `device debug-log` captures phase 1 timing (flush_start → flush_done for current frame only)
- `device debug-log` captures phase 2 timing (flush_start → flush_done for cleanup)
- Compare total latency: phase 1 + phase 2 vs current single flush
- Measure perceived latency: time from key press to display update (phase 1 completion)
- Benchmark rapid key presses: 3 keys in 2 seconds, verify phase 2 is deferred/canceled appropriately

### Success Criteria

- Phase 1 completes in <800ms (vs current ~1.9s)
- Phase 2 completes within 2s of phase 1 (unless canceled)
- Total latency (phase 1 + phase 2) is similar to current single flush
- Perceived latency (phase 1 only) is significantly improved
