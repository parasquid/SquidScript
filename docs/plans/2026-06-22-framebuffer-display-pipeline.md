# Framebuffer Display Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the op-based display pipeline with a 1bpp framebuffer, enabling pixel-level operations and in-memory graphics.

**Architecture:** ALL display draws rasterize directly into a 48 KiB static framebuffer in the SSD1677 driver — including binbook page decompression. At flush, the buffer is SPI-transferred to the e-paper. No direct SPI drawing paths remain. The op array, double-buffering statics, and old op-based flush are removed entirely. One code path for all rendering.

**Tech Stack:** C, Zephyr RTOS, SPI, SSD1677 e-paper driver, protocol ztests

---

## Task 1: Update display backend header

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime_display_backend.h`

- [ ] **Step 1: Replace the old interface with new rasterize/flush functions**

```c
#ifndef SQUIDSCRIPT_VM_RUNTIME_DISPLAY_BACKEND_H
#define SQUIDSCRIPT_VM_RUNTIME_DISPLAY_BACKEND_H

#include "vm_runtime.h"

void sq_display_backend_rasterize_clear(sq_display_color_t color);
void sq_display_backend_rasterize_text(const char *text, int32_t x, int32_t y,
				       int32_t font_height, sq_display_color_t color);
void sq_display_backend_rasterize_rect(int32_t x, int32_t y, int32_t w, int32_t h,
				       sq_display_color_t fill, sq_display_color_t stroke);
void sq_display_backend_rasterize_binbook(const struct sq_vm_runtime_binbook_page *page);
int sq_display_backend_flush_framebuffer(enum sq_vm_runtime_display_refresh_mode mode);

const uint8_t *sq_display_backend_framebuffer(void);
size_t sq_display_backend_framebuffer_size(void);

int sq_display_backend_window_probe(const char *pattern);
void sq_display_backend_reset(void);

#endif
```

- [ ] **Step 2: Verify the header compiles**

Run: `cargo run -p squidc -- target build --target xteink-x4` (or the relevant build command)
Expected: Compilation error in files that still reference old `sq_display_backend_flush` signature

- [ ] **Step 3: Commit**

```bash
git add firmware/zephyr/src/vm_runtime_display_backend.h
git commit -m "display: replace op-based backend interface with framebuffer rasterize/flush"
```

---

## Task 2: Update runtime struct — remove display ops, add framebuffer

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime.h`
- Modify: `firmware/zephyr/runtime_limits.json`

- [ ] **Step 1: Remove display op fields from `sq_vm_runtime`**

In `vm_runtime.h`, remove these lines (around 417-420):

```c
struct sq_vm_runtime_display_op display_ops[SQ_VM_RUNTIME_DISPLAY_OP_MAX];
uint8_t display_op_count;
```

Replace with:

```c
bool display_needs_flush;
```

Keep `display_refresh_mode` and `display_dirty` — `display_dirty` is now set by `display_needs_flush` (rename for clarity or keep as-is).

- [ ] **Step 2: Remove `SQ_VM_RUNTIME_DISPLAY_OP_MAX` and `SQ_VM_RUNTIME_DISPLAY_TEXT_LEN` constants**

Remove these `#ifndef` blocks (around 51-56):

```c
#ifndef SQ_VM_RUNTIME_DISPLAY_OP_MAX
#define SQ_VM_RUNTIME_DISPLAY_OP_MAX 48
#endif
#ifndef SQ_VM_RUNTIME_DISPLAY_TEXT_LEN
#define SQ_VM_RUNTIME_DISPLAY_TEXT_LEN 64
#endif
```

- [ ] **Step 3: Keep `sq_vm_runtime_display_op` struct and enums for binbook path**

The `sq_vm_runtime_display_op` struct and `enum sq_vm_runtime_display_op_kind` are still needed for the binbook drawable display op. Do NOT remove them.

- [ ] **Step 4: Add framebufferBytes to runtime_limits.json**

```json
{
  "vm_runtime": {
    ...
    "work_stack_size": 24576,
    "display_work_stack_size": 4096,
    "framebuffer_bytes": 48000
  }
}
```

- [ ] **Step 5: Verify compilation**

