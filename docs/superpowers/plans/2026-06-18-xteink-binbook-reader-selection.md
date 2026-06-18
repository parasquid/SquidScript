# XTEINK BinBook Reader Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Promote `examples/binbook-reader` into the first full XTEINK reader app with library selection, reader menu, chapter navigation, and interrupted-reading resume using current SquidScript APIs.

**Architecture:** Keep this app-only. The reader stores current app state with `state.load()` / `state.save()`, uses `content.binbook.list("books")` for file-name library rows, reopens the selected `binbook.open(ref)` handle for each event/render, and never persists firmware handles. Upload/transfer remains separate tooling that populates the `books` content library. Do not work around missing SquidScript/compiler/firmware functionality in this app; add the missing capability as part of the feature when that is the right design and ask the user if the boundary is unclear.

**Tech Stack:** SquidScript app source, Rust CLI app-test/package/build tooling, Bash hardware wrapper, XTEINK X4 Zephyr serial protocol.

---

## File Structure

- Modify: `examples/binbook-reader/main.squid` - full reader app state machine and screens.
- Modify: `examples/binbook-reader/README.md` - product flow, controls, resume semantics, hardware-independent key-driving examples.
- Create: `examples/app-tests/xteink/binbook-reader-selection/main.squid` - small portable app-test fixture that proves state/view selection logic without hardware.
- Create: `examples/app-tests/xteink/binbook-reader-selection/test.session` - app-test commands for compile/run expectations.
- Create: `scripts/xteink-x4-test-binbook-reader.sh` - real XTEINK hardware acceptance check.
- Modify: `scripts/tests/test_zephyr_hardware_suite.py` - static checks for the new hardware wrapper.
- Modify: `docs/hardware_target_tests.md` - document the XTEINK reader selection hardware check.
- Modify: `docs/language_spec.md` - align the documented runtime string limit with `compiler/rust/crates/squidvm-limits/src/lib.rs`.
- Modify: `ROADMAP.md` - remove the completed BinBook selection item and add the two agreed follow-ups after inspecting any pre-existing user edits.

## Completed Prerequisite: Display Rect And Program String Budgets

These support changes were completed before implementing the reader flow:

- `service.display.rect(...)` accepts expression coordinates, including variables such as `service.display.rect(18, y, 424, 48, ...)`.
- Program string table limits were doubled in `squidvm-limits`: `MAX_STRINGS = 128` and `MAX_PROGRAM_STRING_BYTES = 1536`.
- The whole-app SQBC cap was raised to 8 KiB and the Zephyr VM load scratch buffer was raised with it so the first full reader app can load without shrinking behavior to fit the old demo-sized container budget.
- The reader app should use normal helper functions for repeated row rendering instead of duplicating literal coordinate blocks to work around old rect parser behavior.

## Task 1: Reader State Machine App

**Files:**
- Modify: `examples/binbook-reader/main.squid`
- Test: `cargo run -p squidc -- app build examples/binbook-reader/main.squid --out target/binbook-reader.sqbc`

- [ ] **Step 1: Write the target app source as the failing implementation target**

Replace `examples/binbook-reader/main.squid` with this complete source. This is intentionally app-only and uses no new builtins. When implementing, refactor the repeated row drawing blocks into helper functions that take variable `y` coordinates; rect expression coordinates are now supported and should be used directly.

