# Design: Display Flush Latency Breakdown

## Goal

Understand where the ~678ms phase 1 display flush latency comes from, then optimize based on evidence.

## Current State

- Total phase 1 flush: ~678ms
- Refresh (hardware): ~506ms (SSD1677 panel refresh, not optimizable)
- Remaining ~172ms: unknown split between CPU rendering, SPI streaming, and overhead
- CPU rendering and SPI streaming are interleaved in a single render_row/write_data loop — cannot be separated with current instrumentation

## Approach

Two parallel workstreams: finer instrumentation + binbook zero-CPU baseline.

### Part 1: Finer Timing Instrumentation

Add debug-log timing markers inside `stream_composed_1bpp_frame` to break down the render+stream phase.

**Markers to add:**

1. `sort_start` / `sort_done` — qsort + y_mins/y_maxs pre-computation time
2. `rows_000_done` through `rows_480_done` at every 48 rows (10 samples per flush) — per-band render+stream time

This gives us:
- Sort overhead (should be negligible, but good to confirm)
- Per-band timing to see if rendering is uniform or if some bands are slower (e.g. bands with more ops)

**File:** `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`

**Implementation:**
- Before qsort: `sq_debug_log_append("%lld:sort_start", ...)`
- After y_mins pre-computation: `sq_debug_log_append("%lld:sort_done", ...)`
- In the row loop, after every 48 rows: `sq_debug_log_append("%lld:rows_%d_done", ..., y)`
- After loop: existing `stream_done` marker stays

### Part 2: BinBook Zero-CPU Baseline

Binbook pages are pre-rendered pixel data — no draw ops, no CPU rendering. Streaming a binbook page should have near-zero CPU cost, isolating the SPI + refresh components.

**Problem:** The binbook-timing app opens `books/reader-one.binbook` but fails with `binbook.validate code=-22 (EINVAL)`. The device's binbook file may be from an older format or missing required sections.

**Fix approach:**
1. Generate a valid test binbook with full-size pages (800x480 GRAY2, 96000 bytes uncompressed) using `generate-test-binbook.py`
2. Upload to device via `squidc device content-put`
3. Update the binbook-timing app to reference the uploaded file name

**If the existing device binbook is fixable:** investigate what section is failing validation (the `binbook_find_sections` check at `vm_runtime_binbook.c:183` requires string_table, page_index, nav_index, chapter_index, page_data with matching entry sizes).

**If not:** use the generated test binbook. It has full-size pages so SPI streaming time matches real usage.

**File:** `examples/binbook-timing/main.squid` (update file path if needed)

### Verification

1. Build firmware with instrumentation changes
2. Flash to XTEINK X4
3. Install + launch grid-cursor, send LEFT, capture debug-log
4. Upload test binbook, install + launch binbook-timing, send RIGHT, capture debug-log
5. Compare timing breakdown:
   - grid-cursor: sort + per-band render+stream + refresh = total
   - binbook-timing: near-zero sort + per-band stream-only + refresh = total
   - Delta between the two = CPU rendering contribution

### Expected Outcomes

| Component | grid-cursor | binbook-timing | Delta |
|-----------|------------|----------------|-------|
| Sort | ~negligible | ~negligible | — |
| CPU render + SPI stream | ~172ms | ~stream-only | CPU cost |
| Refresh | ~506ms | ~506ms | — |
| **Total** | **~678ms** | **~506ms + SPI** | **CPU rendering** |

If binbook-timing total is close to 506ms, CPU rendering doesn't matter and the bottleneck is the panel refresh.
If binbook-timing total is significantly less than 678ms, CPU rendering is a real contributor worth optimizing.
