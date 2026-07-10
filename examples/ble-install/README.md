# ble-install

A SquidScript app that accepts SQBC uploads over BLE and installs them.

## What it does

1. On `app.start`, the app calls `service.upload.start(...)` with the `ble`
   transport and accepts `.sqbc` uploads.
2. On `upload.complete`, it logs `ev.transport`, calls
   `app.install(ev.upload)`, uses the
   returned `installed.id`, and launches the installed app.

`app.install(ev.upload)` validates the uploaded SQBC and installs it under the
app id embedded in the SQBC metadata.

## Building

```sh
cargo run -p squidc -- app build examples/ble-install/main.squid --out /tmp/ble-install.sqbc
```

## Installing Over Serial

```sh
squidc app install /tmp/ble-install.sqbc
```

## Pushing an App Over BLE

Launch this app, then push a compiled SQBC:

```sh
squidc device upload /tmp/installed-app.sqbc --name installed-app.sqbc \
  --transport ble --device <BLE-address-or-name>
```
