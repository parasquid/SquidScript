# Browser XTEINK X4 Simulator

Status: Draft implementation roadmap
Scope: `simulator/browser`

## Purpose

The browser simulator is a development target for SquidScript apps on the XTEINK X4 profile. It is not production firmware and it does not define a production package format.

The simulator provides:

- a React and TypeScript editor/device UI
- a Canvas-rendered XTEINK X4 display surface
- logical X4 buttons and keyboard mappings
- persistent simulated `/sd` storage
- an explicit Compile, Upload, and Run workflow
- a Rust compiler frontend path for WASM builds

## Compile, Upload, Run

The browser workflow is intentionally explicit:

1. `Compile` compiles the editor source to diagnostics plus `main.sqbc`.
2. `Upload` writes `main.sqbc` under `/sd/apps/<app-id>/`.
3. `Run` starts the installed `main.sqbc` through the shared Rust VM.

Editor draft source is saved separately from simulated `/sd`. A debug workflow may later upload `main.squid`, but source upload is non-authoritative.

## App Resource Packages

The canonical app transfer file extension is `.squid.zip`. Plain `.zip` files
are not accepted by the browser-sim package importer.

A package is a ZIP transfer file. It is never mounted lazily from the ZIP
archive. Install unpacks every package entry into a read-only app directory:

```text
/sd/apps/<app-id>/
  main.sqbc
  admin-ui/
  assets/
```

ZIP entries are package-relative paths. Installers must reject absolute paths,
empty paths, parent traversal with `..`, duplicate normalized paths, backslash
paths, dot-files or dot-directories, `.squid` source files, `source-map.json`,
existing `.squid.zip` outputs, and installer/system paths such as `sd/...` or
`system/...`. The browser-sim importer derives `<app-id>` from executable
metadata, clears any previous `/sd/apps/<app-id>/` contents, and writes the
unpacked files below that directory.

Packages require `main.sqbc`. Browser-sim executes the same SQBC bytecode
container that firmware consumes through `squidvm-core`.

Installed app discovery returns valid app records plus diagnostic records for
invalid entries under `/sd/apps/<app-id>/main.sqbc`. A corrupt SQBC executable
surfaces `E_INVALID_INSTALLED_SQBC`; an executable whose embedded app id does
not match the directory name surfaces `E_INSTALLED_APP_ID_MISMATCH`. Valid apps
remain selectable and loadable while invalid records are shown in the storage
diagnostics panel. Direct `loadInstalledApp(...)` calls stay strict and reject
the invalid app id.

Bundled package resources are read-only app resources. `.sqdevice` resources
may live at any safe package-relative path ending `.sqdevice`; package install
stores them but does not activate them. App launch/runtime binding from
top-level `device {}` activates package `.sqdevice` resources or inline
binding metadata before `event.on("app.start")`. Mutable data belongs in
app-scoped runtime state, upload staging, target libraries, or firmware-owned
active device config, not inside the installed resource tree.

Browser-side static assets and SquidScript runtime resources are separate
concepts. Static asset directory names are not reserved; `web/`, `admin-ui/`,
`static/`, and similar names are ordinary package directories. When a future
runtime service starts a static server with:

```squid
httpServer.start(..., { assets: "admin-ui" })
```

the server should mount `/sd/apps/<app-id>/admin-ui/` as the static asset root.
Browser JavaScript imports, CSS URLs, images, and `fetch("./data.json")` then
resolve as normal relative HTTP paths within that root. Static serving must
never expose files outside the selected asset root.

## Target and Layout

The simulator imports `targets/xteink-x4.target.json` for logical display
dimensions, grayscale level count, supported font heights, render policy
metadata, logical buttons, storage, and features.

The current target JSON has no physical simulator layout. Browser-sim therefore
uses an approximate shell layout in code and marks it as placeholder. A future
`squid-layout` layout file should replace the approximation.

Keyboard mapping:

- arrows: `UP`, `DOWN`, `LEFT`, `RIGHT`
- `Enter`: `SELECT`
- `Backspace`: `BACK`
- `Tab`: `POWER`

## Runtime Semantics

