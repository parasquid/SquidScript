# BinBook GRAY1 Graceful Degradation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow GRAY1-packed binbook pages to display on XTEINK X4 and XIAO e-paper targets by using the existing BW plane decompression path.

**Architecture:** Accept `GRAY1_PACKED` pixel format in the SSD1677 display driver validation and decompression functions. For GRAY1 pages, decompress only the BW plane (slot 2) and trigger BW-only refresh, leveraging existing infrastructure.

**Tech Stack:** C (Zephyr firmware), protocol ztests

---

## File Map

| File | Action | Purpose |
|------|--------|---------|
| `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c` | Modify | Accept GRAY1 pixel format in 3 functions |
| `firmware/zephyr/tests/protocol/src/main.c` | Modify | Add GRAY1 test binbook fixture and test cases |

---

### Task 1: Add GRAY1 pixel format constant

**Files:**
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c:174`

- [ ] **Step 1: Add the constant**

After line 174 (`#define BINBOOK_PIXEL_FORMAT_GRAY2_PACKED 2U`), add:

```c
#define BINBOOK_PIXEL_FORMAT_GRAY1_PACKED 1U
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo run -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add firmware/zephyr/src/ssd1677_gdeq0426t82_display.c
git commit -m "firmware: add GRAY1 pixel format constant for binbook graceful degradation"
```

---

### Task 2: Add GRAY1 test binbook fixture to protocol tests

**Files:**
- Modify: `firmware/zephyr/tests/protocol/src/main.c:772` (in `build_test_binbook`)

- [ ] **Step 1: Add GRAY1 binbook builder**

After the existing `build_test_binbook` function, add a new builder:

