# BinBook Per-Plane Blob Format

Status: proposed

## Goal

Redesign the binbook page data layout so each plane is stored as a separate
compressed blob. Devices decompress only the planes they need.

Motivation:
- E-paper: 3 decompression passes × 57,600 PackBits iterations = ~3.2s on
  ESP32-C3. BW-only devices waste 2/3 of that work. Per-plane storage lets
  them read only the BW plane.
- Color LCD/OLED: single pixel buffer, decompress full blob and convert to
  native format. Per-plane storage allows future channel-separated compression.
- Future differential rendering: delta planes stored separately from base planes.

## Scope

Changes the binbook format spec, Rust crate parser, Python generator, and C
firmware page index reader. All repos updated together. Pre-1.0 — no backward
compatibility constraints.

BinBook is a compiled book format. Pages are pre-rendered at the target's
native pixel format and dimensions so the device does minimal work. The format
must support grayscale e-paper, monochrome OLED, and color LCD/OLED displays.

## Pixel Format Extensions

Current constants (kept):

| Value | Name          | Description                        |
|-------|---------------|------------------------------------|
| 1     | GRAY1_PACKED  | 1-bit, 8 pixels/byte, B&W          |
| 2     | GRAY2_PACKED  | 2-bit, 4 pixels/byte, 4-gray       |
| 4     | GRAY4_PACKED  | 4-bit, 2 pixels/byte, 16-gray      |

New constants:

| Value | Name          | Description                        |
|-------|---------------|------------------------------------|
| 8     | RGB565        | 16-bit, 5-6-5, little-endian       |
| 16    | RGB888        | 24-bit, 8-8-8                      |
| 32    | RGBA8888      | 32-bit, 8-8-8-8                    |

**Row padding**: each row is padded to the nearest whole byte boundary.
RGB565 row size = `width * 2` bytes. RGB888 row size = `width * 3` bytes.

## Format Changes

### Page Index Entry (76 → 128 bytes)

The three blob fields (`relative_blob_offset`, `compressed_size`,
`uncompressed_size`) are replaced with an inline plane directory. The 16
reserved bytes are repurposed. Entry size grows from 76 to 128 bytes
(128 = 2^7, cache-line aligned, 20 bytes headroom for future fields).

```c
struct PageIndexEntry {
    u32  page_number;                 // 0-based page index
    u16  page_kind;                   // 1=TEXT, 2=IMAGE, 3=MIXED_RESERVED
    u16  pixel_format;                // Pixel format (see constants above)

    u16  compression_method;          // Default: 0=NONE, 1=RLE_PACKBITS, 2=LZ4
    u16  update_hint;                 // 0=default, 1=full_refresh, 2=partial_refresh_ok
    u32  page_flags;                  // bit 0: per_plane_compression
                                      // bit 1: reserved
                                      // bits 2-31: reserved

    u32  page_crc32;                  // CRC32 over all plane blobs (0 = not computed)

    u16  stored_width;                // Pixel width
    u16  stored_height;               // Pixel height
    u16  placement_x;                 // X offset within content box (usually 0)
    u16  placement_y;                 // Y offset within content box (usually 0)

    u32  source_spine_index;          // EPUB spine index (UINT32_MAX = none)
    u32  chapter_nav_index;           // NAV_INDEX index (UINT32_MAX = none)

    u32  progress_start_ppm;          // Progress at start (0–1,000,000)
    u32  progress_end_ppm;            // Progress at end

    // --- Inline plane directory (32 bytes) ---
    u8   plane_bitmap;                // Which planes are stored (see below)
    u8   plane_compression[4];        // Per-plane compression (only if page_flags bit 0)
    u8   plane_dir_padding[3];        // Alignment to 4-byte boundary
    u32  offset_plane_0;              // Byte offset from PAGE_DATA start
    u32  size_plane_0;                // Compressed size in bytes
    u32  offset_plane_1;              // Byte offset from PAGE_DATA start
    u32  size_plane_1;                // Compressed size in bytes
    u32  offset_plane_2;              // Byte offset from PAGE_DATA start
    u32  size_plane_2;                // Compressed size in bytes
    u32  offset_plane_3;              // Byte offset from PAGE_DATA start (future)
    u32  size_plane_3;                // Compressed size in bytes

    u8   reserved[44];                // Future use (plane delta offsets, etc.)
};
```