The runtime executes SQBC through `squidvm-core` with browser host services for
screen redraw, display refresh, app-scoped state under `/sd/system/app-state`,
logical input, and app exit.

Rendering uses redraw-from-state semantics. Canvas rendering preserves source order, clips to the logical display, maps to the target's 16-level grayscale palette, and chooses deterministic font heights from the target definition.

### Simulated Wi-Fi And Host Network

Browser-sim may simulate `service.wifi.*` state so apps can exercise AP
lifecycle logic, status records, and teardown behavior. It does not create a
real Wi-Fi access point, change the host machine's network, bind LAN ports from
the web page, or prove radio behavior.

When an app starts `service.wifi.startAP(...)`, browser-sim records a simulated
foreground AP with the requested SSID and conventional development AP address
metadata such as `192.168.4.1`. That state is for SquidScript behavior tests and
UI previews only.

Future browser-sim HTTP/static-server work should reuse the browser's existing
origin or the local development server as a host-network transport. That
transport may preview how an AP-hosted admin UI would behave, but it must be
presented as a simulator transport rather than as a real SoftAP.

Button outcomes are mutually exclusive:

- short press fires on release
- long press fires at the configured threshold and suppresses short press
- chord fires when a second key is pressed inside the chord window and suppresses component keys

## Debug Logging

Browser-sim includes an in-app debug log and mirrors the same events to `console.debug`.

Current event scopes:

- `compile`: compile start, backend, diagnostics count, app id
- `upload`: app install writes to simulated `/sd`
- `run`: executable load and runtime start
- `input`: logical key dispatch and resulting runtime state
- `state`: app-state reset
- `storage`: simulated `/sd` reset

The log is intended for simulator/runtime debugging, not for app-visible behavior.

## WASM Compiler Build

The browser app loads generated WASM from `src/compiler/wasm/` through the
hand-written `src/compiler/squidWasm.ts` bridge. Compile status reports
`Compiler: WASM`; if the generated module is unavailable, compilation fails
with a diagnostic.

Build the WASM package from `simulator/browser`:

```bash
npm run wasm:build
```

Requirements:

- `wasm-pack`
- a Rust toolchain with the `wasm32-unknown-unknown` target installed

When Rustup is available, `simulator/browser/scripts/build-wasm.sh` prefers the
Rustup `stable` toolchain and exports its `rustc`/`cargo` paths before invoking
`wasm-pack`. This avoids accidentally using another `rustc` earlier in `PATH`
that lacks the `wasm32-unknown-unknown` target.

If `wasm-pack` fails before it starts with a loader error such as
`libbz2.so.1.0: cannot open shared object file`, the installed `wasm-pack`
binary was linked against a bzip2 soname that is not available on the host.
Rebuild it so `bzip2-sys` uses its vendored static bzip2 instead of a
host-specific `pkg-config` result:

```bash
PKG_CONFIG=/bin/false rustup run stable cargo install wasm-pack --locked --force
```

Then rerun:

```bash
npm run wasm:build
```

## Current Compiler Subset

The initial compiler subset follows the documented SquidScript app shape used by the Hello Menu fixture:

```squid
app "hello-menu" target "xteink-x4"

state {
  selected: int = 0
}

event.on("app.start") {
  state.load()
  screen.open("main")
}

event.on("key.DOWN") {
  state.selected = state.selected + 1
  state.save()
  screen.refresh()
}

screen("main", { render: "compose" }) {
  service.display.clear(color.GRAY0)
  service.display.text("Hello Menu", { x: 20, y: 60, w: 440, h: 48, fontHeight: 32, align: "center" })
}
```

This subset compiles to `squidscript-ir` JSON with state defaults, event
handlers, screens, and display statements.

Currently supported syntax:

- `app "id" target "target-id"`
- `state { name: type = literal }`
- `event.on("app.start") { ... }`
- `event.on("key.KEY") { ... }`
- `screen("name", { render: "compose" }) { ... }`
- `function name(...) { ... }`
- `state.load()`, `state.save()`, `state.reset()`
- `screen.open("name")`, `screen.refresh()`
- `app.exit()`
- local `let` bindings, typed local annotations, assignment, `if/else`, `repeat`, and bounded `for ... in ... max ...`
- expression calls and binary operators `+`, `-`, `==`, `!=`, `<`, `<=`, `>`, and `>=`
- `service.display.clear(...)`, `service.display.text(...)`, `service.display.rect(...)`, `service.display.line(...)`
- expression-valued `service.display.text(...)` text arguments and option values