Run: build the firmware
Expected: Compilation errors in files that reference removed `display_ops[]` and `display_op_count`

- [ ] **Step 6: Commit**

```bash
git add firmware/zephyr/src/vm_runtime.h firmware/zephyr/runtime_limits.json
git commit -m "runtime: remove display op array from sq_vm_runtime, add framebuffer_bytes limit"
```

---

## Task 3: Add framebuffer and rasterize functions to SSD1677 driver

**Files:**
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`

- [ ] **Step 1: Add framebuffer static and size constant**

After the existing `#define` block (around line 168), add:

```c
#define FB_FRAMEBUFFER_SIZE (PANEL_WIDTH * PANEL_HEIGHT / 8U)
static uint8_t fb_framebuffer[FB_FRAMEBUFFER_SIZE];
```

- [ ] **Step 2: Implement framebuffer accessor functions**

```c
const uint8_t *sq_display_backend_framebuffer(void)
{
	return fb_framebuffer;
}

size_t sq_display_backend_framebuffer_size(void)
{
	return FB_FRAMEBUFFER_SIZE;
}
```

- [ ] **Step 3: Implement `sq_display_backend_rasterize_clear()`**

This fills the entire framebuffer. The SSD1677 uses inverted pixel logic (0 = black, 1 = white).

```c
void sq_display_backend_rasterize_clear(sq_display_color_t color)
{
	if (ssd1677_color_is_black(color)) {
		memset(fb_framebuffer, 0x00, FB_FRAMEBUFFER_SIZE);
	} else {
		memset(fb_framebuffer, 0xFF, FB_FRAMEBUFFER_SIZE);
	}
}
```

- [ ] **Step 4: Implement `sq_display_backend_rasterize_text()`**

Adapt the existing `draw_text_row()` logic to write into the framebuffer instead of a single row. The framebuffer is row-major: byte offset for pixel (x, y) is `(y * ROW_BYTES) + (x / 8)`, bit position is `0x80 >> (x % 8)`, with the SSD1677's reversed bit order.

```c
void sq_display_backend_rasterize_text(const char *text, int32_t x, int32_t y,
				       int32_t font_height, sq_display_color_t color)
{
	if (text == NULL || font_height <= 0) {
		return;
	}
	bool text_black = sq_display_color_is_black(color);
	uint8_t scale = (uint8_t)(font_height / 7);
	if (scale == 0) {
		scale = 1;
	}
	uint16_t text_y = y < 0 ? 0 : (uint16_t)y;
	uint16_t cursor_x = x < 0 ? 0 : (uint16_t)x;

	for (size_t i = 0; text[i] != '\0'; ++i) {
		const uint8_t *glyph = glyph_for(text[i]);

		for (uint8_t glyph_row = 0; glyph_row < 7U; ++glyph_row) {
			for (uint8_t row = 0; row < scale; ++row) {
				uint16_t logical_y = text_y + (uint16_t)(glyph_row * scale) + row;
				if (logical_y >= PANEL_HEIGHT) {
					continue;
				}
				for (uint8_t col = 0; col < 5U; ++col) {
					if ((glyph[glyph_row] & (0x10U >> col)) == 0U) {
						continue;
					}
					for (uint8_t dx = 0; dx < scale; ++dx) {
						uint16_t logical_x = cursor_x + (uint16_t)(col * scale) + dx;
						uint16_t physical_x = 0;
						uint16_t physical_y = 0;

						if (logical_to_physical(logical_x, logical_y,
									&physical_x, &physical_y)) {
							uint16_t ram_x = (uint16_t)(PANEL_WIDTH - 1U - physical_x);
							size_t byte_idx = (size_t)physical_y * ROW_BYTES + ram_x / 8U;
							uint8_t mask = (uint8_t)(0x80U >> (ram_x % 8U));

							if (text_black) {
								fb_framebuffer[byte_idx] &= (uint8_t)~mask;
							} else {
								fb_framebuffer[byte_idx] |= mask;
							}
						}
					}
				}
			}
		}
		cursor_x += (uint16_t)(6U * scale);
		if (cursor_x >= LOGICAL_WIDTH) {
			return;
		}
	}
}
```

