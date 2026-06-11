# BinBook Reader Example

Minimal BinBook display and page-turn app.

The app opens the first entry from `content.binbook.list("books")`, reads the
current zero-based page index, and draws the returned drawable handle with
`service.display.draw(page.drawable)`. `key.RIGHT` and `key.DOWN` advance one
page; `key.LEFT` and `key.UP` go back one page. Page indexes clamp at the first
and last page.

Current Zephyr SSD1677 firmware support expects target-native full-panel GRAY2
BinBook page data and streams it from the resource file without allocating a
full-screen framebuffer. Cadence refreshes use true 4-gray output; intermediate
page turns may use thresholded black/white partial refreshes for faster, less
flashy redraws. The SSD1677 backend streams the old and new pages from storage
for partial turns rather than retaining a full framebuffer in RAM.

Hardware-independent event testing can dispatch logical keys directly:

```sh
cargo run -p squidc -- device key RIGHT
cargo run -p squidc -- device key DOWN
cargo run -p squidc -- device key LEFT
cargo run -p squidc -- device key UP
```
