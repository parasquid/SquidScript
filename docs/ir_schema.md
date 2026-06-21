# SquidScript IR JSON Schema

Status: Browser-sim development artifact

IR JSON is the browser simulator's current executable interchange format between the Rust compiler frontend and browser runtime.

It is not a production firmware format. Production firmware remains SQBC bytecode-only.

## Header

Every IR document has:

```json
{
  "format": "squidscript-ir"
}
```

Consumers must reject unknown `format` values.

## Shape

```json
{
  "format": "squidscript-ir",
  "app": {
    "id": "hello-menu",
    "name": "Hello Menu",
    "target": "xteink-x4"
  },
  "state": [
    {
      "name": "selected",
      "value": 0
    }
  ],
  "functions": [],
  "handlers": [
    {
      "event": "key.DOWN",
      "preload": true,
      "statements": [
        {
          "op": "if",
          "condition": {
            "op": "binary",
            "left": { "op": "state", "name": "selected" },
            "operator": "<",
            "right": { "op": "literal", "value": 2 }
          },
          "then_statements": [
            {
              "op": "assign",
              "name": "selected",
              "expr": {
                "op": "binary",
                "left": { "op": "state", "name": "selected" },
                "operator": "+",
                "right": { "op": "literal", "value": 1 }
              }
            },
            { "op": "state.save" },
            { "op": "screen.refresh" }
          ],
          "else_statements": []
        }
      ]
    }
  ],
  "screens": [
    {
      "name": "main",
      "render": "compose",
      "statements": [
        { "op": "service.display.clear", "color": { "op": "literal", "value": 0 } },
        {
          "op": "service.display.text",
          "text": { "op": "literal", "value": "Hello Menu" },
          "options": {
            "x": { "op": "literal", "value": 20 },
            "y": { "op": "literal", "value": 60 },
            "w": { "op": "literal", "value": 440 },
            "h": { "op": "literal", "value": 48 },
            "fontHeight": { "op": "literal", "value": 32 },
            "align": { "op": "literal", "value": "center" }
          }
        }
      ]
    }
  ]
}
```

## Event Names

- `app.start`
- `app.exit`
- `key.UP`
- `key.DOWN`
- `key.LEFT`
- `key.RIGHT`
- `key.SELECT`
- `key.BACK`
- `key.POWER`

## Statements

The browser runtime currently recognizes:

- `assign`
- `state.assign`
- `let`
- `if`
- `repeat`
- `for`
- `return`
- `call`
- `debug.print`
- `debug.block`
- `screen.refresh`
- `screen.open`
- `app.exit`
- `state.load`
- `state.save`
- `service.display.clear`
- `service.display.text`
- `service.display.rect`
- `service.display.line`
- `service.display.select`
- `service.display.image`
- `service.display.draw`

`handlers[].preload` is optional and defaults to `false`. It comes from the
source-level `@preload` hint before `event.on(...)` and remains advisory for
firmware/runtime chunk loading.

Display statements are rendered when they appear in a screen body. Non-display statements are executed in handlers. Unknown statements should be treated as runtime errors once runtime diagnostics are formalized.

`debug.print` evaluates and emits debug output in development profiles and is
stripped from release SQBC. `debug.block` contains a nested `statements` array;
development SQBC encodes the nested statements, while release SQBC strips the
entire block without evaluating contained expressions.

## Expressions

The browser-sim IR currently recognizes:

- `literal`
- `state`
- `variable`
- `binary` with `+`, `-`, `==`, `!=`, `<`, `<=`, `>`, and `>=`
- `field`
- `call`

## Validation

The compiler currently reports diagnostics for missing app declarations, missing
screens, target mismatches, duplicate app declarations, duplicate state blocks,
duplicate state fields, duplicate device bindings, duplicate function names,
duplicate function parameters, duplicate event handlers, duplicate trigger
events, duplicate BLE profile ids, trigger events without matching handlers,
duplicate screen names, unknown `screen.open(...)` targets, unsupported screen
render policies, undeclared local variables, duplicate local variables in a
visible scope, missing explicit state fields, state shadowing warnings, direct
or transitive state mutation from screen rendering, display calls outside screen
rendering, and invalid mutation or side effects inside `debug.block`.
