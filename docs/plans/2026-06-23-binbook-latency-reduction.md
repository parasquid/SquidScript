# BinBook Latency Reduction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce binbook page-turn latency from ~1444ms to under 200ms (excluding hardware display refresh) by buffering compressed page data and reusing the binbook file handle.

**Architecture:** Two independent firmware changes: (A) a heap-allocated compressed page buffer in `vm_runtime_binbook.c` that eliminates byte-by-byte file reads during decompression, and (B) a module-owned reusable `fs_file_t` for binbook operations that eliminates repeated file open/close cycles. Both follow established patterns in the codebase (SQBC file handle reuse, existing packbits reader).

**Tech Stack:** Zephyr C17, Zephyr filesystem API, ztest/Twister, Rust `squidc` host CLI, XTEINK X4 ESP32-C3 hardware.

---

### Task 1: Add failing tests for binbook file handle reuse

**Files:**
- Modify: `firmware/zephyr/tests/protocol/src/main.c`
- Modify: `firmware/zephyr/src/vm_runtime_internal.h`

- [ ] **Step 1: Add drain function declarations**

In `firmware/zephyr/src/vm_runtime_internal.h`, after the existing `runtime_binbook_validate_path` declaration (line ~151), add:

```c
uint64_t sq_vm_runtime_binbook_drain_open_us(void);
uint64_t sq_vm_runtime_binbook_drain_read_page_us(void);
```

- [ ] **Step 2: Add failing handle-reuse test**

In `firmware/zephyr/tests/protocol/src/main.c`, add a new test that opens a binbook twice through the runtime and asserts only one filesystem open:

```c
ZTEST(squidscript_protocol, test_binbook_open_reuses_file_handle)
{
	static struct sq_vm_runtime runtime;
	const uint8_t path[] = "content:books/r/test-timing.binbook";
	SqvmBinBookOpenResult open1 = {0};
	SqvmBinBookOpenResult open2 = {0};

	memset(&runtime, 0, sizeof(runtime));
	runtime.binbook.active = false;

	/* First open should succeed and open a file handle */
	int result1 = runtime_binbook_open(&runtime, path, sizeof(path) - 1, &open1);

	zassert_equal(result1, 0);
	zassert_true(open1.ok, "first binbook.open should succeed");

	size_t open_count_after_first = test_binbook_open_count();

	/* Second open with same path should reuse the handle */
	int result2 = runtime_binbook_open(&runtime, path, sizeof(path) - 1, &open2);

	zassert_equal(result2, 0);
	zassert_true(open2.ok, "second binbook.open should succeed");
	zassert_equal(test_binbook_open_count(), open_count_after_first,
		      "second open should reuse handle, not open new file");
}
```

Note: `test_binbook_open_count()` does not exist yet — the test will fail to compile. This is the expected RED state.

- [ ] **Step 3: Run protocol ztests and verify RED**

Run: `scripts/zephyr-test-protocol.sh`

Expected: Compilation fails because `test_binbook_open_count` is undefined. The fresh pre-change baseline is 144/144 passing tests.

- [ ] **Step 4: Commit the failing test skeleton**

```bash
git add firmware/zephyr/tests/protocol/src/main.c \
  firmware/zephyr/src/vm_runtime_internal.h
git commit -m "test(firmware): add failing binbook handle-reuse test skeleton"
```

### Task 2: Implement binbook file handle reuse

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime_binbook.c`
- Modify: `firmware/zephyr/src/vm_runtime_internal.h`
- Modify: `firmware/zephyr/tests/protocol/src/main.c`

- [ ] **Step 1: Add the module-owned handle state**

In `firmware/zephyr/src/vm_runtime_binbook.c`, after the existing static accumulator variables (around line 30), add:

```c
static struct {
	struct fs_file_t file;
	bool is_open;
	char path[SQ_APP_STORE_PATH_MAX];
} binbook_open_file;

static size_t binbook_file_open_count;

size_t test_binbook_open_count(void)
{
	return binbook_file_open_count;
}

static void binbook_open_file_close(void)
{
	if (binbook_open_file.is_open) {
		(void)fs_close(&binbook_open_file.file);
		binbook_open_file.is_open = false;
	}
	binbook_open_file.path[0] = '\0';
}

