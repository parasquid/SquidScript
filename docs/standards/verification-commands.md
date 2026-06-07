# Verification Commands

Which checks to run per change area. Run checks from the directory shown unless
noted.

| Change area | Commands |
| --- | --- |
| Rust compiler crates, fixtures, IR lowering, SQBC container | `cargo test` from repo root |
| Browser simulator TypeScript runtime, WASM compiler bridge, rendering, storage, input | `npm test` from `simulator/browser` |
| Browser simulator production build or WASM compiler bridge | `npm run build` from `simulator/browser` |
| Browser UI behavior, Hello Menu flow, canvas pixels, Firefox/mobile coverage | `npm run test:e2e` from `simulator/browser` |
| Target definitions or render policy docs | `cargo test` plus relevant browser tests if browser-sim consumes the target |
| Docs-only edits | Usually no tests required, unless examples/fixtures changed |

For browser-sim changes that affect the real app experience, run `npm test`, `npm run build`, and `npm run test:e2e`; then try the flow manually on `http://127.0.0.1:5174/` when visual behavior is relevant.

Expected baseline checks:

- `npm test`
- `npm run build`
- `npm run test:e2e`
- Run the dev server on `http://127.0.0.1:5174/`
- Exercise the browser UI: reset, compile, upload, run, and input navigation for Hello Menu.

Hello Menu should prove:

- compile succeeds with the WASM compiler
- upload installs `/sd/apps/hello-menu/main.sqbc`
- run opens the `menu` screen
- selected row pixels are black and unselected/background pixels are white
- `UP`/`DOWN` move selection correctly and stay bounded
- `SELECT` opens screens or exits according to the script
- `BACK` returns from `hello`/`about` to `menu`; `BACK` exits only from `menu`
- reload preserves saved app state
- reset controls clear the right state/storage
