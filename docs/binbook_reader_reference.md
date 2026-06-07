# BinBook Reader Draft Skeleton

Status: compileable UI skeleton for a future BinBook reader.

The source under `docs/reference/binbook-reader-draft/` intentionally avoids
unimplemented `binbook.*` calls. It exercises current SquidScript module,
state, file picker, screen, and display syntax while leaving real BinBook page
loading and navigation for the deferred `binbook.*` runtime work.

## Source Layout

```text
docs/reference/binbook-reader-draft/
|-- main.squid
|-- lib/
|   |-- chrome.squid
|   `-- ui.squid
`-- screens/
    |-- browser.squid
    |-- reader.squid
    |-- toc.squid
    `-- jump.squid
```

`main.squid` imports the current compileable UI module and opens a browser
screen on startup. `lib/ui.squid` imports the browser and reader screens. The
TOC and jump screens remain parseable draft screens, but they are not imported
by the root app until the BinBook capability is implemented.

## Current App State

The compileable skeleton stores only small serializable values:

```squid
state {
  stateVersion: int = 1
  file: string = ""
  title: string = ""
  pageIndex: int = 0
  pageCount: int = 0
  browserIndex: int = 0
  uiState: string = "b"
}
```

State is loaded on `app.start` and saved before exit or when navigation changes.
The compact `uiState` values keep the draft within current string-table limits.

## Supported Flow

- Browser view: choose browse or resume.
- Browse action: calls `file.pickFile(".bb")`, stores the selected path, and
  opens the reader.
- Reader view: displays placeholder title/page text and supports page
  forward/back through state updates and `screen.refresh()`.

The skeleton app id is `bb` to keep the example within current runtime string
limits. It is a documentation fixture, not the final BinBook reader app id.

## Deferred Work

Real BinBook support requires the future `binbook.*` capability and display
backend work. When that lands, promote the skeleton back to full reader behavior
with page metadata, page image rendering, TOC navigation, and jump-to-page
logic, then re-run:

```sh
cargo run -p squidc -- app build docs/reference/binbook-reader-draft/main.squid --out target/binbook-reader-draft.sqbc
cargo run -p squidc -- fmt --check docs/reference/binbook-reader-draft
```