Unsupported areas remain explicit future work: includes, modules, full arithmetic/logical expression precedence, content APIs, BinBook APIs, and production SQBC execution.

## Missing Target Metadata

The target definition now references an external simulator layout file:

```text
targets/layouts/xteink-x4.layout.json
```

The browser simulator still keeps a derived fallback layout in code for resilience. When the layout JSON is available, that file is the preferred source for shell and physical button placement.

## Verification

When changing `simulator/browser`, verify the actual app behavior, not only unit tests. For the per-area check commands, the expected baseline checks, and the Hello Menu proof checklist, see `docs/standards/verification-commands.md`.

## Dev Server

If browser behavior disagrees with code, check for a stale Vite server on port `5174`. Restart the dev server and hard reload the browser before assuming runtime/compiler behavior is wrong.

Use `http://127.0.0.1:5174/` as the default URL for simulator verification.

## Browser State

The browser simulator uses browser storage:

- IndexedDB backs simulated `/sd`
- `localStorage` stores the editor draft

`localhost:5174` and `127.0.0.1:5174` are separate browser origins. Clearing one does not clear the other.

Use the simulator's reset controls when debugging:

- `Reset App State`: clears app-scoped runtime state
- `Reset Storage`: clears simulated `/sd`
- `Reset Simulator`: clears simulated `/sd`, editor draft, compiled state, installed app selection, runtime state, and debug log
- `Clean Launch`: resets, restores default Hello Menu source, compiles, uploads, and runs in one path

## Grayscale Semantics

SquidScript logical grayscale follows the language spec. See `docs/language_spec.md` and `docs/target_profile_architecture.md`.

- `gray0` is white
- `gray15` is black
- `white` is equivalent to `gray0`
- `black` is equivalent to `gray15`

Do not introduce internal inversions that make renderer and runtime disagree. Renderer-facing draw commands should use the same logical grayscale values.

## Firefox Canvas Caveat

Firefox on Linux may visually composite a scaled `<canvas>` incorrectly when CSS uses:

```css
image-rendering: pixelated;
```

We observed a case where Firefox displayed the X4 canvas as black even though `getImageData()` returned correct pixels:

- background: white
- selected row: black
- unselected row: white

The browser simulator should avoid `image-rendering: pixelated` on the main device canvas unless this is re-tested in real Firefox. Prefer `image-rendering: auto` for the scaled device display.

If Firefox appears visually wrong, inspect the actual canvas before changing runtime logic:

```js
(() => {
  const c = document.querySelector('canvas[aria-label="X4 display"]');
  const ctx = c?.getContext("2d");
  const pix = (x, y) => ctx ? Array.from(ctx.getImageData(x, y, 1, 1).data) : null;

  return {
    canvasFound: !!c,
    attrs: c ? {
      renderOk: c.getAttribute("data-render-ok"),
      commandCount: c.getAttribute("data-command-count"),
      firstCommand: c.getAttribute("data-first-command"),
      width: c.width,
      height: c.height,
      clientWidth: c.clientWidth,
      clientHeight: c.clientHeight
    } : null,
    pixels: {
      background: pix(10, 10),
      selectedRow: pix(40, 170),
      aboutRow: pix(40, 226)
    },
    diagnostics: document.querySelector('[aria-label="display diagnostics"]')?.textContent
  };
})();
```

If backing pixels are correct but the canvas looks wrong, suspect CSS/compositor behavior before changing compiler/runtime semantics.

## Target And Rendering References

- `targets/xteink-x4.target.json`: XTEINK X4 target data used by browser-sim
- `docs/target_definition_reference.md`: target definition reference
- `docs/target_profile_architecture.md`: target profile and grayscale semantics
- `docs/ir_schema.md`: browser-sim IR JSON shape
