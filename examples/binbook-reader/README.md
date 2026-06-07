# BinBook Reader Example

Minimal BinBook display app.

The app opens the package resource at `books/sample.binbook`, reads the current
zero-based page index, and draws the returned drawable handle with
`service.display.draw(page.drawable)`.

Current Zephyr SSD1677 firmware support expects target-native full-panel GRAY2
BinBook page data and streams it from the resource file into the controller's
two display planes without allocating a full-screen framebuffer.
