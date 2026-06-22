# BinBook Rust Crate Extraction Design

Status: approved

## Goal

Extract the BinBook binary format parser from the C firmware module
(`firmware/zephyr/src/vm_runtime_binbook.c`) into a reusable, `no_std` Rust crate
in the external `github.com/parasquid/binbook` repo. The crate is consumed as a
git submodule in SquidScript.

Motivation:
- Host-side testability without Zephyr or hardware.
- Reusable outside SquidScript (other firmware targets, tooling, validation).
- Replace ~600 lines of manual C struct parsing with safe Rust.

## Crate Characteristics

- **`no_std` core** — no alloc, no std, no feature flags required for firmware use.
- **Optional `alloc` feature** — adds `Vec<u8>`-returning convenience methods
  (e.g. `page.to_pixels()`).
- **Optional `std` feature** — adds `std::error::Error` impl on the error type,
  `std::io::Read`-based file opening helpers.
- **Slice-based** — all parsing operates on `&[u8]`. The caller provides the
  entire file contents. No I/O abstraction traits.
- **Read-only** — parse, validate, decompress. No write/builder API.
- **Zero-copy where possible** — string fields return `&[u8]` slices into the
  input. No string allocation.

## Compression Support

Per-page compression method, selected by the `compression_method` field in the
page index entry:

| Value | Method        | Status      |
|-------|---------------|-------------|
| 0     | NONE          | Implemented |
| 1     | RLE_PACKBITS  | Implemented |
| 2     | LZ4           | New         |
| 3     | DELTA_LZ4     | Future      |

The crate handles dispatch in `decompress_page` by matching on the page's
compression method.

### LZ4

Uses `lz4_flex` crate (no_std compatible). Decompresses into a caller-provided
`&mut [u8]` buffer. Single-pass, ~4KB working memory.

### Delta encoding (future roadmap)

Delta pages store an XOR diff against the previous page, then LZ4-compress the
result. Keyframe pages (full compressed pages) appear at a configurable interval
to enable random access. This is a format extension requiring a version bump or
new compression method constant. Not part of this extraction.

## Public API

```rust
// --- Error ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidMagic,
    UnsupportedVersion,
    InvalidHeader,
    MissingSection(u16),
    InvalidSection,
    InvalidPageIndex,
    InvalidNavIndex,
    InvalidChapterIndex,
    InvalidStringRef,
    PageOutOfRange,
    NavOutOfRange,
    ChapterOutOfRange,
    UnsupportedPixelFormat(u16),
    UnsupportedCompression(u16),
    DecompressFailed,
    OutputBufferTooSmall,
}

impl core::fmt::Display for Error { /* ... */ }

#[cfg(feature = "std")]
impl std::error::Error for Error { /* ... */ }

// --- Structs ---

pub struct BinBook<'a> {
    data: &'a [u8],
    header: Header,
    string_table: Section,
    page_index: Section,
    nav_index: Section,
    chapter_index: Section,
    page_data: Section,
}

pub struct Header {
    pub file_size: u64,
    pub section_table_offset: u64,
    pub section_count: u16,
}

pub struct Section {
    pub section_id: u16,
    pub offset: u64,
    pub length: u64,
    pub entry_size: u32,
    pub record_count: u32,
}

pub struct Info<'a> {
    pub title: &'a [u8],
    pub subtitle: &'a [u8],
    pub author: &'a [u8],
    pub publisher: &'a [u8],
    pub language: &'a [u8],
    pub series_name: &'a [u8],
    pub series_index_milli: u32,
    pub page_count: u32,
    pub nav_count: u32,
    pub chapter_count: u32,
    pub logical_width: u16,
    pub logical_height: u16,
    pub physical_width: u16,
    pub physical_height: u16,
    pub logical_to_physical_rotation: u16,
    pub default_pixel_format: u16,
    pub compression: u16,
}

pub struct PageInfo {
    pub page_number: u32,
    pub page_kind: u16,
    pub pixel_format: u16,
    pub compression_method: u16,
    pub stored_width: u16,
    pub stored_height: u16,
    pub placement_x: u16,
    pub placement_y: u16,
    pub progress_start_ppm: u32,
    pub progress_end_ppm: u32,
    pub chapter_nav_index: i32,
}

pub struct PageRef<'a> {
    pub info: PageInfo,
    compressed_data: &'a [u8],
    uncompressed_size: usize,
}

pub struct NavEntry<'a> {
    pub nav_index: u32,
    pub nav_type: u16,
    pub level: u16,
    pub title: &'a [u8],
    pub rendered_page_number: u32,
    pub parent_nav_index: i32,
    pub first_child_nav_index: i32,
    pub next_sibling_nav_index: i32,
}

pub struct ChapterEntry<'a> {
    pub index: u32,
    pub title: &'a [u8],
    pub page_index: u32,
    pub level: u16,
    pub entry_type: u16,
}

pub struct ChapterIter<'a> { /* ... */ }
impl<'a> Iterator for ChapterIter<'a> {
    type Item = Result<ChapterEntry<'a>, Error>;
}

// --- Core API ---

impl<'a> BinBook<'a> {
    /// Parse and validate a .binbook from a byte slice.
    pub fn open(data: &'a [u8]) -> Result<Self, Error>;

    /// Book metadata from BOOK_METADATA, DISPLAY_PROFILE, etc.
    pub fn info(&self) -> Result<Info<'a>, Error>;

    pub fn page_count(&self) -> u32;
    pub fn nav_count(&self) -> u32;
    pub fn chapter_count(&self) -> u32;

    /// Page metadata + compressed blob reference.
    pub fn page(&self, index: u32) -> Result<PageRef<'a>, Error>;

    pub fn page_info(&self, index: u32) -> Result<PageInfo, Error>;
    pub fn nav_entry(&self, index: u32) -> Result<NavEntry<'a>, Error>;
    pub fn chapter(&self, index: u32) -> Result<ChapterEntry<'a>, Error>;
    pub fn chapters(&self, offset: u32, limit: u32) -> Result<ChapterIter<'a>, Error>;
}

/// Decompress a page into a caller-provided buffer.
///
/// Dispatches on page.compression_method:
///   0 (NONE) — memcpy
///   1 (RLE_PACKBITS) — PackBits decode
///   2 (LZ4) — lz4_flex decompress
///
/// Returns Error::OutputBufferTooSmall if out is shorter than uncompressed_size.
pub fn decompress_page(page: &PageRef<'_>, out: &mut [u8]) -> Result<(), Error>;

// --- alloc convenience (behind "alloc" feature) ---

#[cfg(feature = "alloc")]
impl<'a> PageRef<'a> {
    /// Decompress into a new Vec<u8>.
    pub fn to_pixels(&self) -> Result<Vec<u8>, Error>;
}

// --- std convenience (behind "std" feature) ---

#[cfg(feature = "std")]
impl BinBook<'static> {
    /// Open a .binbook file from a filesystem path.
    pub fn open_file(path: &std::path::Path) -> Result<Self, Error>;
}
```

