# BinBook Reader Example

Minimal BinBook display app.

The app opens the package resource at `books/sample.binbook`, reads the current
zero-based page index, and draws the returned drawable handle with
`service.display.draw(page.drawable)`.

Current Zephyr SSD1677 firmware support expects target-native full-panel GRAY2
BinBook page data and thresholds it to 1-bit e-paper output while streaming
from the resource file.
