# BinBook Reader Example

Full XTEINK BinBook reader app.

The app lists `.binbook` files from `content.binbook.list("books")`, opens the
selected content ref with `binbook.open(...)`, renders pages with
`service.display.draw(page.drawable)`, and keeps firmware-owned BinBook handles
transient. Uploaded books can arrive through serial, HTTP, or BLE transfer
tools, but this reader app does not start upload services itself.

Launch behavior:

- If the app was interrupted while reading, it resumes the selected book and page.
- If the last saved view was the library, menu, chapters, or an invalid/missing
  book, it starts in the library.
- Progress is stored for the current selected book only. Per-book reading
  history belongs in a future storage-backed API.

Controls:

- In the library: `key.UP` / `key.DOWN` move selection, `key.SELECT` opens the
  highlighted book, and `key.BACK` exits the app.
- In the reader: `key.RIGHT` / `key.DOWN` advance a page, `key.LEFT` /
  `key.UP` go back a page, and `key.SELECT` or `key.BACK` opens the reader menu.
- In the reader menu: choose Continue, Chapters, Library, or Exit.
- In chapters: `key.UP` / `key.DOWN` move selection, `key.SELECT` jumps to the
  chapter, and `key.BACK` returns to the reader menu.

Current Zephyr SSD1677 firmware support expects target-native full-panel GRAY2
BinBook page data and streams it from the resource file without allocating a
full-screen framebuffer. The reader keeps the page screen on the full GRAY2
refresh path for clean book content. Library, menu, and chapter screens request
`fast1bpp` so selection highlights can move through the SSD1677 differential
partial path. The firmware still redraws these screens from app state; it
retains previous composed frame state only as a private display optimization.

Hardware-independent event testing can dispatch logical keys directly:

```sh
cargo run -p squidc -- device key RIGHT
cargo run -p squidc -- device key DOWN
cargo run -p squidc -- device key LEFT
cargo run -p squidc -- device key UP
cargo run -p squidc -- device key SELECT
cargo run -p squidc -- device key BACK
```