- [ ] **Step 5: Implement `sq_display_backend_rasterize_rect()`**

Adapt the existing `draw_rect_row()` logic to write into the framebuffer.

```c
void sq_display_backend_rasterize_rect(int32_t x, int32_t y, int32_t w, int32_t h,
				       sq_display_color_t fill_color, sq_display_color_t stroke_color)
{
	if (w <= 0 || h <= 0) {
		return;
	}
	bool has_fill = sq_display_color_is_set(fill_color);
	bool has_stroke = sq_display_color_is_set(stroke_color);
	bool fill_black = ssd1677_color_is_black(fill_color);
	bool stroke_black = ssd1677_color_is_black(stroke_color);

	if (!has_fill && !has_stroke) {
		return;
	}
	int32_t left = x < 0 ? 0 : x;
	int32_t top = y < 0 ? 0 : y;
	int32_t right = x + w;
	int32_t bottom = y + h;

	if (right > (int32_t)PANEL_WIDTH) {
		right = (int32_t)PANEL_WIDTH;
	}
	if (bottom > (int32_t)PANEL_HEIGHT) {
		bottom = (int32_t)PANEL_HEIGHT;
	}
	if (left >= right || top >= bottom) {
		return;
	}
	for (int32_t py = top; py < bottom; ++py) {
		for (int32_t px = left; px < right; ++px) {
			bool draw = false;
			bool black = false;

			if (has_fill) {
				draw = true;
				black = fill_black;
			}
			if (has_stroke && (py == top || py == bottom - 1 || px == left || px == right - 1)) {
				draw = true;
				black = stroke_black;
			}
			if (draw) {
				uint16_t ram_x = (uint16_t)(PANEL_WIDTH - 1U - (uint16_t)px);
				size_t byte_idx = (size_t)py * ROW_BYTES + ram_x / 8U;
				uint8_t mask = (uint8_t)(0x80U >> (ram_x % 8U));

				if (black) {
					fb_framebuffer[byte_idx] &= (uint8_t)~mask;
				} else {
					fb_framebuffer[byte_idx] |= mask;
				}
			}
		}
	}
}
```

- [ ] **Step 6: Implement `sq_display_backend_flush_framebuffer()`**

Send the framebuffer to the e-paper via SPI. Adapt the existing flush logic but send the buffer directly instead of row-by-row.

```c
int sq_display_backend_flush_framebuffer(enum sq_vm_runtime_display_refresh_mode mode)
{
	ARG_UNUSED(mode);
	/* Placeholder — real SPI transfer implemented in Task 7 */
	return 0;
}
```

- [ ] **Step 7: Verify compilation**

Run: build the firmware
Expected: New functions compile without errors

- [ ] **Step 8: Commit**

```bash
git add firmware/zephyr/src/ssd1677_gdeq0426t82_display.c
git commit -m "display: add framebuffer and rasterize functions to SSD1677 driver"
```

---

