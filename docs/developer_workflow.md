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

## Host-Compiled Serial App Workflow

SquidScript apps compile on the host. The firmware receives SQBC bytecode or a
package produced by `squidc`; it does not compile source as the normal
development or production path. This keeps parsing, semantic checks, bytecode
validation, source maps, and package assembly in host tooling while the device
keeps a bounded runtime and firmware-owned service surface.

Common serial workflows:

```bash
cargo run -p squidc -- app build examples/hello-menu/main.squid --out target/hello-menu.sqbc
cargo run -p squidc -- app run examples/hello-menu/main.squid
cargo run -p squidc -- app install examples/hello-menu/main.squid
cargo run -p squidc -- app launch hello-menu
cargo run -p squidc -- device output
cargo run -p squidc -- device resources
```

`app run` is the fast host-compiled temp-app path: `squidc` compiles the source
on the host, uploads the resulting SQBC through the native Rust temp-run protocol,
and launches it as a temporary foreground app through the normal lifecycle
handoff path. The temp app is not published into the installed app registry and
does not overwrite `main`. `app install` is the persistent path: source input
is still compiled on the host, then uploaded as installed SQBC or package data.
Use `device output`, `device trace`,
`device drawlog`, `device lifecycle`, `device errors`, and `device resources`
to inspect firmware-visible app behavior after launch.

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
