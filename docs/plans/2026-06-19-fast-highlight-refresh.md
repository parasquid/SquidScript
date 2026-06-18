# Fast Highlight Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make XTEINK BinBook reader selection highlights use the SSD1677 fast partial path without full-refresh flashing, while preserving full-quality page reading refreshes.

**Architecture:** Keep SquidScript redraw-from-state semantics unchanged. Record `rect` as a real Zephyr display op, add a host-testable SSD1677 1bpp row compositor that can render previous and current draw streams, and use that compositor to feed SSD1677 previous/current RAM during `fast1bpp` refreshes. The BinBook reader opts into `fast1bpp` on selection screens only.

**Tech Stack:** Zephyr C firmware, SquidVM FFI display callbacks, SSD1677 e-paper backend, SquidScript reader app, Bash hardware wrapper, native Zephyr ztests.

---

## File Structure

- Modify: `firmware/zephyr/src/vm_runtime.h` - add rectangle fields to `sq_vm_runtime_display_op`.
- Modify: `firmware/zephyr/src/vm_runtime_display.c` - record `runtime_display_rect` as a physical display op.
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c` - add 1bpp op composition and previous/current fast refresh streaming.
- Modify: `firmware/zephyr/tests/protocol/src/main.c` - add ztests for rect op recording, compositor behavior, and refresh policy.
- Modify: `examples/binbook-reader/main.squid` - request `fast1bpp` on library/menu/chapter screens only.
- Modify: `examples/binbook-reader/README.md`, `docs/hardware_target_tests.md`, and `scripts/xteink-x4-test-binbook-reader.sh` - document and verify the changed refresh expectations.

## Task 1: Record Rectangles as Physical Display Ops

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime.h`
- Modify: `firmware/zephyr/src/vm_runtime_display.c`
- Test: `firmware/zephyr/tests/protocol/src/main.c`

- [ ] **Step 1: Write the failing ztest**

Add a ztest near `test_vm_runtime_records_physical_display_clear_and_text_ops`:

```c
ZTEST(squidscript_protocol, test_vm_runtime_records_physical_display_rect_ops)
{
	static struct sq_vm_runtime runtime;
	const SqvmDisplayRectOptions options = {
		.x = 18,
		.y = 76,
		.w = 424,
		.h = 48,
		.stroke_color = (const uint8_t *)"gray15",
		.stroke_color_len = strlen("gray15"),
	};

	memset(&runtime, 0, sizeof(runtime));
	runtime_display_rect(&runtime, &options);

	zassert_equal(runtime.drawlog_count, 1);
	zassert_str_equal(runtime.drawlog[0], "draw=rect x=18 y=76 w=424 h=48");
	zassert_true(runtime.display_dirty);
	zassert_equal(runtime.display_op_count, 1);
	zassert_equal(runtime.display_ops[0].kind, SQ_VM_RUNTIME_DISPLAY_OP_RECT);
	zassert_equal(runtime.display_ops[0].x, 18);
	zassert_equal(runtime.display_ops[0].y, 76);
	zassert_equal(runtime.display_ops[0].w, 424);
	zassert_equal(runtime.display_ops[0].h, 48);
	zassert_str_equal(runtime.display_ops[0].fill_color, "");
	zassert_str_equal(runtime.display_ops[0].stroke_color, "gray15");
}
```

- [ ] **Step 2: Run the failing test**

Run outside the sandbox per firmware tooling guidance:

```bash
scripts/zephyr-test-protocol.sh
```

Expected result: build fails because `SQ_VM_RUNTIME_DISPLAY_OP_RECT`, `w`, `h`,
`fill_color`, and `stroke_color` do not exist yet, or the test fails because no
rect op is appended.

- [ ] **Step 3: Implement the minimal runtime model**

In `firmware/zephyr/src/vm_runtime.h`, add the enum value and fields:

```c
enum sq_vm_runtime_display_op_kind {
	SQ_VM_RUNTIME_DISPLAY_OP_CLEAR = 0,
	SQ_VM_RUNTIME_DISPLAY_OP_TEXT,
	SQ_VM_RUNTIME_DISPLAY_OP_RECT,
	SQ_VM_RUNTIME_DISPLAY_OP_BINBOOK_DRAWABLE,
};

struct sq_vm_runtime_display_op {
	enum sq_vm_runtime_display_op_kind kind;
	char text[SQ_VM_RUNTIME_DISPLAY_TEXT_LEN];
	char fill_color[SQ_VM_RUNTIME_DISPLAY_TEXT_LEN];
	char stroke_color[SQ_VM_RUNTIME_DISPLAY_TEXT_LEN];
	int32_t x;
	int32_t y;
	int32_t w;
	int32_t h;
	int32_t font_height;
	struct sq_vm_runtime_binbook_page binbook_page;
};
```

