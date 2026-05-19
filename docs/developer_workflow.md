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
