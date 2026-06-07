# BinBook Reader Example

Minimal BinBook display app.

The app opens the package resource at `books/sample.binbook`, reads the current
zero-based page index, and draws the returned drawable handle with
`service.display.draw(page.drawable)`.

Current Zephyr SSD1677 firmware support expects target-native full-panel GRAY2
BinBook page data and streams it from the resource file without allocating a
full-screen framebuffer. Cadence refreshes use true 4-gray output; intermediate
page turns may use thresholded black/white partial refreshes for faster, less
flashy redraws. The SSD1677 backend streams the old and new pages from storage
for partial turns rather than retaining a full framebuffer in RAM.
