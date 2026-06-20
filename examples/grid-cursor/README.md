# Grid Cursor Example

A 5×5 grid of cells with distinct shapes per row and a cursor that inverts the
highlighted cell's colors. Designed to isolate and visualize the SSD1677 fast
`fast1bpp` differential refresh path.

## What It Tests

- **Ghost clearing**: when the cursor moves, the old cell's shape must restore
  correctly (no inverted remnants).
- **Shape restoration**: each row has a different shape type (filled rect,
  outlined rect, text "A", text "X", small filled rect). The cursor passes
  over all of them, verifying the differential refresh handles mixed content.
- **Lifecycle reset**: after device reset and app relaunch, the first refresh
  is a full seed (no stale differential against the pre-reset frame).

## Controls

- `key.UP` / `key.DOWN` / `key.LEFT` / `key.RIGHT` — move cursor
- `key.BACK` — exit app

## Running

```sh
cargo run -p squidc -- app build examples/grid-cursor/main.squid --out target/grid-cursor.sqbc
cargo run -p squidc -- app install target/grid-cursor.sqbc --port <serial-port>
cargo run -p squidc -- app launch grid-cursor --port <serial-port>
```

On XTEINK X4 hardware, the full flash + install + drive + verify flow is
automated by `scripts/xteink-x4-test-grid-cursor.sh`.

## Grid Layout

The grid is 400×400 with 80×80 cells, positioned at x=40, y=200 on the 480×800
logical display. Each row uses a different shape type:

| Row | Shape | Description |
| --- | --- | --- |
| 0 | Filled rect | 64×64 filled square centered in cell |
| 1 | Outlined rect | 72×72 stroked border centered in cell |
| 2 | Text "A" | 48px glyph |
| 3 | Text "X" | 48px glyph |
| 4 | Small filled rect | 40×40 filled square centered in cell |

The cursor cell draws a black background then redraws the shape in white,
inverting the colors. When the cursor moves away, the differential refresh
must restore the original shape.