Total: 128 bytes per entry.

### Per-Plane Compression

By default, `compression_method` at the page level applies to all planes.
When `page_flags` bit 0 (`per_plane_compression`) is set, each plane uses
its own method from `plane_compression[4]` instead.

```
plane_compression[0] → compression for plane 0
plane_compression[1] → compression for plane 1
plane_compression[2] → compression for plane 2
plane_compression[3] → compression for plane 3
```

This allows mixing compressors per plane. Example for a GRAY2 e-paper page:

```
compression_method = LZ4           (page default, unused when per_plane set)
page_flags bit 0 = 1               (per-plane compression enabled)
plane_bitmap = 0x07                (MSB + LSB + BW)
plane_compression[0] = RLE_PACKBITS  (MSB: uniform, RLE is fast)
plane_compression[1] = RLE_PACKBITS  (LSB: uniform, RLE is fast)
plane_compression[2] = LZ4           (BW: dithered, LZ4 handles texture)
```

When `page_flags` bit 0 is clear, `plane_compression` is ignored and all
planes use `compression_method`.

**Recommendation**: writers should set `per_plane_compression` when planes
have different content characteristics. For simple cases (all planes same
method), leave bit 0 clear and use the page default.

### Plane Bitmap Interpretation

The `plane_bitmap` bits indicate which of the 4 slot pairs are present. What
each slot means depends on `pixel_format`:

**GRAY1 / GRAY2 / GRAY4 (e-paper):**

| Bit | Value | Plane | Description |
|-----|-------|-------|-------------|
| 0   | 0x01  | 0     | MSB plane (WRITE_RED_RAM) |
| 1   | 0x02  | 1     | LSB plane (WRITE_RAM) |
| 2   | 0x04  | 2     | BW plane (WRITE_RAM, 1-bit dithered) |
| 3   | 0x08  | 3     | Delta plane (future) |

E-paper controllers store separate RAM planes. The firmware reads only the
planes it needs for the current refresh mode. BW-only refresh reads slot 2
only. Full grayscale reads slots 0+1+2.

**RGB565 / RGB888 / RGBA8888 (color LCD/OLED):**

| Bit | Value | Plane | Description |
|-----|-------|-------|-------------|
| 0   | 0x01  | 0     | Full pixel buffer |
| 1-3 | —     | —     | Reserved (future: separate R/G/B channels) |

Color displays decompress the full pixel buffer from slot 0 and convert to
their native format. The plane bitmap should have only bit 0 set.

**Future: channel-separated color**

For constrained devices that can't hold a full RGB buffer, the format could
store R/G/B as separate compressed channels:

| Bit | Value | Plane | Description |
|-----|-------|-------|-------------|
| 0   | 0x01  | 0     | Red channel |
| 1   | 0x02  | 1     | Green channel |
| 2   | 0x04  | 2     | Blue channel |
| 3   | 0x08  | 3     | Alpha channel (future) |

This is a future extension. Current writers should always use slot 0 for the
full pixel buffer.

### Header Changes

```c
struct BinBookHeader {
    ...
    u16  page_index_entry_size;      // 128 (was 76)
    ...
};
```

### PAGE_DATA Section

Raw concatenated plane blobs. No page-local headers — the page index entry
is the authority.

```
PAGE_DATA:
├── [plane 0 blob page 0]    ← PAGE_INDEX[0].offset_plane_0
├── [plane 1 blob page 0]    ← PAGE_INDEX[0].offset_plane_1
├── [plane 2 blob page 0]    ← PAGE_INDEX[0].offset_plane_2
├── [plane 0 blob page 1]    ← PAGE_INDEX[1].offset_plane_0
├── ...
```

Each blob is:
1. Read from `header.page_data_offset + page.offset_plane_N`
2. Validate size = `page.size_plane_N`
3. Optionally validate CRC32 if `page.page_crc32 != 0`
4. Decompress using per-plane or page-default compression method

### Compression

Each plane blob is independently compressed. Compression is either page-wide
(default) or per-plane (when `page_flags` bit 0 is set).

| Value | Method       | Best for |
|-------|--------------|----------|
| 0     | NONE         | Already-native pixel data |
| 1     | RLE_PACKBITS | Uniform content, e-paper MSB/LSB planes |
| 2     | LZ4          | Textured/dithered content, color, BW planes |
| 3     | DELTA_LZ4    | Future: differential page encoding |