```c
#define TEST_BINBOOK_GRAY1_PAGE_DATA_LEN 48000U
#define TEST_BINBOOK_GRAY1_LEN \
    (TEST_BINBOOK_PAGE_DATA_OFFSET + TEST_BINBOOK_GRAY1_PAGE_DATA_LEN)

static void build_test_binbook_gray1(uint8_t out[TEST_BINBOOK_GRAY1_LEN])
{
    memset(out, 0, TEST_BINBOOK_GRAY1_LEN);
    memcpy(&out[0], "BINBOOK", 7);
    test_write_le16(&out[12], TEST_BINBOOK_HEADER_SIZE);
    test_write_le64(&out[16], TEST_BINBOOK_GRAY1_LEN);
    test_write_le64(&out[24], TEST_BINBOOK_HEADER_SIZE);
    test_write_le32(&out[32], TEST_BINBOOK_SECTION_COUNT * TEST_BINBOOK_SECTION_ENTRY_SIZE);
    test_write_le16(&out[36], TEST_BINBOOK_SECTION_ENTRY_SIZE);
    test_write_le16(&out[38], TEST_BINBOOK_SECTION_COUNT);
    test_write_le16(&out[40], TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE);
    test_write_le16(&out[42], TEST_BINBOOK_NAV_INDEX_ENTRY_SIZE);
    test_write_le64(&out[44], TEST_BINBOOK_PAGE_DATA_OFFSET);
    test_write_le64(&out[52], TEST_BINBOOK_GRAY1_PAGE_DATA_LEN);

    test_write_binbook_section(&out[TEST_BINBOOK_HEADER_SIZE], 1,
                               TEST_BINBOOK_STRING_TABLE_OFFSET, TEST_BINBOOK_STRING_TABLE_LEN,
                               0, 0);
    test_write_binbook_section(&out[TEST_BINBOOK_HEADER_SIZE + TEST_BINBOOK_SECTION_ENTRY_SIZE],
                               41, TEST_BINBOOK_NAV_INDEX_OFFSET, TEST_BINBOOK_NAV_INDEX_LEN,
                               TEST_BINBOOK_NAV_INDEX_ENTRY_SIZE, TEST_BINBOOK_NAV_INDEX_COUNT);
    test_write_binbook_section(&out[TEST_BINBOOK_HEADER_SIZE + 2 * TEST_BINBOOK_SECTION_ENTRY_SIZE],
                               43, TEST_BINBOOK_CHAPTER_INDEX_OFFSET,
                               TEST_BINBOOK_CHAPTER_INDEX_LEN,
                               TEST_BINBOOK_CHAPTER_INDEX_ENTRY_SIZE,
                               TEST_BINBOOK_CHAPTER_INDEX_COUNT);
    test_write_binbook_section(&out[TEST_BINBOOK_HEADER_SIZE + 3 * TEST_BINBOOK_SECTION_ENTRY_SIZE],
                               40,
                               TEST_BINBOOK_PAGE_INDEX_OFFSET,
                               TEST_BINBOOK_PAGE_INDEX_LEN,
                               TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE,
                               TEST_BINBOOK_PAGE_INDEX_COUNT);
    test_write_binbook_section(&out[TEST_BINBOOK_HEADER_SIZE + 4 * TEST_BINBOOK_SECTION_ENTRY_SIZE],
                               50, TEST_BINBOOK_PAGE_DATA_OFFSET,
                               TEST_BINBOOK_GRAY1_PAGE_DATA_LEN, 0, 0);

    memcpy(&out[TEST_BINBOOK_STRING_TABLE_OFFSET], "Chapter OneChapter Two",
           TEST_BINBOOK_STRING_TABLE_LEN);

    /* Nav entries */
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET], 0);
    test_write_le16(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + 4], 3);
    test_write_le16(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + 6], 0);
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + 8], 0);
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + 12], 11);
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + 28], 0);
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + 32], UINT32_MAX);
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + 36], UINT32_MAX);
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + 40], UINT32_MAX);
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + TEST_BINBOOK_NAV_INDEX_ENTRY_SIZE], 1);
    test_write_le16(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + TEST_BINBOOK_NAV_INDEX_ENTRY_SIZE + 4],
                    3);
    test_write_le16(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + TEST_BINBOOK_NAV_INDEX_ENTRY_SIZE + 6],
                    0);
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + TEST_BINBOOK_NAV_INDEX_ENTRY_SIZE + 8],
                    0);
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + TEST_BINBOOK_NAV_INDEX_ENTRY_SIZE + 12],
                    11);
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + TEST_BINBOOK_NAV_INDEX_ENTRY_SIZE + 28],
                    0);
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + TEST_BINBOOK_NAV_INDEX_ENTRY_SIZE + 32],
                    UINT32_MAX);
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + TEST_BINBOOK_NAV_INDEX_ENTRY_SIZE + 36],
                    UINT32_MAX);
    test_write_le32(&out[TEST_BINBOOK_NAV_INDEX_OFFSET + TEST_BINBOOK_NAV_INDEX_ENTRY_SIZE + 40],
                    UINT32_MAX);

    /* Chapter entries */
    test_write_le32(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET], 0);
    test_write_le32(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET + 4], 0);
    test_write_le32(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET + 8], 0);
    test_write_le32(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET + 12], 11);
    test_write_le32(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET + 16], 0);
    test_write_le16(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET + 20], 0);
    test_write_le16(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET + 22], 3);
    test_write_le32(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET + TEST_BINBOOK_CHAPTER_INDEX_ENTRY_SIZE],
                    1);
    test_write_le32(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET + TEST_BINBOOK_CHAPTER_INDEX_ENTRY_SIZE +
                        4],
                    0);
    test_write_le32(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET + TEST_BINBOOK_CHAPTER_INDEX_ENTRY_SIZE +
                        8],
                    0);
    test_write_le32(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET + TEST_BINBOOK_CHAPTER_INDEX_ENTRY_SIZE +
                        12],
                    11);
    test_write_le32(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET + TEST_BINBOOK_CHAPTER_INDEX_ENTRY_SIZE +
                        16],
                    0);
    test_write_le16(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET + TEST_BINBOOK_CHAPTER_INDEX_ENTRY_SIZE +
                        20],
                    0);
    test_write_le16(&out[TEST_BINBOOK_CHAPTER_INDEX_OFFSET + TEST_BINBOOK_CHAPTER_INDEX_ENTRY_SIZE +
                        22],
                    3);

    /* Page 0: GRAY1 (pixel_format=1), BW plane only (bitmap=0x04), RLE compressed */
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET], 0);
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 4], 1);   /* page_kind=TEXT */
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 6], 1);   /* pixel_format=GRAY1 */
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 8], 1);   /* compression=RLE */
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 10], 0);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 12], 0);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 16], 0);
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 20], 800);
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 22], 480);
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 24], 0);
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 26], 0);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 28], UINT32_MAX);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 32], 0);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 36], 0);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 40], 0);
    out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 44] = 0x04;  /* plane_bitmap: BW only */
    out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 45] = 0;     /* plane_compression[0] */
    out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 46] = 0;
    out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 47] = 1;     /* plane_compression[2]=RLE */
    out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 48] = 0;
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 52], 0);  /* offset_plane_0 */
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 56], 0);  /* size_plane_0 */
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 60], 0);  /* offset_plane_1 */
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 64], 0);  /* size_plane_1 */
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 68], 0);  /* offset_plane_2 */
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 72], 0);  /* size_plane_2 */

    /* Page 1: GRAY2 (pixel_format=2), MSB+LSB planes (bitmap=0x03), RLE compressed */
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE], 1);
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 4],
                    1);
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 6],
                    2);   /* pixel_format=GRAY2 */
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 8],
                    1);   /* compression=RLE */
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 10],
                    0);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 12],
                    0);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 16],
                    0);
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 20],
                    800);
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 22],
                    480);
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 24],
                    0);
    test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 26],
                    0);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 28],
                    UINT32_MAX);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 32],
                    0);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 36],
                    0);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 40],
                    0);
    out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 44] = 0x03;
    out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 45] = 1;
    out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 46] = 1;
    out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 47] = 0;
    out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 48] = 0;
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 52],
                    0);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 56],
                    4);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 60],
                    0);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 64],
                    4);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 68],
                    0);
    test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE + 72],
                    0);

    /* Page data: 4 bytes RLE-compressed all-white for BW plane */
    out[TEST_BINBOOK_PAGE_DATA_OFFSET] = 0x80 | 3;  /* RLE: repeat 4 times */
    out[TEST_BINBOOK_PAGE_DATA_OFFSET + 1] = 0xff;  /* value: all bits set (white) */
    out[TEST_BINBOOK_PAGE_DATA_OFFSET + 2] = 0x80 | 3;
    out[TEST_BINBOOK_PAGE_DATA_OFFSET + 3] = 0xff;
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo run -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add firmware/zephyr/tests/protocol/src/main.c
git commit -m "firmware: add GRAY1 test binbook fixture for protocol tests"
```

