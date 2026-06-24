# BinBook Per-Plane Blob Format Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement per-plane blob storage in the binbook format so devices decompress only the planes they need, cutting BW-only page turn latency from ~3.2s to ~0.5s.

**Architecture:** Replace the single interleaved compressed blob per page with an inline plane directory in the 128-byte page index entry. Each plane (MSB, LSB, BW, future delta) gets its own offset/size/compression in the page index. The display driver reads the plane bitmap and decompresses only the needed planes.

**Tech Stack:** Python (binbook crate), Rust (binbook crate + squidvm-ffi), C (Zephyr firmware), struct pack/unpack

**Spec:** `docs/specs/2026-06-23-binbook-per-plane-blob-format.md`

---

## File Map

### binbook repo (`/var/home/tristan/Documents/parasquid/binbook`)

| File | Change |
|------|--------|
| `BINBOOK_FORMAT_SPEC.md` | Update PageIndexEntry (76→128), add RGB formats, plane directory, decompressed sizes, alignment, CRC32, delta intent, READER_REQUIREMENTS |
| `binbook/structs.py` | Update `_PAGE_INDEX` struct (76→128), add `PlaneDir` fields to `PageIndexEntry` |
| `binbook/reader.py` | Update `decode_page_bytes` to read per-plane blobs, add `decode_plane` method |
| `binbook/writer.py` | Update `_page_index` to write plane directory, update `build_binbook` for per-plane PAGE_DATA |
| `binbook/constants.py` | Add `PAGE_INDEX_ENTRY_SIZE_V2 = 128`, RGB pixel format constants |
| `binbook/pixels.py` | Add RGB565/RGB888/RGBA8888 row size calculations |
| `tests/test_structs.py` | Update PageIndexEntry roundtrip tests for 128-byte entry |
| `tests/test_roundtrip.py` | Add per-plane roundtrip test |

### SquidScript repo (`/var/home/tristan/Documents/parasquid/SquidScript`)

| File | Change |
|------|--------|
| `compiler/rust/crates/squidvm-ffi/src/lib.rs` | Update `RustBinBookPageMeta` struct, update `rust_binbook_page_meta` FFI (hardcoded 76→128) |
| `compiler/rust/crates/squidvm-ffi/abi/manifest.json` | Update binbook page meta ABI if present |
| `firmware/zephyr/src/vm_runtime.h` | Update `sq_vm_runtime_binbook_page` struct (plane directory fields) |
| `firmware/zephyr/src/vm_runtime_binbook.c` | Update `BINBOOK_PAGE_INDEX_ENTRY_SIZE` (76→128), update `runtime_binbook_read_page` |
| `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c` | Update `decompress_binbook_gray2_to_fb` and `_bw_to_fb` to use plane directory |
| `scripts/generate-test-binbook-480.py` | Update to write 128-byte entries with plane directory |

---

## Task 1: Update binbook format spec

**Files:**
- Modify: `/var/home/tristan/Documents/parasquid/binbook/BINBOOK_FORMAT_SPEC.md`

- [ ] **Step 1: Update PageIndexEntry struct in spec**

Replace the 76-byte `PageIndexEntry` struct (section 3.18) with the 128-byte version from `docs/specs/2026-06-23-binbook-per-plane-blob-format.md`. Key changes:
- Remove `relative_blob_offset`, `compressed_size`, `uncompressed_size`
- Add inline plane directory: `plane_bitmap`, `plane_compression[4]`, `plane_dir_padding[3]`, 4× `offset_plane_N`/`size_plane_N`, `reserved[20]`
- Update `page_flags` to document bit 0 (`per_plane_compression`)

- [ ] **Step 2: Add RGB pixel format constants**

In section 4 (Pixel Formats), add:
```
| 8     | RGB565        | 16-bit, 5-6-5, little-endian       |
| 16    | RGB888        | 24-bit, 8-8-8                      |
| 32    | RGBA8888      | 32-bit, 8-8-8-8                    |
```