```squid
app "binbook-reader"

state {
  view: int = 0
  selectedBookRef: string = ""
  selectedBookName: string = ""
  pageIndex: int = 0
  libraryTop: int = 0
  librarySelected: int = 0
  menuSelected: int = 0
  chapterTop: int = 0
  chapterSelected: int = 0
  chapterCount: int = 0
}

event.on("app.start") {
  state.load()
  if (state.view == 1) {
    if (savedBookStillOpens()) {
      openReader()
    } else {
      resetToLibrary()
    }
  } else {
    openLibrary()
  }
}

event.on("key.RIGHT") {
  if (state.view == 1) {
    nextPage()
  }
}

event.on("key.DOWN") {
  if (state.view == 1) {
    nextPage()
  } else {
    if (state.view == 0) {
      nextLibraryBook()
    } else {
      if (state.view == 2) {
        nextMenuItem()
      } else {
        if (state.view == 3) {
          nextChapter()
        }
      }
    }
  }
}

event.on("key.LEFT") {
  if (state.view == 1) {
    previousPage()
  }
}

event.on("key.UP") {
  if (state.view == 1) {
    previousPage()
  } else {
    if (state.view == 0) {
      previousLibraryBook()
    } else {
      if (state.view == 2) {
        previousMenuItem()
      } else {
        if (state.view == 3) {
          previousChapter()
        }
      }
    }
  }
}

event.on("key.SELECT") {
  if (state.view == 1) {
    openMenu()
  } else {
    if (state.view == 0) {
      selectLibraryBook()
    } else {
      if (state.view == 2) {
        chooseMenuItem()
      } else {
        if (state.view == 3) {
          jumpToSelectedChapter()
        }
      }
    }
  }
}

event.on("key.BACK") {
  if (state.view == 1) {
    openMenu()
  } else {
    if (state.view == 2) {
      openReader()
    } else {
      if (state.view == 3) {
        openMenu()
      } else {
        if (state.view == 0) {
          state.save()
          app.exit()
        }
      }
    }
  }
}

function resetToLibrary() {
  state.view = 0
  state.selectedBookRef = ""
  state.selectedBookName = ""
  state.pageIndex = 0
  state.menuSelected = 0
  state.chapterTop = 0
  state.chapterSelected = 0
  state.chapterCount = 0
  state.save()
  screen.open("library")
}

function openLibrary() {
  state.view = 0
  state.save()
  screen.open("library")
}

function openReader() {
  state.view = 1
  state.save()
  screen.open("reader")
}

function openMenu() {
  state.view = 2
  state.menuSelected = 0
  state.save()
  screen.open("menu")
}

function openChapters() {
  loadChapterCount()
  state.view = 3
  state.chapterSelected = 0
  state.chapterTop = 0
  state.save()
  screen.open("chapters")
}

function savedBookStillOpens() {
  if (state.selectedBookRef == "") {
    return false
  }
  let opened = binbook.open(state.selectedBookRef)
  if (opened.ok) {
    let info = binbook.info(opened.book)
    if (info.ok) {
      if (state.pageIndex >= info.pageCount) {
        state.pageIndex = info.pageCount - 1
      }
      if (state.pageIndex < 0) {
        state.pageIndex = 0
      }
      return true
    }
  }
  return false
}

function nextLibraryBook() {
  let listing = content.binbook.list("books", { offset: state.libraryTop, limit: 5 })
  if (listing.ok) {
    if (state.libraryTop + state.librarySelected + 1 < listing.count) {
      if (state.librarySelected < 4) {
        state.librarySelected = state.librarySelected + 1
      } else {
        state.libraryTop = state.libraryTop + 1
      }
      state.save()
      screen.refresh()
    }
  }
}

function previousLibraryBook() {
  if (state.librarySelected > 0) {
    state.librarySelected = state.librarySelected - 1
    state.save()
    screen.refresh()
  } else {
    if (state.libraryTop > 0) {
      state.libraryTop = state.libraryTop - 1
      state.save()
      screen.refresh()
    }
  }
}

function selectLibraryBook() {
  let listing = content.binbook.list("books", { offset: state.libraryTop, limit: 5 })
  if (listing.ok) {
    for item in listing.items max 5 {
      if (item.name != "") {
        if (item.name == selectedLibraryName(item.name, item.ref)) {
          state.selectedBookRef = item.ref
          state.selectedBookName = item.name
          state.pageIndex = 0
          state.chapterTop = 0
          state.chapterSelected = 0
          state.chapterCount = 0
          if (savedBookStillOpens()) {
            openReader()
          } else {
            state.view = 0
            state.save()
            screen.refresh()
          }
        }
      }
    }
  }
}

function selectedLibraryName(name, ref) {
  let listing = content.binbook.list("books", { offset: state.libraryTop, limit: 5 })
  let index = state.libraryTop
  for item in listing.items max 5 {
    if (index == state.libraryTop + state.librarySelected) {
      return item.name
    }
    index = index + 1
  }
  return ""
}

function nextMenuItem() {
  if (state.menuSelected < 3) {
    state.menuSelected = state.menuSelected + 1
    state.save()
    screen.refresh()
  }
}

function previousMenuItem() {
  if (state.menuSelected > 0) {
    state.menuSelected = state.menuSelected - 1
    state.save()
    screen.refresh()
  }
}

function chooseMenuItem() {
  if (state.menuSelected == 0) {
    openReader()
  } else {
    if (state.menuSelected == 1) {
      openChapters()
    } else {
      if (state.menuSelected == 2) {
        openLibrary()
      } else {
        state.view = 0
        state.save()
        app.exit()
      }
    }
  }
}

function nextPage() {
  if (state.selectedBookRef != "") {
    let opened = binbook.open(state.selectedBookRef)
    if (opened.ok) {
      let info = binbook.info(opened.book)
      if (info.ok) {
        if (state.pageIndex + 1 < info.pageCount) {
          state.pageIndex = state.pageIndex + 1
          state.save()
          screen.refresh()
        }
      }
    }
  }
}

function previousPage() {
  if (state.pageIndex > 0) {
    state.pageIndex = state.pageIndex - 1
    state.save()
    screen.refresh()
  }
}

function loadChapterCount() {
  state.chapterCount = 0
  if (state.selectedBookRef != "") {
    let opened = binbook.open(state.selectedBookRef)
    if (opened.ok) {
      let info = binbook.info(opened.book)
      if (info.ok) {
        state.chapterCount = info.chapterCount
      }
    }
  }
}

function nextChapter() {
  if (state.chapterSelected + 1 < state.chapterCount) {
    state.chapterSelected = state.chapterSelected + 1
    if (state.chapterSelected >= state.chapterTop + 5) {
      state.chapterTop = state.chapterTop + 1
    }
    state.save()
    screen.refresh()
  }
}

function previousChapter() {
  if (state.chapterSelected > 0) {
    state.chapterSelected = state.chapterSelected - 1
    if (state.chapterSelected < state.chapterTop) {
      state.chapterTop = state.chapterTop - 1
    }
    state.save()
    screen.refresh()
  }
}

function jumpToSelectedChapter() {
  if (state.selectedBookRef != "") {
    let opened = binbook.open(state.selectedBookRef)
    if (opened.ok) {
      let chapter = binbook.chapter(opened.book, state.chapterSelected)
      if (chapter.ok) {
        state.pageIndex = chapter.pageIndex
        openReader()
      }
    }
  }
}

function drawLibraryRow0(name) {
  if (state.librarySelected == 0) {
    service.display.rect(18, 76, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(name, { x: 28, y: 76, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

function drawLibraryRow1(name) {
  if (state.librarySelected == 1) {
    service.display.rect(18, 128, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(name, { x: 28, y: 128, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

function drawLibraryRow2(name) {
  if (state.librarySelected == 2) {
    service.display.rect(18, 180, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(name, { x: 28, y: 180, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

function drawLibraryRow3(name) {
  if (state.librarySelected == 3) {
    service.display.rect(18, 232, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(name, { x: 28, y: 232, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

function drawLibraryRow4(name) {
  if (state.librarySelected == 4) {
    service.display.rect(18, 284, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(name, { x: 28, y: 284, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

function drawMenuRow0(label) {
  if (state.menuSelected == 0) {
    service.display.rect(18, 76, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(label, { x: 28, y: 76, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

function drawMenuRow1(label) {
  if (state.menuSelected == 1) {
    service.display.rect(18, 128, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(label, { x: 28, y: 128, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

function drawMenuRow2(label) {
  if (state.menuSelected == 2) {
    service.display.rect(18, 180, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(label, { x: 28, y: 180, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

function drawMenuRow3(label) {
  if (state.menuSelected == 3) {
    service.display.rect(18, 232, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(label, { x: 28, y: 232, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

function drawChapterRow0(index, title) {
  if (state.chapterSelected == index) {
    service.display.rect(18, 76, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(title, { x: 28, y: 76, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

function drawChapterRow1(index, title) {
  if (state.chapterSelected == index) {
    service.display.rect(18, 128, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(title, { x: 28, y: 128, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

function drawChapterRow2(index, title) {
  if (state.chapterSelected == index) {
    service.display.rect(18, 180, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(title, { x: 28, y: 180, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

function drawChapterRow3(index, title) {
  if (state.chapterSelected == index) {
    service.display.rect(18, 232, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(title, { x: 28, y: 232, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

function drawChapterRow4(index, title) {
  if (state.chapterSelected == index) {
    service.display.rect(18, 284, 424, 48, { strokeColor: "gray15" })
  }
  service.display.text(title, { x: 28, y: 284, w: 408, h: 48, fontHeight: 20, valign: "middle", textColor: "gray15" })
}

screen("library") {
  service.display.clear("white")
  service.display.refreshMode("fast1bpp")
  service.display.text("Library", { x: 24, y: 28, fontHeight: 28, textColor: "gray15" })
  let listing = content.binbook.list("books", { offset: state.libraryTop, limit: 5 })
  if (listing.ok) {
    if (listing.count == 0) {
      service.display.text("No books", { x: 24, y: 96, fontHeight: 24, textColor: "gray15" })
      service.display.text("Use transfer", { x: 24, y: 144, fontHeight: 18, textColor: "gray15" })
      debug.print("lib0", listing.warning, listing.count)
    } else {
      let index = state.libraryTop
      for item in listing.items max 5 {
        if (index == state.libraryTop) {
          drawLibraryRow0(item.name)
        }
        if (index == state.libraryTop + 1) {
          drawLibraryRow1(item.name)
        }
        if (index == state.libraryTop + 2) {
          drawLibraryRow2(item.name)
        }
        if (index == state.libraryTop + 3) {
          drawLibraryRow3(item.name)
        }
        if (index == state.libraryTop + 4) {
          drawLibraryRow4(item.name)
        }
        index = index + 1
      }
      debug.print("lib", listing.count, state.libraryTop, state.librarySelected, listing.hasMore)
    }
  } else {
    service.display.text("No library", { x: 24, y: 96, fontHeight: 24, textColor: "gray15" })
    debug.print("liberr", listing.error, listing.warning)
  }
}

screen("menu") {
  service.display.clear("white")
  service.display.refreshMode("fast1bpp")
  service.display.text(state.selectedBookName, { x: 24, y: 28, w: 432, h: 36, fontHeight: 20, textColor: "gray15" })
  drawMenuRow0("Continue")
  drawMenuRow1("Chapters")
  drawMenuRow2("Library")
  drawMenuRow3("Exit")
  debug.print("menu", state.selectedBookName, state.pageIndex, state.menuSelected)
}

screen("reader") {
  service.display.clear("white")
  if (state.selectedBookRef == "") {
    service.display.text("Select a book", { x: 24, y: 64, fontHeight: 24, textColor: "gray15" })
  } else {
    let opened = binbook.open(state.selectedBookRef)
    if (opened.ok) {
      let info = binbook.info(opened.book)
      let page = binbook.readPage(opened.book, state.pageIndex)
      if (page.ok) {
        service.display.draw(page.drawable)
        debug.print("read", state.selectedBookName, state.pageIndex, info.pageCount)
      } else {
        service.display.text("Page failed", { x: 24, y: 64, fontHeight: 24, textColor: "gray15" })
        debug.print("pagerr", page.error, state.pageIndex)
      }
    } else {
      service.display.text("Open failed", { x: 24, y: 64, fontHeight: 24, textColor: "gray15" })
      debug.print("openerr", state.selectedBookName, opened.error)
    }
  }
}

screen("chapters") {
  service.display.clear("white")
  service.display.refreshMode("fast1bpp")
  service.display.text("Chapters", { x: 24, y: 28, fontHeight: 28, textColor: "gray15" })
  if (state.chapterCount == 0) {
    service.display.text("No chapters", { x: 24, y: 96, fontHeight: 24, textColor: "gray15" })
  } else {
    let opened = binbook.open(state.selectedBookRef)
    if (opened.ok) {
      let chapters = binbook.chapters(opened.book, { offset: state.chapterTop, limit: 5 })
      if (chapters.ok) {
        for chapter in chapters.items max 5 {
          if (chapter.index == state.chapterTop) {
            drawChapterRow0(chapter.index, chapter.title)
          }
          if (chapter.index == state.chapterTop + 1) {
            drawChapterRow1(chapter.index, chapter.title)
          }
          if (chapter.index == state.chapterTop + 2) {
            drawChapterRow2(chapter.index, chapter.title)
          }
          if (chapter.index == state.chapterTop + 3) {
            drawChapterRow3(chapter.index, chapter.title)
          }
          if (chapter.index == state.chapterTop + 4) {
            drawChapterRow4(chapter.index, chapter.title)
          }
        }
        debug.print("chapters", chapters.count, state.chapterTop, state.chapterSelected)
      } else {
        service.display.text("Chapter failed", { x: 24, y: 96, fontHeight: 24, textColor: "gray15" })
        debug.print("chaperr", chapters.error)
      }
    }
  }
}
```