## Decompression Model

`decompress_page` takes a `&mut [u8]` output buffer. No heap allocation in the
core path. The caller provides a buffer of at least `page.uncompressed_size`
bytes.

On firmware, the display backend provides its existing page buffer. On host,
tests allocate a `Vec<u8>` (or use the `alloc`-gated `to_pixels()`).

Output buffer size validation: `decompress_page` checks `out.len() >=
page.uncompressed_size` before decompressing, returns
`OutputBufferTooSmall` if insufficient.

## Integration Path

### Firmware (replaces vm_runtime_binbook.c)

1. The git submodule is checked out under the SquidScript workspace.
2. `squidvm-ffi` adds `binbook` as a dependency (default features = no_std).
3. The C callbacks (`runtime_binbook_open`, `runtime_binbook_info`,
   `runtime_binbook_read_page`, etc.) are rewritten in Rust inside
   `squidvm-ffi`. They call `BinBook::open()` on the file bytes read via
   Zephyr's `fs_read`/`fs_seek` into a RAM buffer, or on flash-mapped memory.
4. `runtime_binbook_read_page` calls `decompress_page` into the display
   backend's page buffer.
5. The display backend (`ssd1677_gdeq0426t82_display.c`) keeps only
   tiling/rotation/panel conversion. Its PackBits decompressor is removed.
6. `vm_runtime_binbook.c` is deleted.
7. `generated_runtime_callbacks.inc` is regenerated — the callback function
   signatures stay the same, only the implementation moves to Rust.

### Host tooling

1. `squidc-cli` depends on `binbook` with `features = ["std"]`.
2. CLI can inspect, validate, and dump .binbook files using the Rust API.
3. Test fixtures are created by calling the Rust API directly (replacing
   `generate-test-binbook.py` over time).

### Content listing

`content.binbook.list("books", ...)` stays firmware-side. It scans the app
store filesystem for .binbook files — a filesystem concern, not a format concern.

## Testing Strategy

- **Unit tests in the binbook crate:** parse header, section table, string
  table, page/nav/chapter index, decompress RLE/LZ4/NONE pages, error cases
  (bad magic, truncated file, unsupported compression, buffer too small).
- **Fixture tests:** use `generate-test-binbook.py` output as golden fixtures.
  Verify the Rust parser produces expected metadata and decompressed pixels.
- **Round-trip fuzzing:** generate random valid binbook structures, parse them,
  verify no panics.
- **FFI dispatch tests in squidvm-ffi:** verify the Rust callbacks produce the
  same results as the old C implementation for the test fixtures.
- **Hardware tests:** flash firmware with Rust binbook module, run
  `binbook-reader` example app on XIAO e-paper target, verify page rendering.

## File Layout (binbook repo)

```
binbook/
├── Cargo.toml
├── BINBOOK_FORMAT_SPEC.md
├── src/
│   ├── lib.rs
│   ├── header.rs
│   ├── section.rs
│   ├── string_table.rs
│   ├── page_index.rs
│   ├── nav_index.rs
│   ├── chapter_index.rs
│   ├── decompress.rs
│   ├── rle.rs
│   └── lz4.rs
├── tests/
│   ├── fixtures/
│   │   └── sample.binbook
│   └── integration.rs
└── README.md
```

## Roadmap Items

- Delta encoding (DELTA_LZ4) with implicit reference page + document-level
  keyframe interval. Format extension, not part of initial extraction.
- `binbook-builder` crate for creating .binbook files in Rust (replaces
  `generate-test-binbook.py`).
- Writer/builder API in a separate crate to keep the reader crate focused.
