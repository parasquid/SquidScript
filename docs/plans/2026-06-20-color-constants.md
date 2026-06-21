# Color Constants Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace string color values (`"gray15"`, `"white"`) with compile-time `color.*` constants (`color.GRAY15`, `color.WHITE`) that lower to typed `uint8` values through the full stack — parser, IR, SQBC, VM, FFI, firmware — removing the temporary Plan 2 string parser and eliminating color strings from the SQBC string table.

**Architecture:** `color.GRAY0`..`color.GRAY15`, `color.WHITE`, `color.BLACK` are parsed as integer literals (0-15) in the parser's namespace dispatch. The SQBC emitter uses `emit_i32_option` (already used for coordinates) instead of `emit_string_option` for color options. The VM pops `Value::I32` instead of `Value::String`. The FFI option structs carry `uint8_t` color fields instead of `const uint8_t*` + length pairs. The firmware receives typed colors directly and the `sq_display_color_parse` calls are removed. String color values are replaced entirely (pre-1.0 direct replacement).

**Tech Stack:** Rust compiler (squidc-core, squidvm-core, squidvm-ffi, squidc-wasm), Zephyr C17, ztest/Twister, XTEINK X4 hardware.

**Design spec:** `docs/specs/2026-06-20-x4-ram-reduction-design.md` § "Color Constants"

---

### Task 1: Language spec and parser

**Files:**
- Modify: `docs/language_spec.md` §26
- Modify: `compiler/rust/crates/squidc-core/src/parser/expressions.rs`
- Modify: `compiler/rust/crates/squidc-core/src/parser/statements.rs`
- Modify: `compiler/rust/crates/squidc-core/src/ir.rs`
- Test: `compiler/rust/crates/squidc-core/src/parser/` tests

- [ ] **Step 1: Update language spec §26**

Add the `color.*` constant namespace to the drawing built-ins section. Document all 18 constants (`color.GRAY0` through `color.GRAY15`, `color.WHITE`, `color.BLACK`). State that string color values are no longer accepted. Update all code examples in §26 to use `color.*` constants.

- [ ] **Step 2: Add `color` namespace to the parser**

In `parse_primary_expr` (`parser/expressions.rs`), add a `color` branch in the namespace dispatch (after `system.*`, before the fallback). When the parser sees `Ident("color")` followed by `Dot`, read the next ident, validate it matches a color name, and produce `IrExpr::Literal { value: serde_json::json!(<level>) }`:

```rust
} else if name == "color" && self.at_kind(TokenKind::Dot) {
    self.bump(builder);
    let color_name = self.peek()?.text.clone();
    self.bump(builder);
    let level = parse_color_constant(&color_name)
        .ok_or_else(|| ParseError::new(format!("unknown color: color.{}", color_name)))?;
    IrExpr::Literal { value: serde_json::json!(level) }
}
```

Add `parse_color_constant`:
```rust
fn parse_color_constant(name: &str) -> Option<i32> {
    match name {
        "WHITE" => Some(0),
        "BLACK" => Some(15),
        _ => {
            if let Some(num) = name.strip_prefix("GRAY") {
                let n: i32 = num.parse().ok()?;
                if (0..=15).contains(&n) { Some(n) } else { None }
            } else { None }
        }
    }
}
```

- [ ] **Step 3: Change `display.clear` to accept an expression**

In `parser/statements.rs`, change `DisplayClear` from `color: String` to `color: IrExpr`:
```rust
DisplayClear { color: IrExpr },
```
Parse the color as an expression instead of `consume_string`:
```rust
let color = self.parse_expr(builder)
    .ok_or_else(|| ParseError::new("display.clear requires a color argument"))?;
```

- [ ] **Step 4: Write parser tests**

Test that `color.GRAY15` parses to `IrExpr::Literal { value: json!(15) }`,
`color.WHITE` to `json!(0)`, `color.BLACK` to `json!(15)`, and `color.BOGUS`
fails. Test that `display.clear(color.WHITE)` parses with the color as an
expression. Test that `display.rect(..., { fillColor: color.GRAY8 })` parses.

- [ ] **Step 5: Run parser tests and verify GREEN**

- [ ] **Step 6: Commit Task 1**

### Task 2: SQBC emission and VM dispatch

**Files:**
- Modify: `compiler/rust/crates/squidc-core/src/sqbc.rs`
- Modify: `compiler/rust/crates/squidvm-core/src/vm.rs`
- Modify: `compiler/rust/crates/squidvm-core/src/host.rs`

- [ ] **Step 1: Change SQBC emission for colors**

In `sqbc.rs`, change `DisplayClear` emission from string intern to expr compile:
```rust
IrStatement::DisplayClear { color } => {
    compile_expr(unit, frame, color)?;
    emit_builtin(&mut unit.code, BUILTIN_DISPLAY_CLEAR);
}
```

Change `emit_string_option` calls for color keys (`fillColor`, `strokeColor`, `textColor`, `backgroundColor`, `color`) to `emit_i32_option`. Remove `collect_option_strings` calls for color keys.

- [ ] **Step 2: Change VM dispatch for colors**

