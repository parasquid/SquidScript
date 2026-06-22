# Dispatch Timing Instrumentation

## Goal

Measure the actual wall-clock time of each stage in the button-press-to-display
pipeline so we can identify the real bottleneck instead of estimating from clock
speeds and spec sheets.

## What to measure

Three new metrics, exposed through the existing `device resources` protocol:

| Metric ID | Name | What it measures |
|-----------|------|------------------|
| 56 | `last_sqbc_read_us` | Time spent in `fs_storage_read_sqbc()` during the last completed dispatch |
| 57 | `last_display_flush_us` | Time spent in `sq_display_backend_flush()` on the display worker thread |
| 58 | `last_state_save_us` | Time spent in `fs_storage_save_state()` during the last completed dispatch |

Combined with the existing `last_dispatch_us` (total VM dispatch), this gives
a clean breakdown:

```
last_dispatch_us:        120000   (total VM dispatch, already measured)
last_sqbc_read_us:        85000   (SQBC reads within dispatch)
last_state_save_us:        5000   (state.save() within dispatch)
last_display_flush_us:   350000   (display flush, separate thread)
```

Everything else (bytecode execution, context setup, display op recording) is
`last_dispatch_us - last_sqbc_read_us - last_state_save_us`.

## Implementation

### Step 1: Add timing fields to `vm_runtime.h`

Add after the existing dispatch metrics (line ~370):

```c
uint64_t last_dispatch_sqbc_read_us;
uint64_t dispatch_sqbc_read_us_acc;
uint64_t last_dispatch_state_save_us;
uint64_t dispatch_state_save_us_acc;
```

The `_acc` fields accumulate per-dispatch; the `last_` fields are latched at
dispatch completion.

### Step 2: Instrument SQBC reads in `vm_runtime.c`

In `runtime_read_exact_at()` (line 446), wrap the `backend->read_sqbc()` call:

```c
int32_t runtime_read_exact_at(void *user_data, size_t offset, uint8_t *out, size_t out_len)
{
    struct sq_vm_runtime *runtime = user_data;
    // ... existing null checks ...

    runtime->dispatch_sqbc_read_count++;
    runtime->dispatch_sqbc_read_bytes += out_len;

    uint64_t t0 = k_cycle_get_64();
    int result = runtime->backend->read_sqbc(runtime->backend->user_data, offset, out, out_len);
    runtime->dispatch_sqbc_read_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);

    return result;
}
```

### Step 3: Instrument state save in `vm_fs_storage.c`

Add a static accumulator for state save timing, and wrap the `save_state` call
in `fs_storage_save_state()` (line 186):

```c
static uint64_t fs_storage_state_save_us_acc;

uint64_t sq_vm_fs_storage_drain_state_save_us(void)
{
    uint64_t us = fs_storage_state_save_us_acc;
    fs_storage_state_save_us_acc = 0;
    return us;
}

static int fs_storage_save_state(void *user_data, const uint8_t *bytes, size_t len)
{
    struct sq_vm_fs_storage *storage = user_data;
    if (storage == NULL) {
        return -EINVAL;
    }
    uint64_t t0 = k_cycle_get_64();
    int result = write_file(storage->state_path, bytes, len);
    fs_storage_state_save_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
    return result;
}
```

Add the drain function declaration to `vm_fs_storage.h`.

### Step 4: Wire state save timing into dispatch metrics in `vm_runtime.c`

In `runtime_finish_dispatch_metrics()`, drain the accumulator:

```c
static void runtime_finish_dispatch_metrics(struct sq_vm_runtime *runtime, uint64_t start_cycles)
{
    // ... existing code ...
    runtime->last_dispatch_sqbc_read_us = runtime->dispatch_sqbc_read_us_acc;
    runtime->last_dispatch_state_save_us = sq_vm_fs_storage_drain_state_save_us();
    // ... rest ...
}
```

Reset the accumulators at dispatch start (line ~988):

```c
runtime->dispatch_sqbc_read_us_acc = 0;
```

### Step 5: Instrument display flush in `vm_runtime.c`

Add a static variable for display flush timing, measured in the display worker
thread:

```c
static uint64_t sq_vm_runtime_last_display_flush_us;
```

In `runtime_display_flush_worker()` (line 527), wrap the `sq_display_backend_flush()` call:

```c
while (true) {
    uint64_t t0 = k_cycle_get_64();
    int result = sq_display_backend_flush(...);
    sq_vm_runtime_last_display_flush_us = k_cyc_to_us_floor64(k_cycle_get_64() - t0);
    // ... rest ...
}
```

### Step 6: Add metric IDs in `device_protocol.c`

Add to the enum (after line 143):

```c
SQ_RESOURCE_METRIC_LAST_SQBC_READ_US = 56,
SQ_RESOURCE_METRIC_LAST_DISPLAY_FLUSH_US = 57,
SQ_RESOURCE_METRIC_LAST_STATE_SAVE_US = 58,
```

Add the `SQ_RESOURCE_METRIC(...)` calls in the resource handler (after line 2830):

```c
SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_LAST_SQBC_READ_US,
                   context->runtime == NULL ? 0 : context->runtime->last_dispatch_sqbc_read_us);
SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_LAST_DISPLAY_FLUSH_US, sq_vm_runtime_last_display_flush_us);
SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_LAST_STATE_SAVE_US,
                   context->runtime == NULL ? 0 : context->runtime->last_dispatch_state_save_us);
```

### Step 7: Add metric names in `squid-device-protocol/src/lib.rs`

Add to the `RESOURCE_METRIC_NAMES` array (after line 1041):

```rust
(56, "last_sqbc_read_us"),
(57, "last_display_flush_us"),
(58, "last_state_save_us"),
```

## Verification

1. Build firmware: `cargo run -p squidc -- hardware build --target xteink_x4`
2. Flash and install grid-cursor
3. Send a key press: `cargo run -p squidc -- device key RIGHT`
4. Query metrics: `cargo run -p squidc -- device resources | grep -E 'dispatch|sqbc|display|state'`
5. Verify the three new metrics appear with nonzero values
6. Verify `last_dispatch_us ≈ last_sqbc_read_us + last_state_save_us + bytecode_time`
7. Verify `last_display_flush_us` is measured separately (runs on display thread)

## Files changed

- `firmware/zephyr/src/vm_runtime.h` — add 4 timing fields
- `firmware/zephyr/src/vm_runtime.c` — instrument SQBC reads, reset accumulators, latch metrics, measure display flush
- `firmware/zephyr/src/vm_fs_storage.h` — declare `sq_vm_fs_storage_drain_state_save_us()`
- `firmware/zephyr/src/vm_fs_storage.c` — instrument state save, add accumulator + drain
- `firmware/zephyr/src/device_protocol.c` — add 3 metric IDs and emit them
- `compiler/rust/crates/squid-device-protocol/src/lib.rs` — add 3 metric names
