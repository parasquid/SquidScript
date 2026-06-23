# Design: BinBook Page Turn Latency Reduction

## Purpose

Reduce binbook page-turn latency from ~1444ms to under 200ms (excluding hardware
display refresh) by eliminating repeated file open/close cycles and byte-by-byte
compressed-data reads.

## Measured Baseline

Instrumented measurements on XTEINK X4 hardware with `binbook-pager`:

| Component | Per page turn | Root cause |
|-----------|--------------|------------|
| `binbook.open()` | ~199ms | 2 file open + 2 close cycles |
| `binbook.readPage()` | ~82ms | 1 file open + 1 close cycle |
| Decompress (3 planes) | ~636ms | 170k `fs_read(1 byte)` syscalls |
| Display flush | ~527ms | Hardware e-paper refresh (not optimizable) |
| **Total dispatch** | **~1444ms** | |

The decompression bottleneck is 100% file I/O, not CPU. PackBits decompression
is negligible; the cost comes from ~170,000 individual `fs_read()` calls through
LittleFS + SPI SD.

## Constraints

- ESP32-C3 heap is constrained (~8.6KB largest free block observed). Heap
  allocations must be temporary and freed after use.
- The compressed page buffer must fall back to byte-by-byte file reads on
  allocation failure — no regression in functionality.
- The binbook file handle reuse must follow the same lifecycle invalidation
  pattern as SQBC handles (app switch, reset, book change, storage format).
- The display backend must not open binbook files directly — all file I/O
  stays in the binbook runtime layer.
- Existing 144 protocol ztests must remain green.

## Change A: Compressed Page Buffer

### Architecture

A heap-allocated buffer in `vm_runtime_binbook.c` holds the compressed page
blob during decompression. The display backend decompresses from the buffer
instead of reading the file byte-by-byte.

### Data flow

1. `runtime_binbook_read_page()` reads the 76-byte page index entry (as today).
2. It then reads the compressed blob from `page->blob_offset` into a heap
   buffer in one `fs_read()` call.
3. The buffer pointer and size are stored in `runtime->drawable.page` (new
   fields: `compressed_data`, `compressed_data_len`).
4. `sq_display_backend_rasterize_binbook()` receives a `binbook_page` with
   buffer pointer. The packbits reader init takes the buffer instead of a
   file handle.
5. All `packbits_read_raw()` calls read from the buffer — zero syscalls.
6. The buffer is freed after `sq_display_backend_rasterize_binbook()` returns.

### Buffer sizing

- Allocation size: `page->compressed_size` bytes (known from page index entry).
- Maximum observed: ~38KB. Typical: 1.5–5KB.
- Fallback: `k_malloc` failure → current byte-by-byte file reads.

### Packbits reader changes

The `packbits_reader` struct gains a buffer-backed mode:

```c
struct packbits_reader {
    /* File-backed mode (fallback) */
    struct fs_file_t file;
    /* Buffer-backed mode */
    const uint8_t *buf;
    uint32_t buf_len;
    uint32_t buf_pos;
    /* Shared state */
    uint32_t compressed_left;
    uint8_t repeat_value;
    uint8_t repeat_remaining;
    uint8_t literal_remaining;
};
```

`packbits_read_raw()` checks `reader->buf != NULL` and reads from the buffer
instead of calling `fs_read()`.

## Change B: BinBook File Handle Reuse

### Architecture

One module-owned `fs_file_t` in `vm_runtime_binbook.c` stays open for the
active binbook. All binbook operations (open, readPage, chapters, chapter)
seek through this handle instead of opening/closing the file per call.

### State

```c
static struct {
    struct fs_file_t file;
    bool is_open;
    char path[SQ_APP_STORE_PATH_MAX];
} binbook_open_file;
```

### Lifecycle

- `binbook.open()`: If `path` matches `binbook_open_file.path`, reuse.
  Otherwise close old handle, open new path, store in `binbook_open_file`.
- `binbook.readPage()`: Seek to blob offset through open handle.
- `binbook.chapters()` / `binbook.chapter()`: Seek through open handle.
- `binbook_open_file_close()`: Idempotent close, called at lifecycle
  boundaries (app switch, reset, book change, storage format).

### Expected impact

- `binbook.open()` on reused path: ~0ms (no file operations)
- `binbook.readPage()`: ~0ms (seek only through open handle)
- Total binbook I/O savings: ~281ms per page turn

## Testing Strategy

### TDD approach

Every change starts with a failing test, then implementation, then verification.

### Protocol ztests

- Existing 144 tests must remain green after every commit.
- Add tests for:
  - Compressed buffer allocation + fallback on allocation failure.
  - File handle reuse across multiple reads.
  - File handle release at lifecycle boundaries.
  - Buffer-backed packbits reader produces identical output to file-backed.

### Hardware measurement

After each slice, measure on XTEINK X4:

```sh
cargo run -p squidc -- device key RIGHT --port /dev/ttyACM0
cargo run -p squidc -- device resources --port /dev/ttyACM0 | \
  grep -E 'dispatch_us|binbook_open|binbook_read_page|display_flush'
```

Record: `last_binbook_open_us`, `last_binbook_read_page_us`,
`last_display_flush_us`, `last_dispatch_us`.

### Acceptance criteria

| Metric | Before | Target |
|--------|--------|--------|
| `last_binbook_open_us` | ~199ms | <10ms (reused handle) |
| `last_binbook_read_page_us` | ~82ms | <10ms (seek only) |
| Decompress time | ~636ms | <50ms (buffer-backed) |
| Total dispatch | ~1444ms | <200ms (excl. display flush) |

## Out of Scope

- Changing binbook file format or compression algorithm.
- Pre-decoding pages into an in-memory pixel buffer.
- Changing display refresh semantics or hardware e-paper timing.
- The page ring (3-slot prefetch cache) — separate roadmap item.