**Recommendation**: use LZ4 for BW dithered planes and color. RLE for
uniform e-paper MSB/LSB planes. NONE for small pages where compression
overhead exceeds the saved I/O.

### Decompressed Plane Sizes

The firmware computes decompressed plane sizes from `pixel_format`,
`stored_width`, and `stored_height`. No `uncompressed_size` field is needed.

| Pixel Format | Plane | Decompressed Size |
|-------------|-------|-------------------|
| GRAY1_PACKED | 0 (MSB) | `stored_width / 8 * stored_height` |
| GRAY1_PACKED | 1 (LSB) | `stored_width / 8 * stored_height` |
| GRAY1_PACKED | 2 (BW)  | `stored_width / 8 * stored_height` |
| GRAY2_PACKED | 0 (MSB) | `stored_width / 8 * stored_height` |
| GRAY2_PACKED | 1 (LSB) | `stored_width / 8 * stored_height` |
| GRAY2_PACKED | 2 (BW)  | `stored_width / 8 * stored_height` |
| GRAY4_PACKED | 0 (MSB) | `stored_width / 4 * stored_height` |
| GRAY4_PACKED | 1 (LSB) | `stored_width / 4 * stored_height` |
| GRAY4_PACKED | 2 (BW)  | `stored_width / 4 * stored_height` |
| RGB565       | 0 (full) | `stored_width * 2 * stored_height` |
| RGB888       | 0 (full) | `stored_width * 3 * stored_height` |
| RGBA8888     | 0 (full) | `stored_width * 4 * stored_height` |

For e-paper GRAY2, each of the 3 planes (MSB, LSB, BW) is a 1-bit
framebuffer: `480 / 8 * 480 = 28,800` bytes per plane. The GRAY2 packed
data (57,600 bytes) is decomposed into these 1-bit planes by the writer.

For color, slot 0 holds the full pixel buffer in the declared format.

### Plane Blob Alignment

Plane blob offsets (`offset_plane_*`) must be 4-byte aligned within
PAGE_DATA. Writers pad between blobs with zero bytes. Readers must not
assume any tighter alignment.

### CRC32

`page_crc32` is a single CRC32 over all plane blobs for that page,
computed as: `CRC32(blob_0 || blob_1 || blob_2 || blob_3)` (concatenated
in slot order, skipping absent planes).

- `0` = CRC not computed (validation skipped)
- Nonzero = firmware may validate before decompressing

CRC32 is optional and primarily for corruption detection (SD card bit rot,
transfer errors). Firmware should not fail hard on CRC mismatch during
normal operation — use it as a diagnostic signal. The CPU cost is ~15ms
per page with hardware CRC, ~57ms with software CRC.

Per-plane CRCs are not supported. If per-plane validation is needed in the
future, the 20 bytes of reserved space in the page index entry can hold
per-plane CRC fields.

### Delta Planes (Future)

Delta planes (slot 3) store an XOR diff against the previous page's
corresponding plane, then compressed with the method in
`plane_compression[3]`. Keyframe pages (full compressed planes) appear at
configurable intervals for random access.

This is a future extension. Current writers must not set `plane_bitmap`
bit 3. The delta plane format will be specified when differential
rendering is implemented.

### READER_REQUIREMENTS Updates

The `required_compression_methods` bitmask in the READER_REQUIREMENTS
section must include LZ4 when the file uses LZ4-compressed planes:

| Bit | Method       |
|-----|--------------|
| 0   | NONE         |
| 1   | RLE_PACKBITS |
| 2   | LZ4          |
| 3   | DELTA_LZ4    |

`max_uncompressed_page_size` should be set to the size of the largest
single plane (not the sum of all planes). For GRAY2 at 480×480, this is
28,800 bytes (one 1-bit plane). For RGB565 at 480×480, this is 460,800
bytes (full pixel buffer).

## Firmware Changes

### E-paper (SSD1677)