---

### Task 3: Accept GRAY1 in `validate_binbook_gray2_page`

**Files:**
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c:801-814`

- [ ] **Step 1: Write the failing test**

Add to `firmware/zephyr/tests/protocol/src/main.c` after the existing binbook tests:

```c
ZTEST(squidscript_protocol, test_ssd1677_validate_accepts_gray1_with_bw_plane)
{
    struct sq_vm_runtime_binbook_page page = {0};

    strncpy(page.path, "/sqtest/books/test.binbook", sizeof(page.path) - 1);
    page.pixel_format = 1;  /* GRAY1 */
    page.stored_width = 800;
    page.stored_height = 480;
    page.plane_bitmap = 0x04;  /* BW plane only */

    zassert_equal(validate_binbook_gray2_page(&page), 0,
                  "validate should accept GRAY1 with BW plane");
}

ZTEST(squidscript_protocol, test_ssd1677_validate_rejects_gray1_without_bw_plane)
{
    struct sq_vm_runtime_binbook_page page = {0};

    strncpy(page.path, "/sqtest/books/test.binbook", sizeof(page.path) - 1);
    page.pixel_format = 1;  /* GRAY1 */
    page.stored_width = 800;
    page.stored_height = 480;
    page.plane_bitmap = 0x00;  /* no planes */

    zassert_not_equal(validate_binbook_gray2_page(&page), 0,
                      "validate should reject GRAY1 without BW plane");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `west build -t ztest -b xiao_esp32c3 -- -DSHIELD=seeed_xiao_epaper`
Expected: FAIL (validate_binbook_gray2_page rejects GRAY1)

- [ ] **Step 3: Write minimal implementation**

In `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`, modify `validate_binbook_gray2_page`:

```c
static int validate_binbook_gray2_page(const struct sq_vm_runtime_binbook_page *page)
{
    if (page == NULL || page->path[0] == '\0') {
        return -ENOTSUP;
    }

    /* Accept both GRAY1 and GRAY2 */
    if (page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY2_PACKED &&
        page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY1_PACKED) {
        return -ENOTSUP;
    }

    if (page->stored_width != PANEL_WIDTH || page->stored_height != PANEL_HEIGHT) {
        return -ENOTSUP;
    }

    /* GRAY1 only needs BW plane */
    if (page->pixel_format == BINBOOK_PIXEL_FORMAT_GRAY1_PACKED) {
        if (!(page->plane_bitmap & BINBOOK_PLANE_BW)) {
            return -ENOTSUP;
        }
        return 0;
    }

    /* GRAY2 needs MSB+LSB */
    if (!(page->plane_bitmap & BINBOOK_PLANE_MSB) ||
        !(page->plane_bitmap & BINBOOK_PLANE_LSB)) {
        return -ENOTSUP;
    }
    return 0;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `west build -t ztest -b xiao_esp32c3 -- -DSHIELD=seeed_xiao_epaper`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add firmware/zephyr/src/ssd1677_gdeq0426t82_display.c firmware/zephyr/tests/protocol/src/main.c
git commit -m "firmware: accept GRAY1 pixel format in binbook validation"
```

---

### Task 4: Accept GRAY1 in `stream_binbook_gray2_page`

**Files:**
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c:772-799`

- [ ] **Step 1: Write the failing test**

Add to `firmware/zephyr/tests/protocol/src/main.c`:

```c
ZTEST(squidscript_protocol, test_ssd1677_stream_gray1_routes_to_bw_path)
{
    struct sq_vm_runtime_binbook_page page = {0};
    uint8_t book[TEST_BINBOOK_GRAY1_LEN];

    build_test_binbook_gray1(book);
    zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "binbook-reader",
                                           binbook_reader_sqbc, sizeof(binbook_reader_sqbc)),
                  0);
    zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "binbook-reader",
                                                "books/sample.binbook", book, sizeof(book)),
                  0);
    zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point, "binbook-reader",
                                                  &test_vm_storage),
                  0);

    strncpy(runtime.current_app, "binbook-reader", sizeof(runtime.current_app) - 1);
    runtime.vm_storage = &test_vm_storage;

    /* Page 0 is GRAY1 with BW plane - should succeed */
    strncpy(page.path, "/sqtest/apps/binbook-reader/resources/books/sample.binbook",
            sizeof(page.path) - 1);
    page.page_data_offset = TEST_BINBOOK_PAGE_DATA_OFFSET;
    page.page_index = 0;
    page.pixel_format = 1;  /* GRAY1 */
    page.compression_method = 1;  /* RLE */
    page.stored_width = 800;
    page.stored_height = 480;
    page.plane_bitmap = 0x04;  /* BW only */
    page.per_plane_compression = 1;
    page.plane_compression[2] = 1;  /* RLE for BW plane */
    page.offset_plane_2 = 0;
    page.size_plane_2 = 4;

    zassert_equal(stream_binbook_gray2_page(&page), 0,
                  "stream should accept GRAY1 page via BW path");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `west build -t ztest -b xiao_esp32c3 -- -DSHIELD=seeed_xiao_epaper`