int sq_vm_runtime_binbook_release(void)
{
	binbook_open_file_close();
	return 0;
}
```

- [ ] **Step 2: Add declarations to internal header**

In `firmware/zephyr/src/vm_runtime_internal.h`, after the drain declarations, add:

```c
int sq_vm_runtime_binbook_release(void);
size_t test_binbook_open_count(void);
```

- [ ] **Step 3: Instrument the test mock**

In `firmware/zephyr/tests/protocol/src/main.c`, the test needs a way to observe opens. Since the test already has a real filesystem via `write_test_file`, we need to add a counter. Add a static counter and a getter:

```c
static size_t test_binbook_file_open_count;

size_t test_binbook_open_count(void)
{
	return test_binbook_file_open_count;
}
```

Reset it in `test_binbook_open_reuses_file_handle` before the first open.

- [ ] **Step 4: Run tests and verify GREEN**

Run: `scripts/zephyr-test-protocol.sh`

Expected: The new test passes and all 144/144 protocol tests pass. The handle reuse works because `runtime_binbook_open()` now checks `binbook_open_file.path` before opening.

- [ ] **Step 5: Commit handle reuse implementation**

```bash
git add firmware/zephyr/src/vm_runtime_binbook.c \
  firmware/zephyr/src/vm_runtime_internal.h \
  firmware/zephyr/tests/protocol/src/main.c
git commit -m "perf(firmware): reuse binbook file handle across reads"
```

### Task 3: Add lifecycle invalidation for binbook handle

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime_binbook.c`
- Modify: `firmware/zephyr/src/device_protocol.c`
- Test: `firmware/zephyr/tests/protocol/src/main.c`

- [ ] **Step 1: Add failing release test**

Add a test that opens a binbook, calls release, then opens again and asserts a new file open:

```c
ZTEST(squidscript_protocol, test_binbook_release_closes_handle)
{
	static struct sq_vm_runtime runtime;
	const uint8_t path[] = "content:books/r/test-timing.binbook";
	SqvmBinBookOpenResult open1 = {0};
	SqvmBinBookOpenResult open2 = {0};

	memset(&runtime, 0, sizeof(runtime));
	runtime.binbook.active = false;

	int result1 = runtime_binbook_open(&runtime, path, sizeof(path) - 1, &open1);
	zassert_equal(result1, 0);
	zassert_true(open1.ok);

	size_t count_before_release = test_binbook_open_count();

	/* Release should close the handle */
	zassert_equal(sq_vm_runtime_binbook_release(), 0);

	/* Next open should require a new file open */
	int result2 = runtime_binbook_open(&runtime, path, sizeof(path) - 1, &open2);
	zassert_equal(result2, 0);
	zassert_true(open2.ok);
	zassert_equal(test_binbook_open_count(), count_before_release + 1,
		      "open after release must open a new file handle");
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `scripts/zephyr-test-protocol.sh`

Expected: The release test fails because `sq_vm_runtime_binbook_release()` does not exist yet.

- [ ] **Step 3: Wire release into lifecycle boundaries**

In `firmware/zephyr/src/device_protocol.c`, add a call to `sq_vm_runtime_binbook_release()` in the `release_foreground_storage()` helper (after the SQBC storage release). Also call it in `sq_app_store_vm_storage_for_app` before path replacement.

- [ ] **Step 4: Run tests and verify GREEN**

Run: `scripts/zephyr-test-protocol.sh`

Expected: All 144/144 tests pass including the new release test.

- [ ] **Step 5: Commit lifecycle invalidation**

```bash
git add firmware/zephyr/src/vm_runtime_binbook.c \
  firmware/zephyr/src/device_protocol.c \
  firmware/zephyr/tests/protocol/src/main.c
git commit -m "fix(firmware): invalidate binbook handle at lifecycle boundaries"
```

### Task 4: Instrument binbook decompression timing

**Files:**
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`

- [ ] **Step 1: Add debug_log timing markers**

In `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`, wrap each decompression pass in `sq_display_backend_rasterize_binbook()` with `sq_debug_log_append` markers:

```c
sq_debug_log_append("%lld:decompress_start:cs=%lu", (long long)k_uptime_get(),
                    (unsigned long)page->compressed_size);
(void)decompress_binbook_gray2_to_fb(page, true);
sq_debug_log_append("%lld:decompress_msb_done", (long long)k_uptime_get());
(void)decompress_binbook_gray2_to_fb(page, false);
sq_debug_log_append("%lld:decompress_lsb_done", (long long)k_uptime_get());
(void)decompress_binbook_gray2_bw_to_fb(page, true);
sq_debug_log_append("%lld:decompress_bw_done", (long long)k_uptime_get());
```

- [ ] **Step 2: Build, flash, and measure baseline**

```bash
cargo run -p squidc -- target build --target xteink-x4
cargo run -p squidc -- target flash --target xteink-x4
cargo run -p squidc -- app install examples/binbook-pager/main.squid
cargo run -p squidc -- app launch binbook-pager
```

Wait for launch, send LEFT then RIGHT, capture debug log:

```bash
cargo run -p squidc -- device key LEFT
sleep 5
cargo run -p squidc -- device key RIGHT
sleep 2
cargo run -p squidc -- device debug-log | grep decompress
```

Record baseline decompression times per plane.

- [ ] **Step 3: Commit instrumentation**

```bash
git add firmware/zephyr/src/ssd1677_gdeq0426t82_display.c
git commit -m "diagnostic(firmware): add decompression timing markers"
```

### Task 5: Add failing tests for compressed page buffer

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime_binbook.c`
- Modify: `firmware/zephyr/src/vm_runtime.h`
- Test: `firmware/zephyr/tests/protocol/src/main.c`

- [ ] **Step 1: Add buffer state to runtime struct**

In `firmware/zephyr/src/vm_runtime.h`, add to the `sq_vm_runtime_binbook_page` struct (after `stored_height`):

```c
const uint8_t *compressed_data;
uint32_t compressed_data_len;
```

- [ ] **Step 2: Add failing buffer allocation test**

Add a test that opens a binbook, reads a page, and asserts the buffer is populated:

```c
ZTEST(squidscript_protocol, test_binbook_read_page_allocates_buffer)
{
	static struct sq_vm_runtime runtime;
	const uint8_t path[] = "content:books/r/test-timing.binbook";
	SqvmBinBookOpenResult opened = {0};

	memset(&runtime, 0, sizeof(runtime));
	runtime.binbook.active = false;

	int result = runtime_binbook_open(&runtime, path, sizeof(path) - 1, &opened);
	zassert_equal(result, 0);
	zassert_true(opened.ok);

	SqvmBinBookReadPageResult page = {0};
	result = runtime_binbook_read_page(&runtime, opened.book, 0, &page);
	zassert_equal(result, 0);
	zassert_true(page.ok, "readPage should succeed");

	/* The compressed buffer should be allocated */
	zassert_not_null(runtime.drawable.page.compressed_data,
			 "compressed buffer should be allocated");
	zassert_true(runtime.drawable.page.compressed_data_len > 0,
		     "compressed buffer should have data");

	/* Clean up */
	sq_vm_runtime_binbook_release();
}
```

- [ ] **Step 3: Run tests and verify RED**

Run: `scripts/zephyr-test-protocol.sh`

Expected: The buffer test fails because `compressed_data` is always NULL (buffer not allocated yet).

- [ ] **Step 4: Commit failing test**

```bash
git add firmware/zephyr/src/vm_runtime.h \
  firmware/zephyr/tests/protocol/src/main.c
