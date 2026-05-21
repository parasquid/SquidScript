# Developer Workflow

## Browser Simulator

Install browser dependencies:

```bash
cd simulator/browser
npm install
```

Build the Rust/WASM compiler and browser app:

```bash
npm run build
```

Start the simulator:

```bash
npm run dev -- --port 5174
```

Open:

```text
http://127.0.0.1:5174/
```

## Rust/WASM Toolchain

`npm run build` runs `npm run wasm:build`, which requires:

- `wasm-pack`
- a Rust toolchain with `wasm32-unknown-unknown`

One portable setup is:

```bash
rustup toolchain install stable --target wasm32-unknown-unknown
cargo install wasm-pack --locked
```

`simulator/browser/scripts/build-wasm.sh` prefers Rustup `stable` when Rustup is
available and exports that toolchain's `rustc`/`cargo` paths for `wasm-pack`.
This keeps the browser WASM build from accidentally using another Rust
installation earlier in `PATH`.

If `wasm-pack` fails to start with `libbz2.so.1.0: cannot open shared object
file`, rebuild it without host `pkg-config` so its bzip2 dependency is vendored:

```bash
PKG_CONFIG=/bin/false rustup run stable cargo install wasm-pack --locked --force
```

## Full Check

From the repository root:

```bash
scripts/check.sh
```

This runs:

- `cargo test`
- browser unit tests
- browser production build, including WASM compiler generation
- Playwright browser tests
