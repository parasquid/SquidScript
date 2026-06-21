# Compact Display-Op Representation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Free ~55 KiB of XTEINK X4 static DRAM by compacting the 376-byte display-op struct to a ~84-byte tagged union with typed colors and out-of-band BinBook page storage, clearing the 48 KiB linker headroom target.

**Architecture:** The op struct becomes a tagged union (kind + x + y common header, variant payload per CLEAR/TEXT/RECT, BINBOOK flag-only). Colors become a typed `uint8_t` palette index converted at the FFI producer boundary. The BinBook page moves out of the op array into a heap-allocated flush-job snapshot (zero page RAM for non-BinBook apps). A 3-slot heap page ring caches adjacent page metadata for fast BinBook page turns.

**Tech Stack:** Zephyr C17, ztest/Twister native_sim, XTEINK X4 ESP32-C3 hardware.

**Design spec:** `docs/specs/2026-06-20-x4-ram-reduction-design.md`

**Baseline:** Linker DRAM 375,440 / 378,640 = 3,200 B headroom. Four display-op arrays = 72,224 B.

---

### Task 1: Typed color representation

The compact display-op slice established `sq_display_color_t` and the named
white, black, and unset values in `sq_display_color.h`. Color names are now
resolved by the compiler's `color.*` namespace and cross the VM/FFI boundary as
typed palette indices, so firmware contains no string palette parser.

### Task 2: Compact display-op struct and update all producers and consumers

This is the core compaction. The struct changes from 376 bytes to ~84 bytes.
All producers (`vm_runtime_display.c`), consumers
(`ssd1677_gdeq0426t82_display.c`, `ssd1677_gray2.c`), the flush job copy
(`vm_runtime.c`), and all tests must be updated atomically.

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime.h`
- Modify: `firmware/zephyr/src/vm_runtime_display.c`
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`
- Modify: `firmware/zephyr/src/ssd1677_gray2.c`
- Modify: `firmware/zephyr/src/vm_runtime.c`
- Modify: `firmware/zephyr/tests/protocol/src/main.c`

- [ ] **Step 1: Define the new op struct**

In `vm_runtime.h`, replace `struct sq_vm_runtime_display_op` (lines 283-294)
with the tagged union:

```c
struct sq_vm_runtime_display_op {
	enum sq_vm_runtime_display_op_kind kind;
	int32_t x;
	int32_t y;
	union {
		struct {
			sq_display_color_t color;
		} clear;
		struct {
			char text[SQ_VM_RUNTIME_DISPLAY_TEXT_LEN];
			int32_t font_height;
			sq_display_color_t color;
		} text;
		struct {
			int32_t w;
			int32_t h;
			sq_display_color_t fill_color;
			sq_display_color_t stroke_color;
		} rect;
	} u;
};
```

Include `sq_display_color.h` at the top of `vm_runtime.h`. The
`BINBOOK_DRAWABLE` kind uses only `kind`, `x`, `y`; the page travels
out-of-band (Task 3). Add a static_assert for the size.

- [ ] **Step 2: Update the producers in vm_runtime_display.c**

Update `runtime_display_clear` to store the typed color:

```c
op->kind = SQ_VM_RUNTIME_DISPLAY_OP_CLEAR;
op->u.clear.color = color;
```

Update `runtime_display_text`:

```c
op->kind = SQ_VM_RUNTIME_DISPLAY_OP_TEXT;
runtime_display_copy_text(op->u.text.text, sizeof(op->u.text.text), text, text_len);
op->x = options->x;
op->y = options->y;
op->u.text.font_height = options->font_height;
op->u.text.color = options->text_color;
```

Update `runtime_display_rect`:

```c
op->kind = SQ_VM_RUNTIME_DISPLAY_OP_RECT;
op->x = options->x;
op->y = options->y;
op->u.rect.w = options->w;
op->u.rect.h = options->h;
op->u.rect.fill_color = options->fill_color;
op->u.rect.stroke_color = options->stroke_color;
```

Update `runtime_display_draw` (BINBOOK_DRAWABLE):

