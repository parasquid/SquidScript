# ble-install

A SquidScript app that arms itself for BLE Object Transfer and calls
`app.install(file_ref, "installed-app")` for each completed transfer.

## What it does

1. On `app.start`, the app arms itself so it can receive BLE Object
   Transfer events. It loads its state and resets if the schema has
   changed.
2. On `ble.object.complete`, the app calls `app.install(event.file,
   "installed-app")` to validate the SQBC magic and register the
   app at `/sd/apps/installed-app/main.sqbc`. The staging file is
   `fs_unlink`d by the firmware after the event handler returns, so
   the app must consume (install/copy) the file before returning.
3. On `ble.object.error`, the app increments a failure counter and
   logs the error reason.

## Object Name format

The BLE client writes the staging file under an Object Name of the
form `ble-install/<profile>/.sqbc` (e.g. `ble-install/wallpaper/.sqbc`).
The profile_id segment is a future hook for routing to different
handlers; the example app ignores it.

## Building

```sh
cargo run -p squidc -- app build examples/ble-install/main.squid --out /tmp/ble-install.sqbc
```

## Installing

Push the compiled SQBC over the serial protocol:

```sh
squidc device install --file /tmp/ble-install.sqbc --app-id ble-install
```

## Pairing with the host test driver

The companion `tools/ots-push/` Python package (slice 10) pushes
SQBC files to a paired device's OTS service:

```sh
python -m ots_push --device <BLE-address> \
    --app-id ble-install --profile wallpaper \
    --file /tmp/installed-app.sqbc
```

The device receives the file, the `ble-install` app's
`ble.object.complete` handler fires, and `app.install` registers
`installed-app`.