- [ ] **Step 2: Build the app**

Run:

```bash
cargo run -p squidc -- app build examples/binbook-reader/main.squid --out target/binbook-reader.sqbc
```

Expected: PASS.

- [ ] **Step 3: Run format check**

Run:

```bash
cargo run -p squidc -- fmt --check examples/binbook-reader
```

Expected: PASS. If it fails with formatting diffs, run `cargo run -p squidc -- fmt examples/binbook-reader` and inspect the diff.

- [ ] **Step 4: Commit**

Use escalated git index commands in this repo.

```bash
git add examples/binbook-reader/main.squid
git commit -m "feat: add BinBook reader selection flow"
```

## Task 2: Reader Docs

**Files:**
- Modify: `examples/binbook-reader/README.md`

- [ ] **Step 1: Replace the README with current-state docs**

Use this content:

```markdown
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

Hardware-independent event testing can dispatch logical keys directly:

```sh
cargo run -p squidc -- device key RIGHT
cargo run -p squidc -- device key DOWN
cargo run -p squidc -- device key LEFT
cargo run -p squidc -- device key UP
cargo run -p squidc -- device key SELECT
cargo run -p squidc -- device key BACK
```
```

- [ ] **Step 2: Verify docs mention no upload service**

Run:

```bash
rg -n "service\\.http|service\\.ble|upload service|starts upload" examples/binbook-reader/README.md
```