```c
op->kind = SQ_VM_RUNTIME_DISPLAY_OP_BINBOOK_DRAWABLE;
op->x = options->x;
op->y = options->y;
/* page is stored out-of-band in runtime->drawable.page; the flush
 * job copies it into a snapshot at handoff time (Task 3). */
```

- [ ] **Step 3: Update the consumers in ssd1677_gdeq0426t82_display.c**

Replace `ssd1677_color_is_black(const char *color)` with calls to
`sq_display_color_is_black()` on the typed color value.

Update `draw_text_row` to read `op->u.text.text`, `op->u.text.font_height`,
and `op->u.text.color`.

Update `draw_rect_row` to read `op->u.rect.w`, `op->u.rect.h`,
`op->u.rect.fill_color`, `op->u.rect.stroke_color`.

Update `render_row` and the CLEAR path: for `SQ_VM_RUNTIME_DISPLAY_OP_CLEAR`,
the clear color is `op->u.clear.color`.

Update `find_binbook_drawable_op`: the BINBOOK_DRAWABLE op no longer carries
the page; the page is passed separately (see Task 3). For now, the function
just finds the BINBOOK_DRAWABLE op by kind.

- [ ] **Step 4: Update the consumers in ssd1677_gray2.c**

Update `composed_op_equal`: compare typed colors instead of `strcmp`:

```c
static bool composed_op_equal(const struct sq_vm_runtime_display_op *a,
			      const struct sq_vm_runtime_display_op *b)
{
	if (a == NULL || b == NULL || a->kind != b->kind) {
		return false;
	}
	if (a->x != b->x || a->y != b->y) {
		return false;
	}
	switch (a->kind) {
	case SQ_VM_RUNTIME_DISPLAY_OP_CLEAR:
		return a->u.clear.color == b->u.clear.color;
	case SQ_VM_RUNTIME_DISPLAY_OP_TEXT:
		return strcmp(a->u.text.text, b->u.text.text) == 0 &&
		       a->u.text.font_height == b->u.text.font_height &&
		       a->u.text.color == b->u.text.color;
	case SQ_VM_RUNTIME_DISPLAY_OP_RECT:
		return a->u.rect.w == b->u.rect.w && a->u.rect.h == b->u.rect.h &&
		       a->u.rect.fill_color == b->u.rect.fill_color &&
		       a->u.rect.stroke_color == b->u.rect.stroke_color;
	default:
		return true;
	}
}
```

Update `last_clear_color`: return `op->u.clear.color` (typed).
Update `op_exists_in`, `window_include_op` for the new field paths.
Note: `composed_op_equal` continues to ignore BINBOOK_DRAWABLE page data,
matching current behavior (BINBOOK ops are compared by position only).

- [ ] **Step 5: Update the flush job copy in vm_runtime.c**

`runtime_display_copy_flush_job` memcpys ops — no change needed (the ops are
still by-value in the job, just smaller). Verify the memcpy size is
`op_count * sizeof(runtime->display_ops[0])` which adapts automatically.

- [ ] **Step 6: Update all tests in main.c**

Update every test that reads `op->text`, `op->fill_color`, `op->stroke_color`,
`op->font_height`, `op->w`, `op->h`, `op->binbook_page` to use the new union
field paths. Key tests to update:
- `test_vm_runtime_records_physical_display_clear_and_text_ops` (line ~6118)
- `test_vm_runtime_records_physical_display_rect_ops` (line ~6149)
- `test_vm_runtime_records_binbook_drawable_display_op` (line ~6275)
- `test_display_op_buffer_preserves_clear_for_library_like_screen` (line ~6467)
- `test_ssd1677_composed_dirty_window_tracks_changed_highlight_ops` (line ~6528)
- `test_ssd1677_1bpp_compositor_draws_stroked_rect` (line ~6358)
- `test_ssd1677_1bpp_compositor_moves_highlight_between_frames` (line ~6380)

For BINBOOK_DRAWABLE tests: the page is no longer in the op. The test must
verify the page is stored out-of-band (via `runtime->drawable.page` or the
flush job snapshot). Task 3 handles the page snapshot; for now, the test
checks `runtime->drawable.page` was set correctly.

- [ ] **Step 7: Run the protocol ztests and verify GREEN**

