# ots-push

A Python driver that pushes SQBC files to a SquidScript device over the
BLE Object Transfer Service (GATT UUID 0x1825) using L2CAP Connection
Oriented Channels for the data path.

## Install

```sh
pip install bleak pytest pytest-asyncio
```

(`bleak` is only required at runtime; the package is importable without
it and the CLI returns a clean skip message on hosts that lack bleak.)

## Usage

```sh
python -m ots_push push <device-name-or-address> <app-id> <profile-id> <source.sqbc>
```

Example:

```sh
python -m ots_push push AA:BB:CC:DD:EE:FF ble-install wallpaper examples/ble-install/main.squid
```

## Object Name format

The driver writes the staging file under the Object Name
`<app-id>/<profile-id>/.sqbc` (e.g. `ble-install/wallpaper/.sqbc`). The
firmware parses this into the routing fields used by the trigger table.

## Skip behavior

The driver exits 0 with a clear skip message when the host cannot
support the transfer:

- `bleak` is not installed → `"OK ble-ots-push skipped because bleak is unavailable"`
- No Bluetooth adapter is available → `"OK ble-ots-push skipped because no Bluetooth adapter is available"`
- The paired device has no OTS service (UUID 0x1825) → `"OK ble-ots-push skipped because OTS service 0x1825 not found on device"`
- The source file does not exist → `"OK ble-ots-push skipped because source file not found: ..."`

## Tests

```sh
cd tools/ots-push
python -m pytest tests/
```

The test suite uses a mock bleak backend to verify the GATT/CoC call
order (discover → Object Name write → OACP Create → L2CAP CoC write →
OACP Execute) without requiring a real Bluetooth adapter.

## Hardware test wrapper

`scripts/zephyr-test-ble-object-transfer.sh` wraps the full end-to-end
flow: build and flash the XIAO target, compile the ble-install example,
arm it via the serial CLI, run `ots_push push`, and verify the new
app is registered via `squidc app list`.