git commit -m "test(firmware): add failing compressed buffer allocation test"
```

### Task 6: Implement compressed page buffer

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime_binbook.c`
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`

- [ ] **Step 1: Allocate buffer in runtime_binbook_read_page()**

In `firmware/zephyr/src/vm_runtime_binbook.c`, in `runtime_binbook_read_page()`, after parsing the page metadata and before storing the drawable, add buffer allocation:

```c
/* Allocate compressed data buffer */
void *buf = k_malloc(meta.compressed_size);
if (buf != NULL) {
    /* Reopen file to read compressed blob */
    struct fs_file_t data_file;
    fs_file_t_init(&data_file);
    if (fs_open(&data_file, runtime->binbook.path, FS_O_READ) == 0) {
        int read_result = binbook_read_exact(&data_file,
                                              runtime->binbook.page_data_offset + meta.blob_offset,
                                              (uint8_t *)buf, meta.compressed_size);
        (void)fs_close(&data_file);
        if (read_result == 0) {
            runtime->drawable.page.compressed_data = buf;
            runtime->drawable.page.compressed_data_len = meta.compressed_size;
        } else {
            k_free(buf);
        }
    } else {
        k_free(buf);
    }
}
```

- [ ] **Step 2: Update display backend to use buffer**

In `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`, modify `decompress_binbook_gray2_to_fb()` to accept an optional buffer parameter:

```c
static int decompress_binbook_gray2_to_fb(const struct sq_vm_runtime_binbook_page *page,
                                          bool msb_plane)
{
    struct packbits_reader reader = {0};
    int ret;

    if (page->compressed_data != NULL && page->compressed_data_len > 0) {
        /* Buffer-backed mode */
        reader.buf = page->compressed_data;
        reader.buf_len = page->compressed_data_len;
        reader.buf_pos = 0;
    } else {
        /* File-backed fallback */
        fs_file_t_init(&reader.file);
        ret = fs_open(&reader.file, page->path, FS_O_READ);
        if (ret != 0) {
            return ret;
        }
        ret = fs_seek(&reader.file, (off_t)page->blob_offset, FS_SEEK_SET);
        if (ret != 0) {
            (void)fs_close(&reader.file);
            return ret;
        }
    }
    reader.compressed_left = page->compressed_size;
    /* ... rest of decompression loop unchanged ... */
}
```

- [ ] **Step 3: Update packbits_read_raw for buffer mode**

In `packbits_read_raw()`, add buffer-backed path:

```c
static int packbits_read_raw(struct packbits_reader *reader, uint8_t *out)
{
    if (reader->compressed_left == 0) {
        return -EIO;
    }
    if (reader->buf != NULL) {
        /* Buffer-backed: fast path */
        if (reader->buf_pos >= reader->buf_len) {
            return -EIO;
        }
        *out = reader->buf[reader->buf_pos++];
        reader->compressed_left--;
        return 0;
    }
    /* File-backed: slow path */
    ssize_t read = fs_read(&reader->file, out, 1);
    if (read < 0) {
        return (int)read;
    }
    if (read != 1) {
        return -EIO;
    }
    reader->compressed_left--;
    return 0;
}
```

- [ ] **Step 4: Free buffer after decompression**

In `sq_display_backend_rasterize_binbook()`, after all three decompression passes complete, free the buffer:

```c
/* Free compressed buffer if allocated */
if ((void *)page->compressed_data != NULL) {
    k_free((void *)page->compressed_data);
}
```

Note: This requires casting away const since `k_free` takes `void *`. The buffer was allocated with `k_malloc` in the runtime layer.

- [ ] **Step 5: Run tests and verify GREEN**

Run: `scripts/zephyr-test-protocol.sh`

Expected: All 144/144 tests pass. The buffer test now passes because `runtime_binbook_read_page()` allocates the buffer.

- [ ] **Step 6: Commit compressed buffer implementation**

```bash
git add firmware/zephyr/src/vm_runtime_binbook.c \
  firmware/zephyr/src/ssd1677_gdeq0426t82_display.c