Run `scripts/zephyr-test-protocol.sh`. Expected: all previously-passing tests
pass with the new struct shape; 33 pre-existing failures unchanged.

- [ ] **Step 8: Verify the static DRAM win**

Build X4: `cargo run -p squidc -- target build --target xteink-x4`. Record
linker DRAM. Expected: DRAM drops by approximately
4 × 48 × (376 - 84) ≈ 56 KiB. Headroom should be ~59 KiB.

- [ ] **Step 9: Commit Task 2**

```sh
git add firmware/zephyr/src/vm_runtime.h \
  firmware/zephyr/src/vm_runtime_display.c \
  firmware/zephyr/src/ssd1677_gdeq0426t82_display.c \
  firmware/zephyr/src/ssd1677_gray2.c \
  firmware/zephyr/src/vm_runtime.c \
  firmware/zephyr/tests/protocol/src/main.c
git commit -m "refactor(display): compact display-op to tagged union with typed colors"
```

### Task 3: Out-of-band BinBook page snapshot

Move the BinBook page out of the op array into a heap-allocated snapshot in
the flush job. Zero page RAM for non-BinBook apps.

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime.c`
- Modify: `firmware/zephyr/src/vm_runtime_display_backend.h`
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`
- Modify: `firmware/zephyr/tests/protocol/src/main.c`

- [ ] **Step 1: Write the failing test**

Add a test that verifies the BinBook page snapshot is heap-allocated at flush
handoff and freed after flush:

```c
ZTEST(squidscript_protocol, test_display_flush_job_carries_binbook_page_snapshot)
{
	/* Produce a BINBOOK_DRAWABLE op via runtime_display_draw,
	 * trigger runtime_flush_display_if_dirty, then verify the
	 * flush job's binbook_page pointer is non-NULL and contains
	 * the expected page metadata. After the worker flushes and
	 * the job is cleared, the pointer must be NULL. */
}
```

- [ ] **Step 2: Update the flush job struct**

In `vm_runtime.c`, add a `binbook_page` pointer to the flush job:

```c
struct sq_vm_runtime_display_flush_job {
	struct sq_vm_runtime *runtime;
	struct sq_vm_runtime_display_op ops[SQ_VM_RUNTIME_DISPLAY_OP_MAX];
	uint8_t op_count;
	enum sq_vm_runtime_display_refresh_mode refresh_mode;
	struct sq_vm_runtime_binbook_page *binbook_page;
};
```

- [ ] **Step 3: Heap-allocate the page snapshot at handoff**

In `runtime_display_copy_flush_job`, if the runtime has a drawable page
(`runtime->drawable.active && runtime->drawable.page.path[0] != '\0'`),
`k_malloc` a `struct sq_vm_runtime_binbook_page`, copy the page into it,
and store the pointer in the job. Otherwise set `binbook_page = NULL`.

- [ ] **Step 4: Free the page snapshot after flush**

In `runtime_display_flush_worker`, after `sq_display_backend_flush` returns,
free the page: `k_free(active_job.binbook_page); active_job.binbook_page = NULL;`

On reset (`sq_vm_runtime_reset`), free any pending page. The active job's page
is freed by the worker before the join.

- [ ] **Step 5: Update the backend flush signature**

In `vm_runtime_display_backend.h`, add the page parameter:

```c
int sq_display_backend_flush(const struct sq_vm_runtime_display_op *ops, size_t op_count,
			     enum sq_vm_runtime_display_refresh_mode refresh_mode,
			     const struct sq_vm_runtime_binbook_page *binbook_page);
```

Update the real SSD1677 implementation to read `binbook_page` instead of
`binbook->binbook_page`. Update the non-SSD1677 stub and the test stub.

- [ ] **Step 6: Update lifecycle release paths**

