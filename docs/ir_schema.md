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
      "event": "onKey.DOWN",
      "statements": [
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
          "text": "Hello Menu",
          "options": { "x": 20, "y": 60, "w": 440, "h": 48, "fontHeight": 32, "align": "center" }
        }
      ]
    }
  ]
}
```

## v1 Event Names

- `onStart`
- `onResume`
- `onSuspend`
- `onKey.UP`
- `onKey.DOWN`
- `onKey.LEFT`
- `onKey.RIGHT`
- `onKey.SELECT`
- `onKey.BACK`
- `onKey.POWER`

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

Display statements are rendered when they appear in a screen body. Non-display statements are executed in handlers. Unknown statements should be treated as runtime errors once runtime diagnostics are formalized.

## v1 Expressions

The browser-sim IR currently recognizes:

- `literal`
- `state`
- `binary` with `+`, `-`, `==`, `!=`, `<`, `<=`, `>`, and `>=`
- `call`

## v1 Validation

The compiler currently reports diagnostics for missing app declarations, missing screens, target mismatches, duplicate screen and function names, unknown `screen.open(...)` targets, unsupported screen render policies, direct mutating statements inside screen bodies, and display calls outside screen rendering.