In `runtime_display_rect`, append the op after logging:

```c
struct sq_vm_runtime *runtime = user_data;
struct sq_vm_runtime_display_op *op = runtime_display_append_op(runtime);
if (op != NULL) {
	op->kind = SQ_VM_RUNTIME_DISPLAY_OP_RECT;
	op->x = options->x;
	op->y = options->y;
	op->w = options->w;
	op->h = options->h;
	runtime_display_copy_text(op->fill_color, sizeof(op->fill_color),
				  options->fill_color, options->fill_color_len);
	runtime_display_copy_text(op->stroke_color, sizeof(op->stroke_color),
				  options->stroke_color, options->stroke_color_len);
}
```

- [ ] **Step 4: Verify task 1 passes**

Run:

```bash
scripts/zephyr-test-protocol.sh
```

Expected result: the new rect-op ztest passes and existing display-op ztests
still pass.

## Task 2: Add a Host-Testable 1bpp SSD1677 Compositor

**Files:**
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`
- Test: `firmware/zephyr/tests/protocol/src/main.c`

- [ ] **Step 1: Write compositor ztests**

Add ztests that call non-static test helper functions exposed from the display
backend when building the protocol tests:

```c
ZTEST(squidscript_protocol, test_ssd1677_1bpp_compositor_draws_stroked_rect)
{
	uint8_t line[100] = {0};
	const struct sq_vm_runtime_display_op ops[] = {
		{ .kind = SQ_VM_RUNTIME_DISPLAY_OP_CLEAR, .text = "white" },
		{
			.kind = SQ_VM_RUNTIME_DISPLAY_OP_RECT,
			.x = 8,
			.y = 4,
			.w = 16,
			.h = 6,
			.stroke_color = "gray15",
		},
	};

	sq_ssd1677_test_render_1bpp_row(line, sizeof(line), 4, ops, ARRAY_SIZE(ops));

	zassert_true(sq_ssd1677_test_row_has_black_pixel(line, sizeof(line), 8));
	zassert_true(sq_ssd1677_test_row_has_black_pixel(line, sizeof(line), 23));
	zassert_false(sq_ssd1677_test_row_has_black_pixel(line, sizeof(line), 24));
}

ZTEST(squidscript_protocol, test_ssd1677_1bpp_compositor_moves_highlight_between_frames)
{
	uint8_t previous[100] = {0};
	uint8_t current[100] = {0};
	const struct sq_vm_runtime_display_op old_ops[] = {
		{ .kind = SQ_VM_RUNTIME_DISPLAY_OP_CLEAR, .text = "white" },
		{ .kind = SQ_VM_RUNTIME_DISPLAY_OP_RECT, .x = 8, .y = 4, .w = 16, .h = 6,
		  .stroke_color = "gray15" },
	};
	const struct sq_vm_runtime_display_op new_ops[] = {
		{ .kind = SQ_VM_RUNTIME_DISPLAY_OP_CLEAR, .text = "white" },
		{ .kind = SQ_VM_RUNTIME_DISPLAY_OP_RECT, .x = 8, .y = 20, .w = 16, .h = 6,
		  .stroke_color = "gray15" },
	};

	sq_ssd1677_test_render_1bpp_row(previous, sizeof(previous), 4, old_ops,
				       ARRAY_SIZE(old_ops));
	sq_ssd1677_test_render_1bpp_row(current, sizeof(current), 4, new_ops,
				       ARRAY_SIZE(new_ops));

	zassert_true(sq_ssd1677_test_row_has_black_pixel(previous, sizeof(previous), 8));
	zassert_false(sq_ssd1677_test_row_has_black_pixel(current, sizeof(current), 8));
}
```

- [ ] **Step 2: Run the failing tests**

Run:

```bash
scripts/zephyr-test-protocol.sh
```

Expected result: build fails because the test helper functions do not exist.

- [ ] **Step 3: Implement row composition**

In the SSD1677 backend, add helpers that:

- initialize each row from the latest `CLEAR` op before or during source-order
  composition
- apply `RECT` fill and stroke by mapping logical coordinates through
  `logical_to_physical`
- apply existing text rendering in source order
- keep BinBook drawable row decoding available to the fast path

Expose these only in test builds:

```c
#if defined(CONFIG_ZTEST)
void sq_ssd1677_test_render_1bpp_row(uint8_t *line, size_t line_len, uint16_t physical_y,
				     const struct sq_vm_runtime_display_op *ops,
				     size_t op_count);