Expected: no misleading claim that the reader starts transfer services.

- [ ] **Step 3: Commit**

```bash
git add examples/binbook-reader/README.md
git commit -m "docs: describe BinBook reader selection flow"
```

## Task 3: Portable App Test Fixture

**Files:**
- Create: `examples/app-tests/xteink/binbook-reader-selection/main.squid`
- Create: `examples/app-tests/xteink/binbook-reader-selection/test.session`

- [ ] **Step 1: Add a minimal app-state navigation fixture**

Create `examples/app-tests/xteink/binbook-reader-selection/main.squid`:

```squid
app "binbook-reader-selection-test"

state {
  view: string = "library"
  pageIndex: int = 0
  menuSelected: int = 0
}

event.on("app.start") {
  state.load()
  if (state.view == "reader") {
    screen.open("reader")
  } else {
    screen.open("library")
  }
}

event.on("key.SELECT") {
  if (state.view == "library") {
    state.view = "reader"
    state.pageIndex = 0
    state.save()
    screen.open("reader")
  } else {
    if (state.view == "reader") {
      state.view = "menu"
      state.menuSelected = 0
      state.save()
      screen.open("menu")
    } else {
      if (state.view == "menu") {
        if (state.menuSelected == 0) {
          state.view = "reader"
          state.save()
          screen.open("reader")
        } else {
          state.view = "library"
          state.save()
          screen.open("library")
        }
      }
    }
  }
}

event.on("key.DOWN") {
  if (state.view == "reader") {
    state.pageIndex = state.pageIndex + 1
    state.save()
    screen.refresh()
  } else {
    if (state.view == "menu") {
      if (state.menuSelected < 1) {
        state.menuSelected = state.menuSelected + 1
        state.save()
        screen.refresh()
      }
    }
  }
}

event.on("key.BACK") {
  if (state.view == "reader") {
    state.view = "menu"
    state.menuSelected = 0
    state.save()
    screen.open("menu")
  } else {
    if (state.view == "menu") {
      state.view = "reader"
      state.save()
      screen.open("reader")
    }
  }
}

screen("library") {
  service.display.clear("white")
  service.display.text("library", { x: 0, y: 0, fontHeight: 20 })
  debug.print("view", state.view, state.pageIndex)
}

screen("reader") {
  service.display.clear("white")
  service.display.text("reader", { x: 0, y: 0, fontHeight: 20 })
  debug.print("view", state.view, state.pageIndex)
}

screen("menu") {
  service.display.clear("white")
  service.display.text("menu", { x: 0, y: 0, fontHeight: 20 })
  debug.print("view", state.view, state.menuSelected)
}
```