## Task 4: Update `vm_runtime_display.c` — rasterize into framebuffer

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime_display.c`

- [ ] **Step 1: Replace op-appending in `runtime_display_clear()`**

Replace the `runtime_display_append_op()` call with `sq_display_backend_rasterize_clear()`:

```c
void runtime_display_clear(void *user_data, uint8_t color)
{
	struct sq_vm_runtime *runtime = user_data;
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];
	int written = snprintf(line, sizeof(line), "draw=clear color=%u", (unsigned int)color);

	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(runtime, line);
	}
	sq_display_backend_rasterize_clear(color);
	if (runtime != NULL) {
		runtime->display_needs_flush = true;
		runtime->display_dirty = true;
	}
}
```

- [ ] **Step 2: Replace op-appending in `runtime_display_text()`**

```c
void runtime_display_text(void *user_data, const uint8_t *text, size_t text_len,
				 const SqvmDisplayTextOptions *options)
{
	struct sq_vm_runtime *runtime = user_data;
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=text text=\"%.*s\" x=%d y=%d",
			       (int)text_len, text == NULL ? (const uint8_t *)"" : text,
			       options->x, options->y);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(runtime, line);
	}
	char text_buf[SQ_VM_RUNTIME_DISPLAY_TEXT_LEN];
	size_t copy_len = text_len < sizeof(text_buf) - 1 ? text_len : sizeof(text_buf) - 1;

	if (text != NULL && copy_len > 0) {
		memcpy(text_buf, text, copy_len);
	}
	text_buf[copy_len] = '\0';
	sq_display_backend_rasterize_text(text_buf, options->x, options->y,
					  options->font_height, options->text_color);
	if (runtime != NULL) {
		runtime->display_needs_flush = true;
		runtime->display_dirty = true;
	}
}
```

- [ ] **Step 3: Replace op-appending in `runtime_display_rect()`**

```c
void runtime_display_rect(void *user_data, const SqvmDisplayRectOptions *options)
{
	struct sq_vm_runtime *runtime = user_data;
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=rect x=%d y=%d w=%d h=%d", options->x,
			       options->y, options->w, options->h);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(runtime, line);
	}
	sq_display_backend_rasterize_rect(options->x, options->y, options->w, options->h,
					  options->fill_color, options->stroke_color);
	if (runtime != NULL) {
		runtime->display_needs_flush = true;
		runtime->display_dirty = true;
	}
}
```

- [ ] **Step 4: Keep `runtime_display_draw()` for binbook path**

The binbook drawable op still uses the op-based path. Keep the existing `runtime_display_draw()` implementation as-is for now — it still appends a `BINBOOK_DRAWABLE` op. This will be addressed separately.

- [ ] **Step 5: Remove `runtime_display_append_op()` function**

The function is no longer needed for clear/text/rect. It's still needed for binbook. Keep it but mark it as binbook-only. Or remove if binbook is refactored later.

Actually, keep it — binbook still uses it.

- [ ] **Step 6: Verify compilation**

Run: build the firmware
Expected: Compiles cleanly

- [ ] **Step 7: Commit**

```bash
git add firmware/zephyr/src/vm_runtime_display.c
git commit -m "display: rasterize clear/text/rect into framebuffer instead of appending ops"
```

---

## Task 5: Update `vm_runtime.c` — simplified flush job

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime.c`

- [ ] **Step 1: Simplify `sq_vm_runtime_display_flush_job` struct**

Replace:

```c
struct sq_vm_runtime_display_flush_job {
	struct sq_vm_runtime *runtime;
	struct sq_vm_runtime_display_op ops[SQ_VM_RUNTIME_DISPLAY_OP_MAX];
	uint8_t op_count;
	enum sq_vm_runtime_display_refresh_mode refresh_mode;
	struct sq_vm_runtime_binbook_page *binbook_page;
};
```

With:

```c
struct sq_vm_runtime_display_flush_job {
	struct sq_vm_runtime *runtime;
	enum sq_vm_runtime_display_refresh_mode refresh_mode;
	struct sq_vm_runtime_binbook_page *binbook_page;
};
```

- [ ] **Step 2: Simplify `runtime_display_copy_flush_job()`**

Replace the op-copying logic:

```c
static void runtime_display_copy_flush_job(struct sq_vm_runtime_display_flush_job *job,
					   struct sq_vm_runtime *runtime)
{
	memset(job, 0, sizeof(*job));
	job->runtime = runtime;
	job->refresh_mode = runtime->display_refresh_mode;
	if (runtime->drawable.active && runtime->drawable.page.path[0] != '\0') {
		job->binbook_page = k_malloc(sizeof(*job->binbook_page));
		if (job->binbook_page != NULL) {
			*job->binbook_page = runtime->drawable.page;
		}
	}
}
```

- [ ] **Step 3: Simplify `runtime_flush_display_if_dirty()`**

Replace the condition check and reset logic:

```c
static void runtime_flush_display_if_dirty(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL || !runtime->display_dirty || !runtime->display_needs_flush) {
		return;
	}
	/* ... mutex and thread creation logic stays the same ... */
	/* Replace the reset at the end: */
	runtime->display_refresh_mode = SQ_VM_RUNTIME_DISPLAY_REFRESH_AUTO;
	runtime->display_dirty = false;
	runtime->display_needs_flush = false;
}
```

- [ ] **Step 4: Update `runtime_display_flush_worker()` to call new flush**

Replace the `sq_display_backend_flush()` call:

```c
int result = sq_display_backend_flush_framebuffer(sq_vm_runtime_display_active_job.refresh_mode);
```

Remove the `needs_phase2` handling and phase2 work scheduling.

- [ ] **Step 5: Remove phase2-related code**

Remove:
- `sq_vm_runtime_display_phase2_pending`
- `sq_vm_runtime_display_phase2_job`
- `sq_vm_runtime_display_phase2_work`
- `sq_vm_runtime_display_phase2_work_initialized`
- `runtime_display_phase2_handler()`
- Phase2 logic in `runtime_display_flush_worker()` and `runtime_flush_display_if_dirty()`

- [ ] **Step 6: Update dispatch completion check**

The existing code at line ~1136:

```c
if (runtime->result.outcome == SQVM_DISPATCH_COMPLETE) {
    runtime_flush_display_if_dirty(runtime);
}
```

This stays the same — `runtime_flush_display_if_dirty` now checks `display_needs_flush`.

- [ ] **Step 7: Verify compilation**

Run: build the firmware
Expected: Compiles cleanly, no references to removed fields

- [ ] **Step 8: Commit**

```bash
git add firmware/zephyr/src/vm_runtime.c
git commit -m "runtime: simplify display flush job to use framebuffer, remove phase2"
```

---

## Task 6: Update protocol tests

**Files:**
- Modify: `firmware/zephyr/tests/protocol/src/main.c`

- [ ] **Step 1: Update mock backend — replace `sq_display_backend_flush` with rasterize mocks**

Replace the existing mock:

```c
int sq_display_backend_flush(const struct sq_vm_runtime_display_op *ops, size_t op_count,
			     enum sq_vm_runtime_display_refresh_mode refresh_mode,
			     const struct sq_vm_runtime_binbook_page *binbook_page,
			     bool *needs_phase2)
```

With:

```c
static bool test_rasterize_clear_called;
static sq_display_color_t test_rasterize_clear_color;
static bool test_rasterize_text_called;
static char test_rasterize_text_buf[256];
static int32_t test_rasterize_text_x, test_rasterize_text_y;
static bool test_rasterize_rect_called;
static int32_t test_rasterize_rect_x, test_rasterize_rect_y;
static uint8_t test_fb_framebuffer[48000];
static bool test_display_flush_block;

void sq_display_backend_rasterize_clear(sq_display_color_t color)
{
	test_rasterize_clear_called = true;
	test_rasterize_clear_color = color;
	if (color == SQ_DISPLAY_COLOR_BLACK || color == 0) {
		memset(test_fb_framebuffer, 0x00, sizeof(test_fb_framebuffer));
	} else {
		memset(test_fb_framebuffer, 0xFF, sizeof(test_fb_framebuffer));
	}
}

void sq_display_backend_rasterize_text(const char *text, int32_t x, int32_t y,
				       int32_t font_height, sq_display_color_t color)
{
	test_rasterize_text_called = true;
	if (text != NULL) {
		strncpy(test_rasterize_text_buf, text, sizeof(test_rasterize_text_buf) - 1);
	}
	test_rasterize_text_x = x;
	test_rasterize_text_y = y;
}

void sq_display_backend_rasterize_rect(int32_t x, int32_t y, int32_t w, int32_t h,
				       sq_display_color_t fill, sq_display_color_t stroke)
{
	test_rasterize_rect_called = true;
	test_rasterize_rect_x = x;
	test_rasterize_rect_y = y;
}

int sq_display_backend_flush_framebuffer(enum sq_vm_runtime_display_refresh_mode mode)
{
	ARG_UNUSED(mode);
	test_display_flush_count++;
	if (test_display_flush_block) {
		k_sem_give(&test_display_flush_started);
		(void)k_sem_take(&test_display_flush_release, K_FOREVER);
	}
	return 0;
}

const uint8_t *sq_display_backend_framebuffer(void)
{
	return test_fb_framebuffer;
}

size_t sq_display_backend_framebuffer_size(void)
{
	return sizeof(test_fb_framebuffer);
}
```

- [ ] **Step 2: Update `test_vm_runtime_records_physical_display_clear_and_text_ops`**

Replace assertions that check `display_ops[]` with assertions that check rasterize mock calls:

```c
ZTEST(squidscript_protocol, test_vm_runtime_records_physical_display_clear_and_text_ops)
{
	static struct sq_vm_runtime runtime;
	const SqvmDisplayTextOptions text_options = {
		.x = 10,
		.y = 20,
		.font_height = 24,
		.text_color = SQ_DISPLAY_COLOR_WHITE,
		.background_color = SQ_DISPLAY_COLOR_UNSET,
	};

	test_rasterize_clear_called = false;
	test_rasterize_text_called = false;
	memset(&runtime, 0, sizeof(runtime));
	runtime_display_clear(&runtime, SQ_DISPLAY_COLOR_WHITE);
	runtime_display_text(&runtime, (const uint8_t *)"Hello", strlen("Hello"),
			     &text_options);

	zassert_equal(runtime.drawlog_count, 2);
	zassert_str_equal(runtime.drawlog[0], "draw=clear color=0");
	zassert_str_equal(runtime.drawlog[1], "draw=text text=\"Hello\" x=10 y=20");
	zassert_true(runtime.display_dirty);
	zassert_true(test_rasterize_clear_called);
	zassert_equal(test_rasterize_clear_color, SQ_DISPLAY_COLOR_WHITE);
	zassert_true(test_rasterize_text_called);
	zassert_str_equal(test_rasterize_text_buf, "Hello");
	zassert_equal(test_rasterize_text_x, 10);
	zassert_equal(test_rasterize_text_y, 20);
}
```

- [ ] **Step 3: Update `test_vm_runtime_records_physical_display_rect_ops`**

```c
ZTEST(squidscript_protocol, test_vm_runtime_records_physical_display_rect_ops)
{
	static struct sq_vm_runtime runtime;
	const SqvmDisplayRectOptions options = {
		.x = 18,
		.y = 76,
		.w = 424,
		.h = 48,
		.fill_color = SQ_DISPLAY_COLOR_UNSET,
		.stroke_color = SQ_DISPLAY_COLOR_BLACK,
	};

	test_rasterize_rect_called = false;
	memset(&runtime, 0, sizeof(runtime));
	runtime_display_rect(&runtime, &options);

	zassert_equal(runtime.drawlog_count, 1);
	zassert_str_equal(runtime.drawlog[0], "draw=rect x=18 y=76 w=424 h=48");
	zassert_true(runtime.display_dirty);
	zassert_true(test_rasterize_rect_called);
	zassert_equal(test_rasterize_rect_x, 18);
	zassert_equal(test_rasterize_rect_y, 76);
}
```

- [ ] **Step 4: Update `test_queued_input_dispatches_before_display_flush_finishes`**

This test checks that flush is non-blocking. The mock `sq_display_backend_flush_framebuffer` uses the same semaphore pattern, so the test logic stays similar. Update the mock setup calls.

- [ ] **Step 5: Remove or update `test_display_op_buffer_*` tests**

The `test_display_op_buffer_preserves_clear_for_library_like_screen` test is no longer relevant — there's no op buffer. Remove it.

- [ ] **Step 6: Update `test_vm_runtime_reset_clears_display_backend_previous_frame`**

This test calls `sq_display_backend_reset()` which is still in the interface. Keep it.

- [ ] **Step 7: Remove SSD1677 compositor tests that depend on ops**

Tests like `test_ssd1677_1bpp_compositor_draws_stroked_rect` operate on display ops. These need to be rewritten for framebuffer-based rendering, or removed if the compositor is removed.

- [ ] **Step 8: Verify tests compile and pass**

Run: `west test` or the equivalent test runner
Expected: All updated tests pass

- [ ] **Step 9: Commit**

```bash
git add firmware/zephyr/tests/protocol/src/main.c
git commit -m "tests: update protocol display tests for framebuffer pipeline"
```

---

## Task 7: Wire up SPI flush in SSD1677 driver

