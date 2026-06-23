# Framebuffer Display Pipeline Design

## Problem

The current display pipeline collects high-level ops (clear, text, rect) into a fixed-size array (`display_ops[48]`), then rasterizes them row-by-row at flush time. This prevents pixel-level operations, in-memory graphics calculations, and efficient buffer management. The op array and double-buffering statics consume ~12 KiB of static RAM.

## Goal

Replace the op-based pipeline with a 1bpp framebuffer. All draws rasterize directly into a pixel buffer. At flush, the buffer is sent to the display via SPI. This enables:
- Pixel-level operations (setPixel, getPixel)
- In-memory graphics calculations
- Simpler, faster flush (DMA/SPI the buffer)
- Target-specific buffer sizing

## Scope

Replaces the display op collection and row-by-row rasterizer in the SSD1677 e-paper driver. Affects:
- `vm_runtime.h` — remove display ops from `sq_vm_runtime`
- `vm_runtime_display.c` — rasterize into framebuffer instead of appending ops
- `vm_runtime.c` — simplified flush job
- `ssd1677_gdeq0426t82_display.c` — new buffer-based driver
- `vm_runtime_display_backend.h` — new interface
- Protocol tests — update display-related tests
- Runtime limits — add framebuffer size per target

Does not affect:
- Browser simulator (already has its own rendering)
- Compiler/VM core (display builtins unchanged)

## Architecture

### Buffer Ownership

The framebuffer is static in the display driver file, not in `sq_vm_runtime`. Each target defines its own buffer size. Targets without displays define size 0.

```c
// ssd1677_gdeq0426t82_display.c
static uint8_t fb_framebuffer[PANEL_WIDTH * PANEL_HEIGHT / 8]; // 48,000 bytes
```

### New Display Backend Interface

```c
// Rasterize operations directly into the framebuffer
void sq_display_backend_rasterize_clear(sq_display_color_t color);
void sq_display_backend_rasterize_text(const char *text, int32_t x, int32_t y,
                                       int32_t font_height, sq_display_color_t color);
void sq_display_backend_rasterize_rect(int32_t x, int32_t y, int32_t w, int32_t h,
                                       sq_display_color_t fill, sq_display_color_t stroke);

// Flush the framebuffer to hardware
void sq_display_backend_flush_framebuffer(enum sq_vm_runtime_display_refresh_mode mode);

// Get framebuffer pointer and size (for runtime init)
const uint8_t *sq_display_backend_framebuffer(void);
size_t sq_display_backend_framebuffer_size(void);
```

Old interface (`sq_display_backend_flush(ops, op_count, ...)`) is removed.

### Runtime Changes

Remove from `sq_vm_runtime`:
- `display_ops[SQ_VM_RUNTIME_DISPLAY_OP_MAX]`
- `display_op_count`
- `display_dirty` (replaced by a simpler dirty flag)

Add to `sq_vm_runtime`:
- `uint8_t *framebuffer` — pointer to driver's buffer
- `size_t framebuffer_size` — size of buffer
- `bool display_needs_flush` — set after any rasterize call

### Flush Lifecycle

```
screen("name") body executes
  -> runtime_display_clear() -> sq_display_backend_rasterize_clear()
  -> runtime_display_text()  -> sq_display_backend_rasterize_text()
  -> runtime_display_rect()  -> sq_display_backend_rasterize_rect()
  -> sets runtime->display_needs_flush = true

dispatch completes
  -> runtime_flush_display_if_dirty()
  -> checks display_needs_flush
  -> copies framebuffer pointer + refresh mode to display job
  -> spawns display worker thread
  -> clears display_needs_flush, runtime resumes immediately

display worker thread
  -> calls sq_display_backend_flush_framebuffer(mode)
  -> SPI transfers buffer to e-paper RAM
  -> e-paper refresh (full, no differential)
  -> worker thread returns
```

### Display Worker Thread

Kept as-is. The display worker thread handles the SPI transfer and e-paper refresh. The only change is what it receives: a framebuffer pointer instead of an ops array.

### Differential Refresh

Dropped entirely. No `previous_framebuffer`, no comparison logic. Full refresh only. All differential refresh state machines (`sq_ssd1677_composed_dirty_window`, etc.) are removed. Can be re-added later with a `previous_framebuffer` static and byte-by-byte comparison.

### Display Op Types

The `sq_vm_runtime_display_op` struct and `enum sq_vm_runtime_display_op_kind` are removed. The rasterize functions handle pixel rendering directly.

The `display_refresh_mode` is preserved but simplified. With no differential refresh, `fast1bpp` and `full` both do a full e-paper refresh. The mode is passed through for future use and for the e-paper panel's own refresh strategy selection.

## Memory Budget

### Before

| Component | Size |
|-----------|-----:|
| `sq_vm_runtime` (includes display_ops[48]) | ~29 KiB |
| Display driver statics (previous_composed_ops, sorted_ops) | ~10 KiB |
| **Total display-related** | **~39 KiB** |

### After