- [ ] **Step 2: Add the session**

Create `examples/app-tests/xteink/binbook-reader-selection/test.session`:

```text
:launch
:expect-output view library 0
:key SELECT
:expect-output view reader 0
:key DOWN
:expect-output view reader 1
:key BACK
:expect-output view menu 0
:key DOWN
:expect-output view menu 1
:key SELECT
:expect-output view library 1
:reset
:reload
:output
:expect-output view library 1
:key SELECT
:expect-output view reader 0
:reset
:reload
:output
:expect-output view reader 0
```

- [ ] **Step 3: Run the app test**

Run:

```bash
cargo run -p squidc -- app test examples/app-tests/xteink/binbook-reader-selection
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add examples/app-tests/xteink/binbook-reader-selection
git commit -m "test: cover BinBook reader resume state"
```

## Task 4: XTEINK Hardware Acceptance Script

**Files:**
- Create: `scripts/xteink-x4-test-binbook-reader.sh`
- Modify: `scripts/tests/test_zephyr_hardware_suite.py`

- [ ] **Step 1: Add the hardware wrapper**

Create `scripts/xteink-x4-test-binbook-reader.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

TARGET_ID="${TARGET_ID:-xteink-x4}"
APP_ID="binbook-reader"
APP_DIR="${ROOT}/examples/binbook-reader"
WORK_DIR="${ROOT}/target/hardware-tests/xteink-x4-binbook-reader"
PACKAGE="${WORK_DIR}/${APP_ID}.squid.zip"
BOOK_ONE="${BOOK_ONE:-${ROOT}/tests/hardware/xiao-esp32c3/epaper-gray2-smoke/books/sample.binbook}"
BOOK_TWO="${BOOK_TWO:-${ROOT}/tests/hardware/xiao-esp32c3/epaper-fast-redraw-smoke/books/sample.binbook}"
BOOK_ONE_NAME="${BOOK_ONE_NAME:-reader-one.binbook}"
BOOK_TWO_NAME="${BOOK_TWO_NAME:-reader-two.binbook}"
PORT="${PORT:-}"
SKIP_FLASH="${SKIP_FLASH:-0}"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-240}"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-90}"

usage() {
  cat <<'USAGE'
Usage: scripts/xteink-x4-test-binbook-reader.sh [--target <id>] [--port <serial-port>] [--skip-flash]

Flashes XTEINK firmware, installs two BinBooks into the books library, installs
the BinBook reader app, drives selection/resume/menu flows through serial
logical key events, and checks drawlog/errors.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET_ID="${2:-}"; shift 2 ;;
    --port) PORT="${2:-}"; shift 2 ;;
    --skip-flash) SKIP_FLASH=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done

mkdir -p "${WORK_DIR}"
source "${ROOT}/scripts/zephyr-env.sh"
if [[ -z "${PORT}" ]]; then
  PORT="$(resolve_esp_serial_port)"
fi
export ESPFLASH_PORT="${PORT}"

assert_file_contains() {
  local file="$1"
  local expected="$2"
  if ! grep -Fq "${expected}" "${file}"; then
    printf 'Expected %s to contain: %s\n' "${file}" "${expected}" >&2
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
}

assert_file_empty_command() {
  local file="$1"
  if [[ -s "${file}" ]]; then
    printf 'Expected %s to be empty\n' "${file}" >&2
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
}

wait_for_contains() {
  local label="$1"
  local expected="$2"
  shift 2
  local out="${WORK_DIR}/${label}.out"
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  while (( SECONDS < deadline )); do
    timeout "${COMMAND_TIMEOUT_SECONDS}s" "$@" >"${out}" 2>&1
    if grep -Fq "${expected}" "${out}"; then
      printf '%s\n' "${out}"
      return 0
    fi
    sleep 0.2
  done
  printf 'Timed out waiting for %s\n' "${expected}" >&2
  printf '%s\n' "--- ${out} ---" >&2
  sed -n '1,200p' "${out}" >&2
  exit 1
}

if [[ ! -s "${BOOK_ONE}" ]]; then
  printf 'Book fixture not found or empty: %s\n' "${BOOK_ONE}" >&2
  exit 1
fi
if [[ ! -s "${BOOK_TWO}" ]]; then
  printf 'Book fixture not found or empty: %s\n' "${BOOK_TWO}" >&2
  exit 1
fi

if [[ "${SKIP_FLASH}" != "1" ]]; then
  run_capture build-x4 cargo run --quiet -p squidc -- target build --target "${TARGET_ID}" >/dev/null
  run_capture flash-x4 cargo run --quiet -p squidc -- target flash --target "${TARGET_ID}" >/dev/null
  sleep "${POST_FLASH_SETTLE_SECONDS:-8}"
fi

run_capture reset-before-reader cargo run --quiet -p squidc -- device reset --port "${PORT}" >/dev/null
sleep "${POST_RESET_SETTLE_SECONDS:-2}"
run_capture storage-format cargo run --quiet -p squidc -- device storage-format --port "${PORT}" >/dev/null
run_capture content-put-one cargo run --quiet -p squidc -- device content-put "${BOOK_ONE}" --name "${BOOK_ONE_NAME}" --port "${PORT}" >/dev/null
run_capture content-put-two cargo run --quiet -p squidc -- device content-put "${BOOK_TWO}" --name "${BOOK_TWO_NAME}" --port "${PORT}" >/dev/null
run_capture package-reader cargo run --quiet -p squidc -- app package "${APP_DIR}" --target "${TARGET_ID}" --out "${PACKAGE}" >/dev/null
run_capture install-reader cargo run --quiet -p squidc -- app install "${PACKAGE}" --port "${PORT}" >/dev/null
run_capture launch-reader cargo run --quiet -p squidc -- app launch "${APP_ID}" --port "${PORT}" >/dev/null

library_out="$(wait_for_contains output-library "library" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${library_out}" "${BOOK_ONE_NAME}"

run_capture select-second cargo run --quiet -p squidc -- device key DOWN --port "${PORT}" >/dev/null
run_capture open-second cargo run --quiet -p squidc -- device key SELECT --port "${PORT}" >/dev/null
reader_out="$(wait_for_contains output-reader "reader ${BOOK_TWO_NAME}" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${reader_out}" "${BOOK_TWO_NAME}"

run_capture next-page cargo run --quiet -p squidc -- device key RIGHT --port "${PORT}" >/dev/null
page_out="$(wait_for_contains output-page "reader ${BOOK_TWO_NAME} 1" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${page_out}" "${BOOK_TWO_NAME}"

run_capture reset-interrupted-reader cargo run --quiet -p squidc -- device reset --port "${PORT}" >/dev/null
sleep "${POST_RESET_SETTLE_SECONDS:-2}"
run_capture relaunch-reader cargo run --quiet -p squidc -- app launch "${APP_ID}" --port "${PORT}" >/dev/null
resume_out="$(wait_for_contains output-resume "reader ${BOOK_TWO_NAME} 1" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${resume_out}" "${BOOK_TWO_NAME}"

run_capture open-menu cargo run --quiet -p squidc -- device key BACK --port "${PORT}" >/dev/null
menu_out="$(wait_for_contains output-menu "menu ${BOOK_TWO_NAME}" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${menu_out}" "${BOOK_TWO_NAME}"
run_capture menu-down-1 cargo run --quiet -p squidc -- device key DOWN --port "${PORT}" >/dev/null
run_capture menu-down-2 cargo run --quiet -p squidc -- device key DOWN --port "${PORT}" >/dev/null
run_capture menu-library cargo run --quiet -p squidc -- device key SELECT --port "${PORT}" >/dev/null
library_again_out="$(wait_for_contains output-library-again "library" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${library_again_out}" "library"

run_capture reset-from-library cargo run --quiet -p squidc -- device reset --port "${PORT}" >/dev/null
sleep "${POST_RESET_SETTLE_SECONDS:-2}"
run_capture relaunch-from-library cargo run --quiet -p squidc -- app launch "${APP_ID}" --port "${PORT}" >/dev/null
nonresume_out="$(wait_for_contains output-nonresume "library" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${nonresume_out}" "library"

drawlog_out="$(run_capture drawlog cargo run --quiet -p squidc -- device drawlog --port "${PORT}")"
assert_file_contains "${drawlog_out}" "draw=binbook"
errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors --port "${PORT}")"
assert_file_empty_command "${errors_out}"
resources_out="$(run_capture resources cargo run --quiet -p squidc -- device resources --port "${PORT}")"
assert_file_contains "${resources_out}" "protocol_stack_unused"
assert_file_contains "${resources_out}" "vm_worker_stack_unused"

printf '%s\n' 'OK XTEINK X4 BinBook reader selection hardware check passed'
```

