# ble-install

A SquidScript app that starts BLE file transfer and installs uploaded SQBC
apps.

## What it does

1. On `app.start`, the app calls `service.ble.start("file-transfer", ...)`
   and accepts `.sqbc` uploads.
2. On `ble.file.complete`, it calls `app.install(ev.upload)`, uses the
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
python -m ble_file_push push <BLE-address-or-name> /tmp/installed-app.sqbc
```