- [ ] **Step 3: Add decompressed plane sizes section**

Add new section after Compression (section 5) documenting how to compute decompressed plane sizes from `pixel_format` and `stored_width/stored_height`. Use the table from the design spec.

- [ ] **Step 4: Add plane blob alignment, CRC32, delta plane, and READER_REQUIREMENTS sections**

Add sections covering:
- 4-byte aligned plane blob offsets
- CRC32 as single page-level CRC over concatenated blobs
- Delta plane intent (slot 3, future, XOR + LZ4)
- READER_REQUIREMENTS updates (LZ4 bit 2, max_uncompressed_page_size per largest plane)

- [ ] **Step 5: Update decoding algorithm**

Update section 7 (Decoding Algorithm) step 7 to read plane directory from page index entry instead of single blob.

- [ ] **Step 6: Commit**

```bash
cd /var/home/tristan/Documents/parasquid/binbook
git add BINBOOK_FORMAT_SPEC.md
git commit -m "spec: per-plane blob format (76→128 byte page index entry)"
```

---

## Task 2: Update Python crate constants and structs

**Files:**
- Modify: `/var/home/tristan/Documents/parasquid/binbook/binbook/constants.py`
- Modify: `/var/home/tristan/Documents/parasquid/binbook/binbook/structs.py`
- Test: `/var/home/tristan/Documents/parasquid/binbook/tests/test_structs.py`

- [ ] **Step 1: Write the failing test for new PageIndexEntry size**

In `tests/test_structs.py`, add:
```python
def test_page_index_entry_size_is_128():
    from binbook.structs import PAGE_INDEX_ENTRY_SIZE
    assert PAGE_INDEX_ENTRY_SIZE == 128
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /var/home/tristan/Documents/parasquid/binbook && python -m pytest tests/test_structs.py::test_page_index_entry_size_is_128 -v`
Expected: FAIL (PAGE_INDEX_ENTRY_SIZE is 76)

- [ ] **Step 3: Update constants.py**

Add new constants:
```python
PAGE_INDEX_ENTRY_SIZE_V2 = 128
# Keep old for migration
PAGE_INDEX_ENTRY_SIZE_LEGACY = 76

# RGB pixel formats
class PixelFormat:
    GRAY1 = 1
    GRAY2 = 2
    GRAY4 = 4
    RGB565 = 8
    RGB888 = 16
    RGBA8888 = 32
```

Update `PAGE_INDEX_ENTRY_SIZE` to 128.

- [ ] **Step 4: Update PageIndexEntry in structs.py**

Replace the `_PAGE_INDEX` struct and `PageIndexEntry` dataclass. Remove `relative_blob_offset`, `compressed_size`, `uncompressed_size`. Add:
```python
@dataclass
class PlaneDir:
    bitmap: int
    compression: list[int]  # 4 methods, one per plane
    offsets: list[int]      # 4 offsets into PAGE_DATA
    sizes: list[int]        # 4 compressed sizes

@dataclass
class PageIndexEntry:
    page_number: int
    page_kind: int
    pixel_format: int
    compression_method: int
    update_hint: int
    page_flags: int
    page_crc32: int
    stored_width: int
    stored_height: int
    placement_x: int
    placement_y: int
    source_spine_index: int
    chapter_nav_index: int
    progress_start_ppm: int
    progress_end_ppm: int
    plane_dir: PlaneDir
```

Update `pack()` and `unpack()` methods for 128-byte layout.

- [ ] **Step 5: Write roundtrip test**