- [ ] **Step 2: Make script executable**

Run:

```bash
chmod +x scripts/xteink-x4-test-binbook-reader.sh
```

- [ ] **Step 3: Add static script tests**

In `scripts/tests/test_zephyr_hardware_suite.py`, add a test near the other XTEINK script tests:

```python
def test_xteink_binbook_reader_script_drives_selection_and_resume(self):
    script = self.read("scripts/xteink-x4-test-binbook-reader.sh")
    self.assertIn("app package", script)
    self.assertIn("device content-put", script)
    self.assertIn("device key DOWN", script)
    self.assertIn("device key SELECT", script)
    self.assertIn("device key BACK", script)
    self.assertIn("device reset", script)
    self.assertIn("device drawlog", script)
    self.assertIn("device errors", script)
    self.assertIn("draw=binbook", script)
```

- [ ] **Step 4: Run static checks**

Run:

```bash
bash -n scripts/xteink-x4-test-binbook-reader.sh
python3 -m pytest scripts/tests/test_zephyr_hardware_suite.py -k binbook_reader
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/xteink-x4-test-binbook-reader.sh scripts/tests/test_zephyr_hardware_suite.py
git commit -m "test: add XTEINK BinBook reader hardware check"
```

## Task 5: Docs, Limits, And Roadmap