In `runtime_flush_display_if_dirty`, clear `runtime->drawable` after handing
off the page snapshot (matching today's memset of display_ops). In
`sq_vm_runtime_reset`, ensure the drawable is cleared.

- [ ] **Step 7: Run tests and verify GREEN**

Run `scripts/zephyr-test-protocol.sh` and hardware grid-cursor + binbook-reader.

- [ ] **Step 8: Commit Task 3**

```sh
git commit -m "refactor(display): move BinBook page out-of-band to heap snapshot"
```

### Task 4: BinBook page ring (3-slot circular buffer)

Replace `runtime->drawable.page` (single slot) with a 3-slot heap circular
buffer for page-turn prefetch. Allocated lazily on first `binbook.readPage`,
freed on reset. Forward page turns are ring hits + prefetch of the new next.

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime.h`
- Modify: `firmware/zephyr/src/vm_runtime_binbook.c`
- Modify: `firmware/zephyr/src/vm_runtime_display.c`
- Modify: `firmware/zephyr/src/vm_runtime.c`
- Test: `firmware/zephyr/tests/protocol/src/main.c`

- [ ] **Step 1: Write the failing test**

Test that `runtime_binbook_read_page` populates the ring, a second read of
the same page is a hit (no file reopen), and a read of an adjacent page is a
hit if prefetched.

- [ ] **Step 2: Define the ring struct**

In `vm_runtime.h`, replace `struct sq_vm_runtime_drawable_handle` with a ring:

```c
#define SQ_VM_RUNTIME_BINBOOK_PAGE_RING_SLOTS 3

struct sq_vm_runtime_binbook_page_ring {
	struct sq_vm_runtime_binbook_page *slots; /* heap, SQ_VM_RUNTIME_BINBOOK_PAGE_RING_SLOTS */
	int32_t page_index[SQ_VM_RUNTIME_BINBOOK_PAGE_RING_SLOTS];
	bool valid[SQ_VM_RUNTIME_BINBOOK_PAGE_RING_SLOTS];
	uint8_t head; /* index of current page */
	bool active;
};
```

- [ ] **Step 3: Implement ring lookup and insertion**

On `binbook.readPage(idx)`:
- If the ring is not allocated, allocate `k_malloc(SLOTS * sizeof(page))`.
- If `idx` is in the ring (scan `page_index[]`), set `head` to that slot.
  Return the slot's page as the drawable.
- If miss: evict the slot furthest from `idx` (oldest behind), load `idx`
  from flash into that slot, set `head`. Prefetch `idx+1` into the next
  evictable slot if it is not present.

- [ ] **Step 4: Wire the ring into the read-page callback**

`runtime_binbook_read_page` uses the ring instead of directly writing
`runtime->drawable.page`. The drawable handle returned to the VM references
the ring's head slot. `runtime_display_draw` copies the head slot's page into
the flush job snapshot (Task 3).

- [ ] **Step 5: Free the ring on reset**

In `sq_vm_runtime_reset`, `k_free` the ring slots and zero the ring control.
On `binbook.open` with a different path, clear the ring (invalidate all slots).

- [ ] **Step 6: Run tests and verify GREEN**

- [ ] **Step 7: Commit Task 4**

```sh
git commit -m "feat(binbook): add 3-slot page ring for fast page turns"
```

### Task 5: Documentation and hardware acceptance

**Files:**
- Modify: `docs/specs/2026-06-20-x4-ram-reduction-design.md`
- Modify: `docs/plans/2026-06-20-compact-display-op.md`
- Modify: `ROADMAP.md`

- [ ] **Step 1: Run final native verification**

Run `scripts/zephyr-test-protocol.sh` and compare the failure set with the
recorded baseline (33 pre-existing). No new failures.

- [ ] **Step 2: Build and flash X4 sequentially**

```sh
cargo run -p squidc -- target build --target xteink-x4
cargo run -p squidc -- target flash --target xteink-x4
```

Record linker DRAM. Require ≥48 KiB headroom.

- [ ] **Step 3: Run the X4 RAM workload script**

```sh
SKIP_FLASH=1 scripts/xteink-x4-measure-ram-workloads.sh
```

Record the `summary.tsv`. Compare heap/stack high-water with the Plan 1
baseline. Verify grid-cursor and binbook-reader render correctly with empty
`device errors`.

- [ ] **Step 4: Update docs and roadmap**

Mark the display-op compaction as done in ROADMAP.md. Update the design spec
with the verified outcome. Note the typed color at the FFI boundary as a
temporary bridge that Plan 3 (color constants) will replace.

- [ ] **Step 5: Commit and push**

Commit only the task-owned docs and ROADMAP. Push.