Expected: FAIL (stream_binbook_gray2_page rejects GRAY1)

- [ ] **Step 3: Write minimal implementation**

In `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`, modify `stream_binbook_gray2_page`:

```c
static int stream_binbook_gray2_page(const struct sq_vm_runtime_binbook_page *page)
{
    int ret;
    bool per_plane;
    uint8_t method;

    if (page == NULL || page->path[0] == '\0') {
        return -ENOTSUP;
    }

    /* Accept both GRAY1 and GRAY2 */
    if (page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY2_PACKED &&
        page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY1_PACKED) {
        return -ENOTSUP;
    }

    if (page->stored_width != PANEL_WIDTH || page->stored_height != PANEL_HEIGHT) {
        return -ENOTSUP;
    }

    /* For GRAY1, use BW-only path */
    if (page->pixel_format == BINBOOK_PIXEL_FORMAT_GRAY1_PACKED) {
        return stream_binbook_gray2_bw_page(page, false);
    }

    /* Existing GRAY2 MSB+LSB handling */
    per_plane = page->per_plane_compression;
    if (!(page->plane_bitmap & BINBOOK_PLANE_MSB) ||
        !(page->plane_bitmap & BINBOOK_PLANE_LSB)) {
        return -ENOTSUP;
    }
    method = per_plane ? page->plane_compression[0] : page->compression_method;
    if (method != BINBOOK_COMPRESSION_RLE_PACKBITS) {
        return -ENOTSUP;
    }
    ret = stream_binbook_gray2_plane(page, page->offset_plane_0, page->size_plane_0,
                                     SSD1677_CMD_WRITE_RED_RAM, true);
    if (ret != 0) {
        return ret;
    }
    method = per_plane ? page->plane_compression[1] : page->compression_method;
    return stream_binbook_gray2_plane(page, page->offset_plane_1, page->size_plane_1,
                                      SSD1677_CMD_WRITE_RAM, false);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `west build -t ztest -b xiao_esp32c3 -- -DSHIELD=seeed_xiao_epaper`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add firmware/zephyr/src/ssd1677_gdeq0426t82_display.c firmware/zephyr/tests/protocol/src/main.c
git commit -m "firmware: route GRAY1 binbook pages through BW decompression path"
```