git commit -m "perf(firmware): buffer compressed binbook page data for decompression"
```

### Task 7: Add allocation-failure fallback test

**Files:**
- Test: `firmware/zephyr/tests/protocol/src/main.c`

- [ ] **Step 1: Add failing fallback test**

Add a test that exhausts the heap, reads a page, and asserts the read still succeeds (via file-backed fallback):

```c
ZTEST(squidscript_protocol, test_binbook_read_page_falls_back_on_alloc_failure)
{
	static struct sq_vm_runtime runtime;
	const uint8_t path[] = "content:books/r/test-timing.binbook";
	SqvmBinBookOpenResult opened = {0};

	memset(&runtime, 0, sizeof(runtime));
	runtime.binbook.active = false;

	/* Exhaust heap */
	void *allocations[16];
	for (int i = 0; i < 16; i++) {
		allocations[i] = k_malloc(512);
		if (allocations[i] == NULL) {
			break;
		}
	}

	int result = runtime_binbook_open(&runtime, path, sizeof(path) - 1, &opened);
	zassert_equal(result, 0);
	zassert_true(opened.ok);

	SqvmBinBookReadPageResult page = {0};
	result = runtime_binbook_read_page(&runtime, opened.book, 0, &page);
	zassert_equal(result, 0);
	zassert_true(page.ok, "readPage should succeed even with no heap");

	/* Buffer should be NULL (allocation failed) */
	zassert_null(runtime.drawable.page.compressed_data,
		     "compressed buffer should be NULL when heap exhausted");

	/* Free test allocations */
	for (int i = 0; i < 16; i++) {
		if (allocations[i] != NULL) {
			k_free(allocations[i]);
		}
	}

	sq_vm_runtime_binbook_release();
}
```

- [ ] **Step 2: Run tests and verify GREEN**

Run: `scripts/zephyr-test-protocol.sh`

Expected: The fallback test passes because `k_malloc` failure leaves `compressed_data` as NULL, and the display backend falls back to file-backed reads.

- [ ] **Step 3: Commit fallback test**

```bash
git add firmware/zephyr/tests/protocol/src/main.c
git commit -m "test(firmware): verify compressed buffer allocation failure fallback"
```

### Task 8: Hardware verification and latency measurement

**Files:**
- Existing: `firmware/zephyr/src/vm_runtime_binbook.c`
- Existing: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`

- [ ] **Step 1: Build and flash**

```bash
cargo run -p squidc -- target build --target xteink-x4
cargo run -p squidc -- target flash --target xteink-x4
```

- [ ] **Step 2: Install and launch binbook-pager**

```bash
cargo run -p squidc -- app install examples/binbook-pager/main.squid
cargo run -p squidc -- app launch binbook-pager
```

- [ ] **Step 3: Measure page turn latency**

Send LEFT then RIGHT, measure timing:

```bash
cargo run -p squidc -- device key LEFT
sleep 5
cargo run -p squidc -- device key RIGHT
sleep 5
cargo run -p squidc -- device resources | \
  grep -E 'dispatch_us|binbook_open|binbook_read_page|display_flush'
```

- [ ] **Step 4: Compare against acceptance criteria**

| Metric | Before | Target | Actual |
|--------|--------|--------|--------|
| `last_binbook_open_us` | ~199ms | <10ms | |
| `last_binbook_read_page_us` | ~82ms | <10ms | |
| Decompress time | ~636ms | <50ms | |
| Total dispatch | ~1444ms | <200ms | |

Record actual values. If any metric misses target, investigate and fix.

- [ ] **Step 5: Capture decompression timing from debug log**

```bash
cargo run -p squidc -- device debug-log | grep decompress
```

Verify per-plane decompression times dropped from ~200ms to under 50ms.

- [ ] **Step 6: Commit verified results**

```bash
git add -A
git commit -m "perf(firmware): verify binbook latency reduction on hardware"
```

### Task 9: Clean up diagnostic instrumentation

**Files:**
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`

- [ ] **Step 1: Remove decompression timing markers**

Remove the `sq_debug_log_append` calls added in Task 4 from `sq_display_backend_rasterize_binbook()`. The markers were for measurement; the metrics in `device resources` now cover ongoing monitoring.

- [ ] **Step 2: Remove test-only functions if no longer needed**

Check if `test_binbook_open_count()` is still needed. If only used in tests, keep it behind a `#ifdef CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC` guard.

- [ ] **Step 3: Run tests and verify GREEN**

Run: `scripts/zephyr-test-protocol.sh`

Expected: All 144/144 tests pass.

- [ ] **Step 4: Commit cleanup**

```bash
git add firmware/zephyr/src/ssd1677_gdeq0426t82_display.c
git commit -m "chore(firmware): remove temporary decompression timing markers"
```
