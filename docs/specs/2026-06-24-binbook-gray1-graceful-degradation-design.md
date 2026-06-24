# BinBook GRAY1 Graceful Degradation

Status: proposed

## Goal

Allow the XTEINK X4 and XIAO e-paper targets to display GRAY1-packed binbook pages
by using the existing BW plane decompression path, rather than rejecting
the pages as unsupported.

## Problem

Both targets declare `supportedPixelFormats: ["GRAY1_PACKED", "GRAY2_PACKED"]` in
their target JSON files. However, the SSD1677 display driver only accepts
`GRAY2_PACKED` pages and returns `-ENOTSUP` for any other pixel format. A binbook
with GRAY1 pages (1-bit B&W) gets rejected even though the hardware can display it.

## Background

### Pixel Format Differences

| Format | Bits/pixel | Pixels/byte | Page size (800×480) |
|--------|------------|-------------|---------------------|
| GRAY1  | 1          | 8           | 48,000 bytes        |
| GRAY2  | 2          | 4           | 96,000 bytes        |

### Plane Structure

Both GRAY1 and GRAY2 use the same plane bitmap layout:
- Slot 0 (bit 0): MSB plane (WRITE_RED_RAM)
- Slot 1 (bit 1): LSB plane (WRITE_RAM)
- Slot 2 (bit 2): BW plane (WRITE_RAM, 1-bit dithered)
- Slot 3 (bit 3): Delta plane (future)

Each plane is a 1-bit framebuffer: `stored_width / 8 * stored_height` bytes.

For GRAY2, the writer decomposes 2-bit pixels into separate MSB and LSB planes.
For GRAY1, the MSB plane IS the pixel data (1 bit = 1 pixel), and LSB is redundant.

### The Issue

The existing `stream_binbook_gray2_plane` function uses
`sq_ssd1677_gray2_msb_active_mask` which expects 2-bit packed bytes (4 pixels/byte).
GRAY1 data is 1-bit packed (8 pixels/byte), so those mask functions produce wrong output.

## Approach: BW-Only Path

For GRAY1 B&W-only pages, the simplest path is:
1. Accept `GRAY1_PACKED` pixel format in validation
2. Decompress only the BW plane (slot 2)
3. Write to WRITE_RAM
4. Trigger BW-only refresh

This leverages the existing BW plane decompression path and avoids
complex plane conversion logic.

### Why BW-Only?

- GRAY1 is B&W-only (1-bit), so the BW plane contains the complete image
- MSB/LSB planes are redundant for GRAY1 content
- BW-only refresh is faster (~0.5s vs ~2.5s for full grayscale)
- Reduces ghosting by avoiding unnecessary grayscale transitions

## Firmware Changes

### File: `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`

#### 1. Add GRAY1 Pixel Format Constant

```c
#define BINBOOK_PIXEL_FORMAT_GRAY1_PACKED 1U
```

#### 2. Modify `stream_binbook_gray2_page`

