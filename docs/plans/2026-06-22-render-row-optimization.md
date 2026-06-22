# Plan: Optimize render_row Rendering Pipeline

## Problem

The composed display flush path (used by grid-cursor) re-renders from ops every time. For each of the 480 rows, it iterates through all 32 ops, evaluates a switch, and calls draw functions. This results in 15,360 iterations per flush, taking ~592ms of CPU time.

Most ops only affect a small portion of the screen. A text op at y=10 with height 8 only affects rows 10-17, but we're checking it for all 480 rows.

## Goal

Reduce CPU rendering time. Target: reduce from ~592ms to <100ms.

**Responsiveness metric**: button press → first visible change (phase 1 only). Phase 2 is deferred to idle and doesn't count for responsiveness.

## Approach

Two optimizations applied sequentially:

### 1. Row-Range Skipping

Skip ops that don't affect the current row.

**Op Y-Ranges:**

| Op Type | Y-Range |
|---------|---------|
| CLEAR | 0 to PANEL_HEIGHT (affects all rows) |
| TEXT | op->y to op->y + (7 × scale) × num_lines |
| RECT | op->y to op->y + op->u.rect.h |

**Implementation:**
- Add `op_y_min(op)` and `op_y_max(op)` helper functions
- In `render_row()`, skip ops where `y < op_y_min(op) || y >= op_y_max(op)`

### 2. Sort Ops by Y-Range

After row-range skipping, sort ops by y-min so we can `break` early when we pass the current row's range.

**Implementation:**
- Before streaming, sort ops by `op_y_min()` ascending
- In `render_row()`, break out of the loop when `op_y_min(op) > y`

### Files to Modify

- `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`:
  - Add `op_y_min()`, `op_y_max()` helper functions
  - Add `compare_op_y_min()` comparator for qsort
  - Add `build_y_index()` function
  - Modify `render_row()` to use sorted ops and y-index
  - Add timing instrumentation

### Verification

1. Build and flash firmware
2. Install and launch grid-cursor app
3. Send LEFT key and measure timing via `device debug-log`
4. Verify CPU rendering time reduced from ~592ms to <100ms
5. Verify total phase 1 flush time reduced from ~1108ms to <600ms
6. Verify visual output is identical (same rendered pixels)

### Results

- CPU rendering: ~592ms → ~68ms (8.7x improvement)
- Total phase 1: ~1108ms → ~575ms (1.9x improvement)
- Optimization 3 (y-index) cancelled — optimizations 1+2 achieved target

### Risk

Low — these are pure optimizations with no behavioral change. The rendered output should be identical.