In `tests/test_structs.py`, add:
```python
def test_page_index_entry_roundtrip_128():
    from binbook.structs import PageIndexEntry, PlaneDir, PAGE_INDEX_ENTRY_SIZE
    plane = PlaneDir(
        bitmap=0x07,
        compression=[1, 1, 2, 0],
        offsets=[0, 1000, 2000, 0],
        sizes=[500, 500, 800, 0],
    )
    entry = PageIndexEntry(
        page_number=0, page_kind=1, pixel_format=2,
        compression_method=2, update_hint=0, page_flags=1,
        page_crc32=0, stored_width=480, stored_height=480,
        placement_x=0, placement_y=0,
        source_spine_index=0xFFFFFFFF, chapter_nav_index=0,
        progress_start_ppm=0, progress_end_ppm=500000,
        plane_dir=plane,
    )
    data = entry.pack()
    assert len(data) == PAGE_INDEX_ENTRY_SIZE
    restored = PageIndexEntry.unpack(data)
    assert restored.page_number == 0
    assert restored.plane_dir.bitmap == 0x07
    assert restored.plane_dir.compression[2] == 2
    assert restored.plane_dir.offsets[1] == 1000
    assert restored.plane_dir.sizes[2] == 800
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd /var/home/tristan/Documents/parasquid/binbook && python -m pytest tests/test_structs.py -v`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
cd /var/home/tristan/Documents/parasquid/binbook
git add binbook/constants.py binbook/structs.py tests/test_structs.py
git commit -m "feat: 128-byte page index entry with plane directory"
```

---

## Task 3: Update Python crate reader

**Files:**
- Modify: `/var/home/tristan/Documents/parasquid/binbook/binbook/reader.py`
- Test: `/var/home/tristan/Documents/parasquid/binbook/tests/test_roundtrip.py`

- [ ] **Step 1: Write failing test for per-plane decode**

In `tests/test_roundtrip.py`, add:
```python
def test_per_plane_decode(tmp_path):
    from binbook.writer import build_binbook
    from binbook.reader import BinBookReader
    from binbook.structs import PlaneDir
    # Build a 2-page binbook with per-plane blobs
    # Page 0: MSB+LSB+BW planes, Page 1: BW only
    # ... (generate test data)
    out = build_binbook(pages, profile)
    (tmp_path / "test.binbook").write_bytes(out)
    reader = BinBookReader.from_file(tmp_path / "test.binbook")
    page0 = reader.pages[0]
    assert page0.plane_dir.bitmap == 0x07
    page1 = reader.pages[1]
    assert page1.plane_dir.bitmap == 0x04
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /var/home/tristan/Documents/parasquid/binbook && python -m pytest tests/test_roundtrip.py::test_per_plane_decode -v`
Expected: FAIL (old format doesn't have plane_dir)

- [ ] **Step 3: Update reader.py**

In `reader.py`:
- Update `_read_pages` to use new 128-byte entry size
- Update page blob bounds check to validate plane directory offsets
- Add `decode_plane(page_number, plane_slot)` method that reads a single plane blob
- Update `decode_page_bytes` to reconstruct full page from planes (for backwards-compatible full-page decode)

Key change in `_read_pages`:
```python
def _read_pages(data, sections, header):
    section = sections.get(SectionId.PAGE_INDEX)
    if section is None:
        return []
    return [
        PageIndexEntry.unpack(data, section.offset + index * section.entry_size)
        for index in range(section.record_count)
    ]