**Files:**
- Modify: `docs/hardware_target_tests.md`
- Modify: `docs/language_spec.md`
- Modify: `ROADMAP.md`

- [ ] **Step 1: Document the hardware test**

Add this paragraph in `docs/hardware_target_tests.md` near the XTEINK BinBook checks:

```markdown
`scripts/xteink-x4-test-binbook-reader.sh` is the XTEINK BinBook reader
selection and interrupted-resume hardware check. It flashes the XTEINK firmware
unless `--skip-flash` is passed, formats app/content storage for an isolated
test run, installs two `.binbook` files into the `books` library, installs
`examples/binbook-reader`, drives the library, reader, menu, and relaunch flows
with serial `device key` events, and verifies `device drawlog` contains
`draw=binbook`, `device errors` is empty, and resource metrics are available.
This proves the promoted reader app can select uploaded content-library books
and resumes only when the saved foreground view was the reader.
```

- [ ] **Step 2: Fix stale runtime string limit text**

Check:

```bash
rg -n "maximum bytes in one dynamic string|MAX_RUNTIME_STRING_BYTES" docs/language_spec.md compiler/rust/crates/squidvm-limits/src/lib.rs
```

Change the `docs/language_spec.md` bullet for maximum bytes in one dynamic string to:

```markdown
- maximum bytes in one dynamic string: 128 bytes
```