bool sq_ssd1677_test_row_has_black_pixel(const uint8_t *line, size_t line_len,
					 uint16_t physical_x);
#endif
```

- [ ] **Step 4: Verify task 2 passes**

Run:

```bash
scripts/zephyr-test-protocol.sh
```

Expected result: compositor tests pass.

## Task 3: Stream Previous and Current Composed Frames for Fast Refresh

**Files:**
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`
- Test: `firmware/zephyr/tests/protocol/src/main.c`

- [ ] **Step 1: Write refresh-policy ztests**

Add tests that drive the backend policy helpers without requiring a physical
display:

```c
ZTEST(squidscript_protocol, test_ssd1677_fast_composed_refresh_requires_previous_frame)
{
	struct sq_ssd1677_composed_refresh_state state = {0};

	zassert_equal(sq_ssd1677_composed_refresh_decide(&state,
		      SQ_VM_RUNTIME_DISPLAY_REFRESH_FAST_1BPP),
		      SQ_SSD1677_COMPOSED_REFRESH_FULL_SEED);

	sq_ssd1677_composed_refresh_record(&state, SQ_SSD1677_COMPOSED_REFRESH_FULL_SEED);
	zassert_equal(sq_ssd1677_composed_refresh_decide(&state,
		      SQ_VM_RUNTIME_DISPLAY_REFRESH_FAST_1BPP),
		      SQ_SSD1677_COMPOSED_REFRESH_BW_DIFFERENTIAL_PARTIAL);
}

ZTEST(squidscript_protocol, test_ssd1677_failed_composed_refresh_does_not_replace_previous_frame)
{
	struct sq_ssd1677_composed_refresh_state state = {0};

	sq_ssd1677_composed_refresh_record(&state, SQ_SSD1677_COMPOSED_REFRESH_FULL_SEED);
	zassert_true(state.previous_ops_valid);
	sq_ssd1677_composed_refresh_reset(&state);
	zassert_false(state.previous_ops_valid);
}
```

- [ ] **Step 2: Run the failing tests**

Run:

```bash
scripts/zephyr-test-protocol.sh
```

Expected result: build fails because the composed refresh state and helpers do
not exist.

- [ ] **Step 3: Implement retained composed state**

Add a static retained previous-op buffer in the SSD1677 backend:

```c
static struct sq_vm_runtime_display_op previous_composed_ops[SQ_VM_RUNTIME_DISPLAY_OP_MAX];
static size_t previous_composed_op_count;
static bool previous_composed_ops_valid;
```

For `SQ_VM_RUNTIME_DISPLAY_REFRESH_FAST_1BPP` on non-page-only or mixed op
streams:

- if `previous_composed_ops_valid` is false, render the current ops with the
  full black/white refresh path and copy current ops into `previous_composed_ops`
  after success
- if valid, stream `previous_composed_ops` rows to `SSD1677_CMD_WRITE_RED_RAM`
  and current ops rows to `SSD1677_CMD_WRITE_RAM`, then call
  `refresh_partial_display`
- copy current ops into `previous_composed_ops` only after the refresh succeeds

Keep the existing page-only BinBook differential path available for pure
BinBook page turns. For mixed streams containing a BinBook drawable plus
overlays, use the composed fast path so overlay commands are not dropped.

- [ ] **Step 4: Verify task 3 passes**

Run:

```bash
scripts/zephyr-test-protocol.sh
```

Expected result: refresh-policy tests pass and existing BinBook refresh-policy
tests still pass.

## Task 4: Use Fast Refresh on Reader Selection Screens

**Files:**
- Modify: `examples/binbook-reader/main.squid`
- Modify: `examples/binbook-reader/README.md`

- [ ] **Step 1: Update the reader app**