```

Add plane decode method:
```python
def decode_plane(self, page_number: int, plane_slot: int) -> bytes:
    page = self.pages[page_number]
    pd = page.plane_dir
    if plane_slot > 3 or pd.offsets[plane_slot] == 0:
        raise ValueError(f"plane {plane_slot} not present")
    absolute = self.header.page_data_offset + pd.offsets[plane_slot]
    compressed = self.data[absolute : absolute + pd.sizes[plane_slot]
    method = pd.compression[plane_slot] if page.page_flags & 1 else page.compression_method
    return decode_compressed(compressed, method)
```

- [ ] **Step 4: Run tests**

Run: `cd /var/home/tristan/Documents/parasquid/binbook && python -m pytest tests/test_roundtrip.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /var/home/tristan/Documents/parasquid/binbook
git add binbook/reader.py tests/test_roundtrip.py
git commit -m "feat: per-plane blob decoding in reader"
```

---

## Task 4: Update Python crate writer

**Files:**
- Modify: `/var/home/tristan/Documents/parasquid/binbook/binbook/writer.py`

- [ ] **Step 1: Update `_page_index` to write plane directory**

Replace the `_page_index` function to:
1. Accept pages with per-plane compressed data (list of `EncodedPage` with `planes: list[bytes]`)
2. Build `PlaneDir` with bitmap, compression methods, offsets, and sizes
3. Pack into 128-byte `PageIndexEntry`

```python
def _page_index(pages, profile):
    out = bytearray()
    blob_offset = 0
    for index, page in enumerate(pages):
        plane_offsets = []
        plane_sizes = []
        for plane_data in page.planes:
            plane_offsets.append(blob_offset)
            plane_sizes.append(len(plane_data))
            blob_offset += len(plane_data)
        # Pad to 4-byte alignment
        while blob_offset % 4 != 0:
            blob_offset += 1
        plane_dir = PlaneDir(
            bitmap=page.plane_bitmap,
            compression=page.plane_compression,
            offsets=plane_offsets,
            sizes=plane_sizes,
        )
        out.extend(PageIndexEntry(..., plane_dir=plane_dir).pack())
    return bytes(out)
```

- [ ] **Step 2: Update `build_binbook` to write per-plane PAGE_DATA**

Update PAGE_DATA assembly to concatenate plane blobs (not single compressed blobs), with 4-byte padding between pages.

- [ ] **Step 3: Update `EncodedPage` dataclass**

Add fields: `planes: list[bytes]`, `plane_bitmap: int`, `plane_compression: list[int]`

- [ ] **Step 4: Run full test suite**

Run: `cd /var/home/tristan/Documents/parasquid/binbook && python -m pytest tests/ -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /var/home/tristan/Documents/parasquid/binbook
git add binbook/writer.py
git commit -m "feat: per-plane blob writing in writer"
```

---

## Task 5: Update Rust crate page index parser

**Files:**
- Modify: `/var/home/tristan/Documents/parasquid/binbook/rust/src/page_index.rs`
- Modify: `/var/home/tristan/Documents/parasquid/binbook/rust/src/lib.rs`

- [ ] **Step 1: Update page_index.rs**

Replace 76-byte parser with 128-byte parser. Update `PageInfo` struct:
```rust
pub const PAGE_INDEX_ENTRY_SIZE: usize = 128;

pub struct PlaneDir {
    pub bitmap: u8,
    pub compression: [u8; 4],
    pub offsets: [u32; 4],
    pub sizes: [u32; 4],
}

pub struct PageInfo {
    pub page_number: u32,
    pub page_kind: u16,
    pub pixel_format: u16,
    pub compression_method: u16,
    pub page_flags: u32,
    pub page_crc32: u32,
    pub stored_width: u16,
    pub stored_height: u16,
    pub placement_x: u16,
    pub placement_y: u16,
    pub chapter_nav_index: i32,
    pub progress_start_ppm: u32,
    pub progress_end_ppm: u32,
    pub plane_dir: PlaneDir,
}
```

Update `parse_page_info_from_bytes` to read 128 bytes at new offsets.

- [ ] **Step 2: Update lib.rs page() and decompress_page()**

Update `page()` to read individual plane blobs instead of single blob.
Update `decompress_page()` to support per-plane compression dispatch.

- [ ] **Step 3: Run Rust tests**

Run: `cd /var/home/tristan/Documents/parasquid/binbook && cargo test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
cd /var/home/tristan/Documents/parasquid/binbook
git add rust/src/page_index.rs rust/src/lib.rs
git commit -m "feat: 128-byte page index with plane directory in Rust crate"
```

---

## Task 6: Update SquidScript FFI and firmware struct

**Files:**
- Modify: `/var/home/tristan/Documents/parasquid/SquidScript/compiler/rust/crates/squidvm-ffi/src/lib.rs`
- Modify: `/var/home/tristan/Documents/parasquid/SquidScript/firmware/zephyr/src/vm_runtime.h`

- [ ] **Step 1: Update RustBinBookPageMeta in squidvm-ffi**

Replace the struct at `lib.rs:6095`:
```rust
#[repr(C)]
pub struct RustBinBookPageMeta {
    pub ok: bool,
    pub pixel_format: u16,
    pub compression_method: u16,
    pub page_flags: u32,
    pub stored_width: u16,
    pub stored_height: u16,
    pub plane_bitmap: u8,
    pub plane_compression: [u8; 4],
    pub offset_plane_0: u32,
    pub size_plane_0: u32,
    pub offset_plane_1: u32,
    pub size_plane_1: u32,
    pub offset_plane_2: u32,
    pub size_plane_2: u32,
    pub offset_plane_3: u32,
    pub size_plane_3: u32,
}
```

- [ ] **Step 2: Update rust_binbook_page_meta FFI function**

At `lib.rs:6155`, change hardcoded `76` to `page_index_entry_size` (passed as parameter or read from header). Parse plane directory fields into the new struct.

- [ ] **Step 3: Update sq_vm_runtime_binbook_page in vm_runtime.h**

Replace the struct at `vm_runtime.h:272`:
```c
struct sq_vm_runtime_binbook_page {
    char path[SQ_APP_STORE_PATH_MAX];
    uint32_t page_index;
    uint16_t pixel_format;
    uint16_t compression_method;
    uint16_t stored_width;
    uint16_t stored_height;
    uint8_t plane_bitmap;
    uint8_t per_plane_compression;
    uint8_t plane_compression[4];
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

- [ ] **Step 4: Build firmware to verify compilation**

Run: `cd /var/home/tristan/Documents/parasquid/SquidScript && cargo build -p squidvm-ffi`
Expected: PASS (with expected errors in vm_runtime_binbook.c and display driver — those are next tasks)

- [ ] **Step 5: Commit**

```bash
cd /var/home/tristan/Documents/parasquid/SquidScript
git add compiler/rust/crates/squidvm-ffi/src/lib.rs firmware/zephyr/src/vm_runtime.h
git commit -m "feat: plane directory in FFI struct and firmware page struct"
```

---

## Task 7: Update firmware page index reader

**Files:**
- Modify: `/var/home/tristan/Documents/parasquid/SquidScript/firmware/zephyr/src/vm_runtime_binbook.c`

- [ ] **Step 1: Update BINBOOK_PAGE_INDEX_ENTRY_SIZE**

Change `#define BINBOOK_PAGE_INDEX_ENTRY_SIZE 76U` to `128U` at line 8.

- [ ] **Step 2: Update runtime_binbook_read_page**

Update the function at line 606 to:
1. Read 128 bytes instead of 76
2. Call updated `rust_binbook_page_meta` with plane directory parsing
3. Copy plane directory fields into `runtime->drawable.page`

- [ ] **Step 3: Build firmware**

Run: `cd /var/home/tristan/Documents/parasquid/SquidScript && cargo build -p squidvm-ffi`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
cd /var/home/tristan/Documents/parasquid/SquidScript
git add firmware/zephyr/src/vm_runtime_binbook.c
git commit -m "feat: 128-byte page index reader in firmware"
```

---

## Task 8: Update display driver for plane-directory decompression

**Files:**
- Modify: `/var/home/tristan/Documents/parasquid/SquidScript/firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`

- [ ] **Step 1: Update decompress_binbook_gray2_to_fb signature**

Change to accept plane offset/size instead of reading from page struct:
```c
static int decompress_binbook_gray2_plane_to_fb(
    const struct sq_vm_runtime_binbook_page *page,
    uint32_t plane_offset, uint32_t plane_size,
    uint8_t compression_method, bool msb_plane)
```

- [ ] **Step 2: Update decompress_binbook_gray2_bw_to_fb similarly**

Same pattern — accept plane offset/size/compression.

- [ ] **Step 3: Update sq_display_backend_rasterize_binbook**

Replace the 3-pass flow with plane-directory-driven logic:
```c
void sq_display_backend_rasterize_binbook(const struct sq_vm_runtime_binbook_page *page)
{
    uint8_t bitmap = page->plane_bitmap;
    bool per_plane = page->per_plane_compression;

    if (refresh_mode == FAST1BPP) {
        if (bitmap & PLANE_BW) {
            uint8_t method = per_plane ? page->plane_compression[2] : page->compression_method;
            decompress_binbook_gray2_plane_to_fb(page, page->offset_plane_2, page->size_plane_2, method, false);
        }
    } else {
        if (bitmap & PLANE_MSB) {
            uint8_t method = per_plane ? page->plane_compression[0] : page->compression_method;
            decompress_binbook_gray2_plane_to_fb(page, page->offset_plane_0, page->size_plane_0, method, true);
        }
        if (bitmap & PLANE_LSB) {
            uint8_t method = per_plane ? page->plane_compression[1] : page->compression_method;
            decompress_binbook_gray2_plane_to_fb(page, page->offset_plane_1, page->size_plane_1, method, false);
        }
        if (bitmap & PLANE_BW) {
            uint8_t method = per_plane ? page->plane_compression[2] : page->compression_method;
            decompress_binbook_gray2_plane_to_fb(page, page->offset_plane_2, page->size_plane_2, method, false);
        }
    }
}
```

- [ ] **Step 4: Build firmware**

Run: `cd /var/home/tristan/Documents/parasquid/SquidScript && cargo build -p squidvm-ffi`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /var/home/tristan/Documents/parasquid/SquidScript
git add firmware/zephyr/src/ssd1677_gdeq0426t82_display.c
git commit -m "feat: plane-directory-driven decompression in display driver"
```

---

## Task 9: Update test binbook generator

**Files:**
- Modify: `/var/home/tristan/Documents/parasquid/SquidScript/scripts/generate-test-binbook-480.py`

- [ ] **Step 1: Update generator to write 128-byte entries with plane directory**

Update `build()` to:
1. Set `page_index_entry_size = 128` in header
2. Compress BW plane separately from MSB/LSB planes
3. Build `PlaneDir` per page with bitmap + offsets/sizes
4. Write plane blobs into PAGE_DATA with 4-byte alignment padding

- [ ] **Step 2: Generate test binbook and verify**

Run: `python3 scripts/generate-test-binbook-480.py /tmp/test-v2.binbook`
Verify: `python3 -c "from binbook.reader import BinBookReader; r = BinBookReader.from_file(open('/tmp/test-v2.binbook','rb').read()); print(r.pages[0].plane_dir)"`

- [ ] **Step 3: Commit**

```bash
cd /var/home/tristan/Documents/parasquid/SquidScript
git add scripts/generate-test-binbook-480.py
git commit -m "feat: generator writes 128-byte plane directory format"
```

---

## Task 10: Run protocol ztests and hardware verification

- [ ] **Step 1: Run protocol ztests**

Run: `cd /var/home/tristan/Documents/parasquid/SquidScript/firmware/zephyr/tests/protocol && cargo test`
Expected: 146/146 PASS (or same pre-existing failure count)

- [ ] **Step 2: Run binbook repo tests**

Run: `cd /var/home/tristan/Documents/parasquid/binbook && python -m pytest tests/ -v && cargo test`
Expected: all PASS

- [ ] **Step 3: Build firmware for hardware test**

Run: west build for xteink-x4 target
Expected: BUILD SUCCESS

- [ ] **Step 4: Flash and test on hardware (if available)**

Flash firmware, install test binbook, verify page turns work.

- [ ] **Step 5: Measure decompression time**

Compare `binbook_read_page` timing metrics before and after.

- [ ] **Step 6: Commit any fixes**

```bash
cd /var/home/tristan/Documents/parasquid/SquidScript
git add -A
git commit -m "fix: address test failures from per-plane format"
```