- [ ] **Step 3: Update ROADMAP carefully**

Before editing, inspect the user-modified file:

```bash
git diff -- ROADMAP.md
```

Preserve all unrelated user changes. In `ROADMAP.md`:

- Remove the completed Storage And Content item:

```markdown
- Add a reader-facing BinBook selection flow that uses the uploaded `books`
  library contents rather than a hardcoded fixture path, so the chapter list
  and page reader can naturally open books delivered by serial, HTTP, or BLE
  transfer.
```

- Add these two follow-ups under Storage And Content:

```markdown
- Add a storage-backed BinBook reading history API so the reader can remember
  per-book page positions without spending fixed app-state slots or inventing
  app-local history tables.
- Investigate BinBook library metadata caching so the reader can show titles,
  authors, page counts, and other metadata for folders of BinBooks without
  opening every document during each library render.
```

- [ ] **Step 4: Run docs checks**

Run:

```bash
rg -n "hardcoded fixture path|opens the first entry" ROADMAP.md examples/binbook-reader docs/hardware_target_tests.md
cargo run -p squidc -- fmt --check examples/binbook-reader
```

Expected:
- No stale “hardcoded fixture path” roadmap item.
- No README claim that the full reader opens only the first entry.
- Format check passes.

- [ ] **Step 5: Commit**

```bash
git add docs/hardware_target_tests.md docs/language_spec.md ROADMAP.md
git commit -m "docs: update BinBook reader roadmap and hardware docs"
```

## Task 6: Full Verification

**Files:**
- No source edits unless verification exposes a defect.

- [ ] **Step 1: Run app and script static verification**

Run:

```bash
cargo run -p squidc -- app build examples/binbook-reader/main.squid --out target/binbook-reader.sqbc
cargo run -p squidc -- fmt --check examples/binbook-reader
cargo run -p squidc -- app test examples/app-tests/xteink/binbook-reader-selection
bash -n scripts/xteink-x4-test-binbook-reader.sh
python3 -m pytest scripts/tests/test_zephyr_hardware_suite.py -k binbook_reader
```

Expected: all pass.

- [ ] **Step 2: Run the XTEINK hardware check**

This command requires escalated execution because hardware/serial/flashing commands are host-only in this repo.

Run:

```bash
scripts/xteink-x4-test-binbook-reader.sh
```

Expected final line:

```text
OK XTEINK X4 BinBook reader selection hardware check passed
```

If the board is already flashed with the correct build and only app behavior changed, this narrower run is acceptable:

```bash
scripts/xteink-x4-test-binbook-reader.sh --skip-flash
```

Do not run any other hardware/serial command in parallel with this script.

- [ ] **Step 3: Inspect final git state**

Run:

```bash
git status --short
```

Expected: only intentional files changed, or clean after commits. Any unrelated pre-existing `ROADMAP.md` changes must remain preserved.

- [ ] **Step 4: Commit verification fixes**

If verification changed source, docs, scripts, or tests, commit them:

```bash
git add examples/binbook-reader examples/app-tests/xteink/binbook-reader-selection scripts/xteink-x4-test-binbook-reader.sh scripts/tests/test_zephyr_hardware_suite.py docs/hardware_target_tests.md docs/language_spec.md ROADMAP.md
git commit -m "fix: stabilize XTEINK BinBook reader verification"
```

## Acceptance Criteria

- `examples/binbook-reader` starts in library unless saved state proves reading was interrupted in reader view.
- Selecting a book from `content.binbook.list("books")` opens that book and renders it through `service.display.draw`.
- Page turns persist the current selected book and page.
- `SELECT` and `BACK` from reader open the reader menu.
- Menu supports Continue, Chapters, Library, and Exit.
- Relaunch after reset from reader resumes the book/page.
- Relaunch after leaving reader for library starts in library.
- The full XTEINK hardware script passes on the attached device.
