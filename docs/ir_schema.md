# SquidScript IR JSON Schema

Status: Browser-sim v1 development artifact

IR JSON is the browser simulator's current executable interchange format between the Rust compiler frontend and browser runtime.

It is not a production firmware format. Production firmware remains SQBC bytecode-only.

## Version

Every IR document has:

```json
{
  "format": "squidscript-ir",
  "version": 1
}
```

Consumers must reject unknown `format` values and unsupported versions.

## Shape

```json
{
  "format": "squidscript-ir",
  "version": 1,
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
        { "op": "display.clear", "color": "gray0" },
        {
          "op": "display.text",
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

## v1 Event Names

- `app.start`
- `app.exit`
- `key.UP`
- `key.DOWN`
- `key.LEFT`
- `key.RIGHT`
- `key.SELECT`
- `key.BACK`
- `key.POWER`

## v1 Statements

The browser runtime currently recognizes:

- `assign`
- `let`
- `if`
- `repeat`
- `for`
- `return`
- `call`
- `screen.refresh`
- `screen.open`
- `app.exit`
- `state.load`
- `state.save`
- `display.clear`
- `display.text`
- `display.rect`
- `display.line`

`handlers[].preload` is optional and defaults to `false`. It comes from the
source-level `@preload` hint before `event.on(...)` and remains advisory for
firmware/runtime chunk loading.

Display statements are rendered when they appear in a screen body. Non-display statements are executed in handlers. Unknown statements should be treated as runtime errors once runtime diagnostics are formalized.

## v1 Expressions

The browser-sim IR currently recognizes:

- `literal`
- `state`
- `binary` with `+`, `-`, `==`, `!=`, `<`, `<=`, `>`, and `>=`
- `call`

## v1 Validation

The compiler currently reports diagnostics for missing app declarations, missing screens, target mismatches, duplicate screen and function names, unknown `screen.open(...)` targets, unsupported screen render policies, direct mutating statements inside screen bodies, and display calls outside screen rendering.