```c
// plane_bitmap interpretation for GRAY2:
//   bit 0 = MSB (slot 0), bit 1 = LSB (slot 1), bit 2 = BW (slot 2)

uint8_t msb_method = per_plane ? page->plane_compression[0] : page->compression_method;
uint8_t lsb_method = per_plane ? page->plane_compression[1] : page->compression_method;
uint8_t bw_method  = per_plane ? page->plane_compression[2] : page->compression_method;

if (refresh_mode == FAST1BPP) {
    if (bitmap & PLANE_BW)
        decompress_plane_to_fb(page, page->offset_plane_2, page->size_plane_2, bw_method);
} else {
    if (bitmap & PLANE_MSB)
        decompress_plane_to_fb(page, page->offset_plane_0, page->size_plane_0, msb_method);
    if (bitmap & PLANE_LSB)
        decompress_plane_to_fb(page, page->offset_plane_1, page->size_plane_1, lsb_method);
    if (bitmap & PLANE_BW)
        decompress_plane_to_fb(page, page->offset_plane_2, page->size_plane_2, bw_method);
}
```

### Color LCD/OLED

```c
// plane_bitmap interpretation for RGB565/RGB888:
//   bit 0 = full pixel buffer (slot 0)

uint8_t method = per_plane ? page->plane_compression[0] : page->compression_method;

if (bitmap & PLANE_0) {
    decompress_to_buffer(page, page->offset_plane_0, page->size_plane_0, method, fb);
    display_write_pixels(fb, page->stored_width, page->stored_height);
}
```

### `sq_vm_runtime_binbook_page` struct

```c
struct sq_vm_runtime_binbook_page {
    char path[SQ_APP_STORE_PATH_MAX];
    uint32_t page_index;
    uint16_t pixel_format;
    uint16_t compression_method;       // Default method (all planes)
    uint16_t stored_width;
    uint16_t stored_height;
    uint8_t plane_bitmap;
    uint8_t per_plane_compression;     // 0 = use page default, 1 = per-plane
    uint8_t plane_compression[4];      // Per-plane methods (valid if per_plane)
    uint32_t offset_plane_0;
    uint32_t size_plane_0;
    uint32_t offset_plane_1;
    uint32_t size_plane_1;
    uint32_t offset_plane_2;
    uint32_t size_plane_2;
    uint32_t offset_plane_3;
    uint32_t size_plane_3;
};
```

## Generator Changes

`generate-test-binbook-480.py` (and the production binbook writer):

1. Set `page_index_entry_size = 128` in the header.
2. For each page, compress planes independently:
   - E-paper GRAY2: RLE for MSB/LSB, LZ4 for BW (set `per_plane_compression`)
   - Color: LZ4 for full pixel buffer (use page default, bit 0 clear)
3. Build page index entries with inline plane directory.
4. Write plane blobs into PAGE_DATA.

## Repos Affected

1. **binbook repo** (`../binbook`):
   - `BINBOOK_FORMAT_SPEC.md` — update PageIndexEntry (76→128), add RGB pixel
     formats, add per-plane compression, update PAGE_DATA section, document
     decompressed plane sizes, alignment, CRC32, delta plane intent, and
     READER_REQUIREMENTS updates
   - `binbook/` Python crate — update page index parser, add color + per-plane compression
   - `rust/src/` — update page index parser, add plane fields to `PageInfo`
   - `tests/` — update test fixtures for new entry size

2. **SquidScript repo**:
   - `docs/specs/2026-06-22-binbook-rust-crate-design.md` — update PageInfo/PageRef
   - `firmware/zephyr/src/vm_runtime.h` — update `sq_vm_runtime_binbook_page` struct
   - `firmware/zephyr/src/vm_runtime_binbook.c` — update page index reader
   - `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c` — plane-directory-driven decompression
   - `scripts/generate-test-binbook-480.py` — new format writer
   - `scripts/generate-test-binbook.py` — update or deprecate

## Testing

- Generate 2-page test binbook with new format (GRAY2 e-paper).
- Verify checkerboard page has MSB+LSB+BW planes, text page has BW only.
- Measure decompression time: BW-only should be ~1/3 of current 3-pass time.
- Verify `fast1bpp` refresh only reads the BW plane (one file open/close).
- Verify plane blob offsets are 4-byte aligned in generated files.
- Verify page_crc32 is correct (or 0 when not computed).
- Run existing 146 protocol ztests (no regressions).
- Rust crate: round-trip parse test for new page index entry.
- Python crate: inspect command shows plane bitmap, per-plane sizes, and compression methods.
- Future: generate color test binbook (RGB565) and verify single-blob path.
- Future: generate delta plane test binbook and verify differential decompression.