| Component | Size |
|-----------|-----:|
| `sq_vm_runtime` (no display ops) | ~21 KiB |
| Display driver fb_framebuffer | 48 KiB |
| **Total display-related** | **~69 KiB** |
| **Net change** | **+30 KiB** |

The 30 KiB increase is justified by enabling pixel-level operations and simpler flush. The 8 KiB saved from `sq_vm_runtime` partially offsets the increase.

### Heap Impact

The framebuffer is static (BSS), not from the heap. It does not affect `CONFIG_HEAP_MEM_POOL_SIZE` or heap free bytes. The heap pool increase (64 KiB -> 128 KiB) is a separate optimization for app-level dynamic allocation.

## Files to Modify

### 1. `firmware/zephyr/src/vm_runtime.h`

- Remove `display_ops[]`, `display_op_count` fields from `struct sq_vm_runtime`
- Add `uint8_t *framebuffer`, `size_t framebuffer_size`, `bool display_needs_flush`
- Remove `SQ_VM_RUNTIME_DISPLAY_OP_MAX` constant (or repurpose)
- Remove `struct sq_vm_runtime_display_op` and related enums

### 2. `firmware/zephyr/src/vm_runtime_display.c`

- Replace `runtime_display_append_op()` calls with `sq_display_backend_rasterize_*()` calls
- Each handler (clear, text, rect) rasterizes directly into the framebuffer
- Remove op eviction logic (memmove when buffer full)
- Set `runtime->display_needs_flush = true` after any rasterize call

### 3. `firmware/zephyr/src/vm_runtime.c`

- Simplify `struct sq_vm_runtime_display_flush_job`: remove ops array, keep refresh_mode
- `runtime_flush_display_if_dirty()`: send framebuffer pointer instead of copying ops
- `runtime_display_flush_worker()`: call `sq_display_backend_flush_framebuffer()`
- Remove op copy logic from flush path

### 4. `firmware/zephyr/src/ssd1677_gdeq0426t82_display.c`

- Add `static uint8_t fb_framebuffer[FB_FRAMEBUFFER_SIZE]`
- Implement `sq_display_backend_rasterize_clear()`: memset buffer
- Implement `sq_display_backend_rasterize_text()`: render bitmap font into buffer
- Implement `sq_display_backend_rasterize_rect()`: render filled/stroked rect into buffer
- Implement `sq_display_backend_flush_framebuffer()`: SPI transfer buffer to e-paper
- Remove `stream_composed_1bpp_frame()`, `render_row()`, `draw_text_row()`, `draw_rect_row()`
- Remove `previous_composed_ops[]`, `sorted_ops[]` statics
- Remove differential refresh state machines

### 5. `firmware/zephyr/src/vm_runtime_display_backend.h`

- Replace function signatures with new rasterize/flush interface
- Remove old `sq_display_backend_flush()` signature

### 6. `firmware/zephyr/tests/protocol/src/main.c`

- Update mock backend: replace `sq_display_backend_flush` mock with rasterize mocks
- Update `test_vm_runtime_records_physical_display_*` tests: verify rasterize calls instead of op recording
- Update `test_display_op_buffer_*` tests: remove or rewrite for framebuffer model
- Update `test_display_refresh_mode_*` tests: verify refresh mode passes through to framebuffer flush
- Update concurrency test: verify framebuffer flush is non-blocking
- Update SSD1677 compositor tests: rewrite for buffer-based rendering

### 7. `firmware/zephyr/runtime_limits.json`

- Add `"framebufferBytes"` field per target (48000 for SSD1677 800x480, 0 for no-display targets)

### 8. Target JSON files

- Add framebuffer metadata to `xteink-x4.target.json` and other display targets

## Test Strategy

### TDD Approach

Write failing tests first, then implement.

### Test Layers

1. **Firmware ztests (protocol)** — Primary test layer
   - Rasterize functions produce correct pixel output in framebuffer
   - Flush sends buffer to display worker
   - Refresh mode passes through correctly
   - Concurrency: flush is non-blocking for input dispatch
   - No display ops array exists (compile-time check)

2. **Display driver unit tests** — SSD1677-specific
   - `rasterize_clear` fills buffer correctly
   - `rasterize_text` renders bitmap font at correct coordinates
   - `rasterize_rect` renders filled and stroked rectangles
   - Buffer bounds checking (no out-of-bounds writes)
   - SPI transfer sends complete buffer

3. **Integration test** — Grid cursor app
   - Run grid-cursor example on hardware
   - Verify display renders correctly
   - Verify input responsiveness

### Test Fixtures

- `display-primitives.squid` — Updated to test framebuffer rendering
- `display-framebuffer.squid` — New fixture for pixel-level operations

## Risks

1. **RAM increase** — 30 KiB net increase in static BSS. May require increasing `CONFIG_HEAP_MEM_POOL_SIZE` or reducing other statics.
2. **Binbook path** — Binbook rendering currently streams from SD card. It needs to either render into the framebuffer or keep its own path. Initial implementation keeps binbook path separate.
3. **Display refresh timing** — Buffer-based flush may change timing characteristics. Grid cursor latency should be measured before and after.