In `screen("library")`, replace:

```squid
service.display.refreshMode("full")
```

with:

```squid
service.display.refreshMode("fast1bpp")
```

In `screen("menu")`, replace:

```squid
service.display.refreshMode("full")
```

with:

```squid
service.display.refreshMode("fast1bpp")
```

In `screen("chapters")`, replace:

```squid
service.display.refreshMode("full")
```

with:

```squid
service.display.refreshMode("fast1bpp")
```

Leave `screen("reader")` on its existing `service.display.refreshMode("full")`
line.

- [ ] **Step 2: Update the README**

Replace the current paragraph that says the reader requests full refresh for
clean 4-gray output with:

```markdown
The reader keeps the page screen on the full GRAY2 refresh path for clean book
content. Library, menu, and chapter screens request `fast1bpp` so selection
highlights can move through the SSD1677 differential partial path. The firmware
still redraws these screens from app state; it retains previous composed frame
state only as a private display optimization.
```

- [ ] **Step 3: Verify app build**

Run:

```bash
cargo run -p squidc -- app build examples/binbook-reader/main.squid --out target/binbook-reader.sqbc
```

Expected result: app build succeeds.

## Task 5: Update Hardware Script Expectations

**Files:**
- Modify: `scripts/xteink-x4-test-binbook-reader.sh`
- Modify: `scripts/tests/test_zephyr_hardware_suite.py`
- Modify: `docs/hardware_target_tests.md`

- [ ] **Step 1: Update the hardware script**

Keep the existing reader-page assertion:

```bash
assert_file_contains "${reader_drawlog_out}" "draw=binbook"
assert_file_contains "${reader_drawlog_out}" "mode=full"
```

After menu navigation, capture drawlog again and assert fast selection refresh:

```bash
selection_drawlog_out="$(run_capture drawlog-selection cargo run --quiet -p squidc -- device drawlog --port "${PORT}")"
assert_file_contains "${selection_drawlog_out}" "draw=refresh mode=fast1bpp"
```

- [ ] **Step 2: Update static script tests**

In `scripts/tests/test_zephyr_hardware_suite.py`, keep the `"mode=full"`
assertion and add:

```python
self.assertIn("drawlog-selection", script)
self.assertIn("mode=fast1bpp", script)
```

- [ ] **Step 3: Update hardware docs**

In `docs/hardware_target_tests.md`, update the BinBook reader paragraph to say
the script verifies full refresh for the reader page, fast refresh requests for
selection screens, empty `device errors`, and resource metrics.

- [ ] **Step 4: Verify script checks**

Run:

```bash
bash -n scripts/xteink-x4-test-binbook-reader.sh
python3 -m pytest scripts/tests/test_zephyr_hardware_suite.py -k binbook_reader
```

Expected result: both commands pass.

## Task 6: Final Verification and Roadmap Handling

**Files:**
- Read: `ROADMAP.md`
- Modify only if the user confirms the wording for any replacement validation item.

- [ ] **Step 1: Run host verification**

Run:

```bash
scripts/zephyr-test-protocol.sh
cargo run -p squidc -- app test examples/app-tests/xteink/binbook-reader-selection
cargo run -p squidc -- app build examples/binbook-reader/main.squid --out target/binbook-reader.sqbc
bash -n scripts/xteink-x4-test-binbook-reader.sh
python3 -m pytest scripts/tests/test_zephyr_hardware_suite.py -k binbook_reader
```

Expected result: all host checks pass.

- [ ] **Step 2: Run hardware serial verification when a target is available**

Run outside the sandbox and never in parallel with another hardware command:

```bash
scripts/xteink-x4-test-binbook-reader.sh
```

Expected result: the script prints:

```text
OK XTEINK X4 BinBook reader selection hardware check passed
```

- [ ] **Step 3: Preserve the optical follow-up**

Because visual ghosting/flashing validation is deferred, do not silently remove
the roadmap item as completed. If the serial implementation is done before the
interactive visual check, propose replacing the roadmap entry with:

```markdown
- Validate the XTEINK BinBook reader fast highlight refresh path interactively
  on hardware: move library/menu/chapter highlights repeatedly and confirm the
  SSD1677 fast partial path avoids full-refresh flashing and unacceptable
  ghosting.
```

Wait for user confirmation before editing `ROADMAP.md`.
