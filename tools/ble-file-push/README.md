# ble-file-push

A Python driver that pushes SQBC files to a SquidScript device over the
firmware's custom BLE GATT file-transfer service.

## Install

bleak is required for real BLE pushes:

```sh
pip install bleak pytest pytest-asyncio
```

When running from a SquidScript Zephyr wrapper that sources
`scripts/zephyr-env.sh`, install bleak into the Zephyr venv:

```sh
pip install --target target/zephyr/venv/lib/python3.14/site-packages bleak
```

## Usage

```sh
python -m ble_file_push push <device-name-or-address> <source.sqbc>
```

Example:

```sh
python -m ble_file_push push AA:BB:CC:DD:EE:FF /tmp/installed-app.sqbc
```

The BLE File name is `.sqbc`. Firmware delivers the uploaded file to the
foreground app's active `service.ble.start("file-transfer", ...)` profile.
The receiving app decides what to do with the file, such as
`app.install(ev.upload)`.

## Skip behavior

The CLI exits 0 with a clear skip message when bleak or a Bluetooth adapter is
not available. Real transfer failures return a non-zero exit code.

## Tests

```sh
PYTHONPATH=tools/ble-file-push pytest -q tools/ble-file-push/tests
```