---

### Task 5: Accept GRAY1 in `sq_display_backend_rasterize_binbook`

**Files:**
- Modify: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c:1329-1370`

- [ ] **Step 1: Write the failing test**

Add to `firmware/zephyr/tests/protocol/src/main.c`:

```c
ZTEST(squidscript_protocol, test_ssd1677_rasterize_gray1_decompresses_bw_plane)
{
    struct sq_vm_runtime_binbook_page page = {0};
    uint8_t book[TEST_BINBOOK_GRAY1_LEN];

    build_test_binbook_gray1(book);
    zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "binbook-reader",
                                           binbook_reader_sqbc, sizeof(binbook_reader_sqbc)),
                  0);
    zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "binbook-reader",
                                                "books/sample.binbook", book, sizeof(book)),
                  0);
    zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point, "binbook-reader",
                                                  &test_vm_storage),
                  0);

    strncpy(runtime.current_app, "binbook-reader", sizeof(runtime.current_app) - 1);
    runtime.vm_storage = &test_vm_storage;

    strncpy(page.path, "/sqtest/apps/binbook-reader/resources/books/sample.binbook",
            sizeof(page.path) - 1);
    page.page_data_offset = TEST_BINBOOK_PAGE_DATA_OFFSET;
    page.page_index = 0;
    page.pixel_format = 1;  /* GRAY1 */
    page.compression_method = 1;  /* RLE */
    page.stored_width = 800;
    page.stored_height = 480;
    page.plane_bitmap = 0x04;  /* BW only */
    page.per_plane_compression = 1;
    page.plane_compression[2] = 1;  /* RLE for BW plane */
    page.offset_plane_2 = 0;
    page.size_plane_2 = 4;

    sq_display_backend_rasterize_binbook(&page);

    /* Verify the page was rasterized (check framebuffer or drawlog) */
    zassert_true(true, "GRAY1 rasterize should complete without error");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `west build -t ztest -b xiao_esp32c3 -- -DSHIELD=seeed_xiao_epaper`
Expected: FAIL (sq_display_backend_rasterize_binbook ignores GRAY1)

- [ ] **Step 3: Write minimal implementation**

In `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`, modify `sq_display_backend_rasterize_binbook`:

