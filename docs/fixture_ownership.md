# Fixture Ownership

SquidScript fixtures live at the layer that owns the behavior under test.

- `compiler/fixtures/` contains compiler language and IR fixtures.
- `compiler/rust/fixtures/conformance/` contains shared Rust SQBC and VM
  conformance fixtures.
- `tests/repl/` contains host CLI REPL session fixtures.
- `tests/hardware/` contains hardware target app fixtures.

Lower-level crates should not include files from `examples/`. Promote reusable
inputs into the owning fixture directory instead.
