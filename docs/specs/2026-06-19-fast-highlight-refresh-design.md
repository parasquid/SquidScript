# Fast Highlight Refresh Design

## Scope

The XTEINK BinBook reader should be able to move menu and chapter highlights
without using a full flash-style e-paper refresh for every selection change.
The SquidScript language contract does not change: `screen.refresh()` reruns
the current screen from app state and produces a fresh draw-command stream.
Firmware may keep private previous-frame state only as a rendering
optimization.

This design covers the Zephyr SSD1677 display backend used by the XTEINK X4
path, the firmware runtime display-op model needed by that backend, and the
BinBook reader's refresh-mode choices for selection screens. It does not add
new SquidScript syntax, SQBC opcodes, app-visible retained layers, framebuffer
mutation APIs, or compatibility behavior for old APIs.

## Rendering Model

The firmware runtime should record `service.display.rect(...)` as a physical
display operation, not only as a drawlog entry. A display flush then contains
the ordered commands emitted by the screen render pass:

- `clear`
- `rect`
- `text`
- `binbook drawable`

The SSD1677 backend should compose these operations in source order into a
1bpp row stream for fast refreshes. White-like colors (`white`, `gray0`) map to
white; black-like colors (`black`, `gray15`) map to black. Other grayscale
values may be thresholded to the nearest 1bpp value in the fast path. Full GRAY2
BinBook page refreshes remain the quality path for reading content.

`rect` support must cover the highlight shape used by the reader:

- `fillColor` fills the rectangle interior.
- `strokeColor` draws the rectangle border.
- a rectangle with only `strokeColor` leaves its interior unchanged.
- clipping keeps negative or out-of-bounds coordinates inside the logical
  display.

Text rendering remains the existing bitmap-font renderer for the SSD1677
firmware path.

## Previous-Frame State

Fast partial refreshes need both the old and new physical pixels. The backend
should retain the previous successful composed draw-command stream, not a
full-screen framebuffer. On each fast 1bpp flush:

1. If no previous composed state is valid, render the current state with the
   clean full-refresh path and retain it after success.
2. If previous state is valid, stream the previous composed 1bpp rows into the
   SSD1677 previous/RED RAM plane.
3. Stream the current composed 1bpp rows into the current/BW RAM plane.
4. Trigger the SSD1677 partial activation.
5. Replace the retained previous state only after the flush succeeds.

The retained state is invalidated when the backend switches away from the
SSD1677 black/white mode, when a full refresh is forced and fails, when
display configuration fails, or when the operation list cannot be represented
by the 1bpp compositor.

The existing BinBook page differential path already retains enough page
metadata to re-decode the previous and current pages. The generic composed
state should handle overlay and menu cases. The implementation may keep the
existing page-only optimized path for pure page turns, but mixed draw streams
such as `clear`, `draw(page)`, `rect`, and `text` must not silently ignore the
overlay operations in fast mode.

## Reader Behavior

The BinBook reader should request fast 1bpp refreshes only for screens where
selection chrome is the primary changing visual state:

- library
- reader menu
- chapters

The page reading screen should keep the current full/auto GRAY2 refresh policy
so page content remains clean and readable.

The reader should continue to draw highlights with normal SquidScript display
commands. The app must not manually erase old highlights or depend on a
retained scene graph. Event handlers update state and call `screen.refresh()`;
screen bodies redraw from state.

## Acceptance Checks

Host-testable acceptance:

- `runtime_display_rect` appends a physical display op with dimensions and
  colors.
- the SSD1677 1bpp compositor renders clear, rect, text, and BinBook drawable
  operations in source order.
- moving a stroked highlight from one row to another produces old and new
  1bpp row streams that include the unhighlighted old row and highlighted new
  row.
- fast refresh without valid previous state falls back to a clean full refresh
  and seeds retained state.
- fast refresh with valid previous state uses the differential partial path.
- failed flushes do not replace retained previous state.

Hardware-script acceptance for the implementation slice:

- the BinBook reader page drawlog still reports a full-quality page refresh.
- library/menu/chapter selection navigation reports `fast1bpp` refresh
  requests.
- `device errors` is empty after the scripted flow.
- resource metrics remain available.

Final optical acceptance is deferred to an interactive XTEINK session. The
roadmap item should remain open, or be replaced with a concise validation
entry, until highlight movement has been visually checked on the real panel for
full-flash behavior and unacceptable ghosting.

