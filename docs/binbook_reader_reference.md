# Draft Reference Implementation: BinBook Reader

Status: Draft
Purpose: Show what a practical SquidScript BinBook reader might look like using app-owned persistent state and the `binbook.*` standard domain capability.

Draft source files are available under:

```text
examples/binbook-reader/
```

This example is intentionally limited to reading and navigation:

- first-screen BinBook browser
- resume last book from app state
- page forward/back
- coarse page movement
- table of contents navigation
- jump-to-page

It does not include dictionaries, annotations, highlighting, search, bookmarks, or background indexing.

---

## App Layout

```text
/sd/apps/binbook-reader/
|-- main.sqbc
|-- source-map.json
|-- main.squid
|-- screens/
|   |-- browser.squid
|   |-- reader.squid
|   |-- toc.squid
|   `-- jump.squid
`-- lib/
    `-- ui.squid
```

---

## Persistent App State

The reader uses firmware-managed SquidScript app state for resume data. It stores only small serializable values such as the current file path, page index, cached title, and UI selection.

```squid
state {
  stateVersion: int = 1
  file: string = ""
  title: string = ""
  pageIndex: int = 0
  pageCount: int = 0
  navCount: int = 0
  tocIndex: int = 0
  tocTop: int = 0
  jumpPage: int = 1
  browserIndex: int = 0
  uiState: string = "browser"
}
```

Nonvolatile resume state is stored by:

```squid
state.save()
```

and restored by:

```squid
state.load()
```

The firmware owns the storage location and atomic write behavior. The app persists only small serializable values: file path, page index, cached title/counts, UI selection, and current UI state.

---

## Startup Flow

The first screen is always the app browser.

```squid
event.on("app.start") {
  state.load()
  openBrowser()
}
```

If `file` is empty, the browser shows only `Browse for BinBook`.

If `file` is non-empty, the browser also shows `Resume`, using `title`, `pageIndex`, and `pageCount` from app state. Selecting resume refreshes the metadata from the file and opens the reader at the saved page.

---

## Browser Actions

The browser is app-owned UI. It uses the firmware file picker only when the user chooses to browse.

```squid
function browseForBook() {
  let picked = content.pickFile(".binbook")

  if (picked != "") {
    state.file = picked
    state.pageIndex = 0
    state.tocIndex = 0
    state.tocTop = 0
    state.jumpPage = 1
    loadBookInfo()
    state.save()
    openReader()
  }
}

function resumeBook() {
  if (state.file != "") {
    loadBookInfo()
    state.save()
    openReader()
  }
}
```

This keeps resume behavior inside the BinBook reader's own app state.

---

## Navigation Model

`uiState` is a persisted string used by `stateMachine.*` to route input. The state machine is backed by that normal app state variable, so explicit assignments to `state.uiState` and calls to `stateMachine.enter("uiState", "...")` affect the same source of truth.

```squid
event.on("key.SELECT") {
  handleSelect()
}

function handleSelect() {
  if (stateMachine.is("uiState", "reader")) {
    openJump()
  } else {
    if (stateMachine.is("uiState", "toc")) {
      openSelectedTocEntry()
    } else {
      if (stateMachine.is("uiState", "jump")) {
        commitJump()
      } else {
        if (stateMachine.is("uiState", "browser")) {
          openSelectedBrowserItem()
        }
      }
    }
  }
}
```

On XTEINK X4, the seventh logical key is `POWER`, not `MENU`. The reference app may use a short foreground `POWER` press as a menu/TOC shortcut when firmware policy allows app-visible power key events. Long-press sleep and wake behavior remains firmware-owned.

The main views are:

- `browser`: choose Browse or Resume
- `reader`: read the current page
- `toc`: choose a navigation entry
- `jump`: adjust a page number and jump to it

---

## Reader Screen

The reader screen composes BinBook page rendering through `service.display.draw` and declares `render: "stream"` because it is page-image-dominant. Browser, TOC, and jump screens omit `render`, so they use the target default policy.

```squid
screen("reader", { render: "stream" }) {
  service.display.clear("white")

  let book = binbook.open(state.file)
  let page = binbook.page(book, state.pageIndex)
  let image = binbook.pageImage(page)

  service.display.draw(image, { x: 0, y: 0 })

  drawBottomBar(string.format("{}/{}", state.pageIndex + 1, state.pageCount))
}
```

The app stores `file` and `pageIndex`, not BinBook handles or decoded page buffers.

---

## TOC And Chapter Navigation

The TOC uses the draft BinBook capability contract:

```squid
binbook.navCount(book)
binbook.navEntry(book, navIndex)
```

The reference implementation uses a bounded chapter scan:

```squid
repeat (32) {
  if (scan < state.navCount) {
    let entry = binbook.navEntry(book, scan)

    if (entry.renderedPageNumber > state.pageIndex) {
      state.tocIndex = scan
      setPage(entry.renderedPageNumber)
      return
    }

    scan = scan + 1
  }
}
```

The limit keeps event work bounded. A future capability helper could replace this if chapter navigation becomes common enough to justify it.

---

## Source Files

The draft implementation is split into:

- `examples/binbook-reader/main.squid`
- `examples/binbook-reader/lib/ui.squid`
- `examples/binbook-reader/screens/browser.squid`
- `examples/binbook-reader/screens/reader.squid`
- `examples/binbook-reader/screens/toc.squid`
- `examples/binbook-reader/screens/jump.squid`

These files are the reference source for this example. The snippets above explain the key design choices but are not a separate implementation.

---

## Notes For The Spec

This example intentionally pressures a few current draft design choices:

- It uses app-owned persistent state for resume instead of a separate library/recent-books capability.
- It uses `content.pickFile(".binbook")` as the browse action because the
  current draft does not expose direct directory enumeration.
- It uses `stateMachine.*` backed by a `uiState` string to route key handlers without requiring a `screen.current()` built-in or hidden state-machine storage.
- It uses `binbook.navCount(book)` and `binbook.navEntry(book, index)` from the draft capability contract.
- It treats read-only BinBook operations as render-safe so screens can open, resolve, and draw a page without storing handles in persistent state.
- It uses bounded chapter scans instead of unbounded search.
- It avoids dictionaries, annotations, highlighting, and arbitrary document mutation.