```c
static int stream_binbook_gray2_page(const struct sq_vm_runtime_binbook_page *page)
{
    int ret;
    bool per_plane;
    uint8_t method;

    if (page == NULL || page->path[0] == '\0') {
        return -ENOTSUP;
    }

    // Accept both GRAY1 and GRAY2
    if (page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY2_PACKED &&
        page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY1_PACKED) {
        return -ENOTSUP;
    }

    if (page->stored_width != PANEL_WIDTH || page->stored_height != PANEL_HEIGHT) {
        return -ENOTSUP;
    }

    // For GRAY1, use BW-only path
    if (page->pixel_format == BINBOOK_PIXEL_FORMAT_GRAY1_PACKED) {
        return stream_binbook_gray2_bw_page(page, false);
    }

    // Existing GRAY2 MSB+LSB handling
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

#### 3. Modify `validate_binbook_gray2_page`

```c
static int validate_binbook_gray2_page(const struct sq_vm_runtime_binbook_page *page)
{
    if (page == NULL || page->path[0] == '\0') {
        return -ENOTSUP;
    }

    // Accept both GRAY1 and GRAY2
    if (page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY2_PACKED &&
        page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY1_PACKED) {
        return -ENOTSUP;
    }

    if (page->stored_width != PANEL_WIDTH || page->stored_height != PANEL_HEIGHT) {
        return -ENOTSUP;
    }

    // GRAY1 only needs BW plane
    if (page->pixel_format == BINBOOK_PIXEL_FORMAT_GRAY1_PACKED) {
        if (!(page->plane_bitmap & BINBOOK_PLANE_BW)) {
            return -ENOTSUP;
        }
        return 0;
    }

    // GRAY2 needs MSB+LSB
    if (!(page->plane_bitmap & BINBOOK_PLANE_MSB) ||
        !(page->plane_bitmap & BINBOOK_PLANE_LSB)) {
        return -ENOTSUP;
    }
    return 0;
}
```

#### 4. Modify `sq_display_backend_rasterize_binbook`

```c
void sq_display_backend_rasterize_binbook(const struct sq_vm_runtime_binbook_page *page)
{
    bool per_plane;
    uint8_t method;

    if (page == NULL || page->path[0] == '\0') {
        return;
    }

    // Accept both GRAY1 and GRAY2
    if ((page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY2_PACKED &&
         page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY1_PACKED) ||
        page->stored_width != PANEL_WIDTH || page->stored_height != PANEL_HEIGHT) {
        return;
    }

    // For GRAY1, decompress only BW plane to framebuffer
    if (page->pixel_format == BINBOOK_PIXEL_FORMAT_GRAY1_PACKED) {
        if (page->plane_bitmap & BINBOOK_PLANE_BW) {
            (void)decompress_binbook_gray2_bw_to_fb(page, page->offset_plane_2,
                                                    page->size_plane_2, true);
        }
        return;
    }

    // Existing GRAY2 MSB+LSB+BW handling
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

## Refresh Mode Handling

GRAY1 pages use BW-only refresh by default:
- **BW_DIFFERENTIAL_PARTIAL**: Fast BW-only refresh (~0.5s)
- **GRAY2_FULL**: Full grayscale refresh (~2.5s)

Since GRAY1 is B&W-only, we always use `BW_DIFFERENTIAL_PARTIAL`. This is:
- Faster page turns
- Reduced ghosting
- Lower power consumption

The existing refresh decision logic in `sq_ssd1677_binbook_refresh_decide`
already handles this correctly—no changes needed.

## Testing Strategy

### Unit Tests

1. **Protocol test with GRAY1 fixture**: Add GRAY1 page to test binbook
2. **Validation test**: Verify `validate_binbook_gray2_page` accepts GRAY1 with BW plane
3. **Rejection test**: Verify GRAY1 without BW plane returns `-ENOTSUP`
4. **Decompression test**: Verify BW plane decompresses correctly

### Hardware Tests

1. **Generate GRAY1 test binbook**: B&W content only (text, patterns)
2. **Flash to XIAO/XTEINK**: Verify page renders correctly
3. **Measure refresh time**: Expect ~0.5s for BW-only
4. **Visual verification**: Black/white only, no gray artifacts

### Edge Cases

- GRAY1 page with no BW plane → `-ENOTSUP`
- GRAY1 page with wrong dimensions → `-ENOTSUP`
- GRAY1 page with MSB+LSB planes → ignored (BW-only path)
- GRAY1 page with per-plane compression → handled correctly

## Success Criteria

1. GRAY1 binbook pages display correctly on XTEINK X4 and XIAO targets
2. Refresh time is ~0.5s (BW-only), not ~2.5s (full grayscale)
3. No visual artifacts (correct black/white rendering)
4. Existing GRAY2 pages continue to work without regression
5. All protocol ztests pass

## Future Extensions

If GRAY1 pages with MSB+LSB planes are needed later:
- Add `stream_binbook_gray1_plane` function for 1-bit packed data
- Handle MSB plane → WRITE_RED_RAM
- Handle LSB plane → WRITE_RAM
- Trigger full grayscale refresh

This is out of scope for the initial implementation.