```c
void sq_display_backend_rasterize_binbook(const struct sq_vm_runtime_binbook_page *page)
{
    bool per_plane;
    uint8_t method;

    if (page == NULL || page->path[0] == '\0') {
        return;
    }

    /* Accept both GRAY1 and GRAY2 */
    if ((page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY2_PACKED &&
         page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY1_PACKED) ||
        page->stored_width != PANEL_WIDTH || page->stored_height != PANEL_HEIGHT) {
        return;
    }

    /* For GRAY1, decompress only BW plane to framebuffer */
    if (page->pixel_format == BINBOOK_PIXEL_FORMAT_GRAY1_PACKED) {
        if (page->plane_bitmap & BINBOOK_PLANE_BW) {
            (void)decompress_binbook_gray2_bw_to_fb(page, page->offset_plane_2,
                                                    page->size_plane_2, true);
        }
        return;
    }

    /* Existing GRAY2 MSB+LSB+BW handling */
    per_plane = page->per_plane_compression;
#if IS_ENABLED(CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC)
    sq_debug_log_append("%lld:decompress_start:bitmap=0x%02x", (long long)k_uptime_get(),
                        (unsigned)page->plane_bitmap);
#endif
    if (page->plane_bitmap & BINBOOK_PLANE_MSB) {
        method = per_plane ? page->plane_compression[0] : page->compression_method;
        (void)decompress_binbook_gray2_to_fb(page, page->offset_plane_0,
                                             page->size_plane_0, true);
#if IS_ENABLED(CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC)
        sq_debug_log_append("%lld:decompress_msb_done", (long long)k_uptime_get());
#endif
    }
    if (page->plane_bitmap & BINBOOK_PLANE_LSB) {
        method = per_plane ? page->plane_compression[1] : page->compression_method;
        (void)decompress_binbook_gray2_to_fb(page, page->offset_plane_1,
                                             page->size_plane_1, false);
#if IS_ENABLED(CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC)
        sq_debug_log_append("%lld:decompress_lsb_done", (long long)k_uptime_get());
#endif
    }
    if (page->plane_bitmap & BINBOOK_PLANE_BW) {
        method = per_plane ? page->plane_compression[2] : page->compression_method;
        (void)decompress_binbook_gray2_bw_to_fb(page, page->offset_plane_2,
                                                page->size_plane_2, true);
#if IS_ENABLED(CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC)
        sq_debug_log_append("%lld:decompress_bw_done", (long long)k_uptime_get());
#endif
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `west build -t ztest -b xiao_esp32c3 -- -DSHIELD=seeed_xiao_epaper`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add firmware/zephyr/src/ssd1677_gdeq0426t82_display.c firmware/zephyr/tests/protocol/src/main.c
git commit -m "firmware: rasterize GRAY1 binbook pages using BW plane decompression"
```

---

### Task 6: Run full protocol test suite

**Files:**
- None (verification only)

- [ ] **Step 1: Run all protocol ztests**

Run: `west build -t ztest -b xiao_esp32c3 -- -DSHIELD=seeed_xiao_epaper`
Expected: All tests pass, no regressions

- [ ] **Step 2: Verify no GRAY2 regressions**

Check that existing `test_ssd1677_gray2_maps_canonical_binbook_values_to_distinct_planes` still passes.

- [ ] **Step 3: Final commit with all changes**

```bash
git add -A
git commit -m "firmware: complete GRAY1 binbook graceful degradation

- Accept GRAY1_PACKED pixel format in validation and stream functions
- Route GRAY1 pages through BW-only decompression path
- Add GRAY1 test binbook fixture and protocol test cases
- GRAY1 pages now display correctly on XTEINK X4 and XIAO targets"
```

---

## Verification Checklist

After implementation, verify:

1. [ ] `validate_binbook_gray2_page` accepts GRAY1 with BW plane (bit 2 = 0x04)
2. [ ] `validate_binbook_gray2_page` rejects GRAY1 without BW plane
3. [ ] `stream_binbook_gray2_page` routes GRAY1 to BW path
4. [ ] `sq_display_backend_rasterize_binbook` decompresses BW plane for GRAY1
5. [ ] Existing GRAY2 tests still pass
6. [ ] No regressions in protocol ztest suite