In `vm.rs`, change `BUILTIN_DISPLAY_CLEAR` to pop `Value::I32`:
```rust
BUILTIN_DISPLAY_CLEAR => {
    let color = self.pop()?.expect_i32()?;
    host.draw_clear(color as u8);
}
```

Change text/rect/line dispatch to pop colors as `Option<i32>` → `Option<u8>`:
```rust
let fill_color = self.pop_optional_i32()?.map(|v| v as u8);
```

Add `pop_optional_i32` if it doesn't exist (it may already exist for coordinate options).

- [ ] **Step 3: Change Host trait Display*Options**

In `host.rs`, change color fields from `Option<&str>` to `Option<u8>`:
```rust
pub struct DisplayRectOptions {
    pub x: i32, pub y: i32, pub w: i32, pub h: i32,
    pub fill_color: Option<u8>,
    pub stroke_color: Option<u8>,
}
```
Same for `DisplayTextOptions` (`text_color`, `background_color`) and `DisplayLineOptions` (`color`). Change `draw_clear` to `fn draw_clear(&mut self, color: u8)`.

- [ ] **Step 4: Run compiler/VM tests and verify**

- [ ] **Step 5: Commit Task 2**

### Task 3: FFI ABI change

**Files:**
- Modify: `compiler/rust/crates/squidvm-ffi/abi/manifest.json`
- Regenerate: `firmware/zephyr/src/squidvm_ffi.h`
- Regenerate: `compiler/rust/crates/squidvm-ffi/src/generated_callbacks.rs`
- Modify: `compiler/rust/crates/squidvm-ffi/src/lib.rs`
- Modify: `compiler/rust/crates/squidc-wasm/src/lib.rs`

- [ ] **Step 1: Update the FFI manifest**

In `manifest.json`, change the display option struct color fields from
`const uint8_t *` + `size_t` to `uint8_t`:
```json
{ "name": "fill_color", "type": "u8" }
```
(absent = 0xFF). Same for `stroke_color`, `text_color`, `background_color`,
and the clear callback's `color` parameter.

- [ ] **Step 2: Regenerate the FFI header and generated Rust**

Run:
```sh
python3 scripts/check-squidvm-ffi-abi.py --write-header --write-doc --write-generated
```

- [ ] **Step 3: Update the Rust FFI shim**

In `squidvm-ffi/src/lib.rs`, update `draw_clear`, `draw_text`, `draw_rect`,
`draw_line` to pass `u8` instead of `*const u8` + `usize`. Remove
`option_ptr`/`option_len` for color fields; pass `Option<u8>` as `u8` (0xFF
for None).

- [ ] **Step 4: Update the WASM host**

In `squidc-wasm/src/lib.rs`, update `draw_clear`, `draw_text`, `draw_rect`,
`draw_line` to receive `Option<u8>` instead of `Option<&str>`. Remove
`color_to_gray` (the value is already the gray level). Pass the numeric
value directly to the JSON draw command.

- [ ] **Step 5: Run FFI equivalence tests**

- [ ] **Step 6: Commit Task 3**

### Task 4: Firmware and test migration

**Files:**
- Modify: `firmware/zephyr/src/vm_runtime_display.c`
- Modify: `firmware/zephyr/tests/protocol/src/main.c`
- Modify: all `examples/*/main.squid`
- Modify: `simulator/browser/src/compiler/defaultSource.ts`
- Modify: `simulator/browser/src/compiler/types.ts`

- [ ] **Step 1: Update firmware to receive typed colors**

In `vm_runtime_display.c`, the FFI callbacks now receive `uint8_t` colors
directly. Remove `sq_display_color_parse` calls in `runtime_display_clear`,
`runtime_display_text`, `runtime_display_rect`. Store the `uint8_t` directly
into the op's typed color field. For `display.clear`, the callback signature
changes from `(void *, const uint8_t *, size_t)` to `(void *, uint8_t)`.

- [ ] **Step 2: Update firmware tests**

Update test fixtures and assertions that pass color strings to use typed
values. Update the test stub callback signatures to match the new FFI.

- [ ] **Step 3: Migrate all examples**

Replace every `"gray15"` with `color.GRAY15`, `"white"` with `color.WHITE`,
`"black"` with `color.BLACK`, `"gray0"` with `color.GRAY0`, `"gray8"` with
`color.GRAY8`, etc. across all `.squid` files in `examples/` and
`tests/hardware/`.

- [ ] **Step 4: Migrate the browser simulator**

Update `defaultSource.ts` to use `color.*` constants. Update `types.ts` IR
types if needed. The renderer already works with numeric `gray` values.

- [ ] **Step 5: Run native ztests**

Run `scripts/zephyr-test-protocol.sh`. Expected: no new failures beyond the
pre-existing 33.

- [ ] **Step 6: Run Rust tests**

Run `cargo test -p squidc-core -p squidvm-core -p squidvm-ffi -p squid-device-protocol`.
All must pass.

- [ ] **Step 7: Commit Task 4**

### Task 5: Hardware verification and docs

- [ ] **Step 1: Build and flash X4**
- [ ] **Step 2: Run grid-cursor and binbook-reader workloads**
- [ ] **Step 3: Verify rendering is identical (no visual regressions)**
- [ ] **Step 4: Update docs (language spec, runtime limits, design spec)**
- [ ] **Step 5: Commit and push**