**Files:**
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`

- [ ] **Step 1: Implement `sq_display_backend_flush_framebuffer()` with real SPI transfer**

Replace the placeholder from Task 2 Step 6 with the actual SPI transfer logic. Reuse the existing `configure_display()`, `set_full_window()`, and SPI write functions from the old flush path.

The key change: instead of streaming row-by-row from the ops rasterizer, send the entire `fb_framebuffer` in one or more SPI transactions.

```c
int sq_display_backend_flush_framebuffer(enum sq_vm_runtime_display_refresh_mode mode)
{
	int ret;

	ret = configure_display();
	if (ret != 0) {
		return ret;
	}
	set_full_window();
	/* Send the framebuffer to the e-paper BW RAM */
	ret = write_command(SSD1677_CMD_WRITE_RAM);
	if (ret != 0) {
		return ret;
	}
	ret = write_data(fb_framebuffer, FB_FRAMEBUFFER_SIZE);
	if (ret != 0) {
		return ret;
	}
	/* Trigger display refresh based on mode */
	switch (mode) {
	case SQ_VM_RUNTIME_DISPLAY_REFRESH_FULL:
		ret = refresh_display();
		break;
	case SQ_VM_RUNTIME_DISPLAY_REFRESH_FAST_1BPP:
	case SQ_VM_RUNTIME_DISPLAY_REFRESH_AUTO:
	default:
		ret = refresh_display();
		break;
	}
	return ret;
}
```

- [ ] **Step 2: Remove old flush path functions**

Remove or ifdef-out:
- `stream_composed_1bpp_frame()`
- `render_row()`
- The old `sq_display_backend_flush()` implementation
- `previous_composed_ops[]`, `sorted_ops[]` statics
- Differential refresh state machines

Keep: `draw_text_row()`, `draw_rect_row()` if they're used by the binbook path. Otherwise remove.

- [ ] **Step 3: Verify compilation**

Run: build the firmware
Expected: Compiles cleanly

- [ ] **Step 4: Flash and test on hardware**

Run: `cargo run -p squidc -- target flash --target xteink-x4`
Then: `cargo run -p squidc -- app launch grid-cursor`
Expected: Display renders the grid cursor app correctly

- [ ] **Step 5: Commit**

```bash
git add firmware/zephyr/src/ssd1677_gdeq0426t82_display.c
git commit -m "display: implement SPI flush of framebuffer in SSD1677 driver"
```

---

## Task 8: Test with grid cursor app

**Files:**
- Test: `examples/app-tests/xteink/grid-cursor/main.squid`

- [ ] **Step 1: Run grid cursor app on hardware**

```bash
cargo run -p squidc -- hardware test --target xteink-x4
```

- [ ] **Step 2: Verify display renders correctly**

Check that the grid and cursor render on the e-paper display. The cursor should move when buttons are pressed.

- [ ] **Step 3: Measure display flush latency**

Use `device resources` to check `last_display_flush_us`. Compare with the previous op-based pipeline baseline (~1.9s for fast refresh).

- [ ] **Step 4: Commit test results**

```bash
git add examples/app-tests/xteink/grid-cursor/
git commit -m "test: verify grid cursor app with framebuffer display pipeline"
```

---

## Task 9: Update documentation

**Files:**
- Modify: `docs/runtime_limits.md`
- Modify: `docs/specs/2026-06-22-framebuffer-display-pipeline-design.md`

- [ ] **Step 1: Update `docs/runtime_limits.md`**

Remove the "Retained display ops per screen" row (line 36) since `SQ_VM_RUNTIME_DISPLAY_OP_MAX` is removed. Replace with a note about the framebuffer:

```markdown
| Framebuffer size | 48000 bytes | `FB_FRAMEBUFFER_SIZE` | yes (per target) |
```

- [ ] **Step 2: Update `docs/specs/2026-06-22-framebuffer-display-pipeline-design.md`**

Update the "Display Op Types" section to note that `sq_vm_runtime_display_op` struct and enums are kept temporarily for internal driver use but no longer exposed through the public API. Update the binbook section to reflect that binbook now draws to the framebuffer.

- [ ] **Step 3: Verify documentation consistency**

Check that all docs referencing the display pipeline are consistent with the new framebuffer architecture.

- [ ] **Step 4: Commit**

```bash
git add docs/runtime_limits.md docs/specs/2026-06-22-framebuffer-display-pipeline-design.md
git commit -m "docs: update display pipeline documentation for framebuffer architecture"
```
