# ots-push

A Python driver that pushes SQBC files to a SquidScript device over the
BLE Object Transfer Service (GATT UUID 0x1825) using L2CAP Connection
Oriented Channels for the data path.

## Install

bleak is required at runtime (not just for tests). Install it into the
same Python interpreter that will run the driver:

```sh
# If running outside the SquidScript Zephyr venv (standalone CI):
pip install bleak pytest pytest-asyncio

# If running from a SquidScript Zephyr wrapper (scripts/zephyr-test-ble-object-transfer.sh,
# which sources scripts/zephyr-env.sh and prepends target/zephyr/venv/bin to PATH),
# install bleak into the Zephyr venv so it's importable from the same python3:
pip install --target target/zephyr/venv/lib/python3.14/site-packages bleak
```

The package is importable without bleak and the CLI returns a clean skip
message on hosts that lack bleak, but the actual push requires bleak
plus a host platform with L2CAP CoC support (currently not exposed by
bleak 3.x's cross-platform client; see "L2CAP CoC limitation" below).

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
- bleak 3.x does not expose L2CAP CoC on this platform → `"OK ble-ots-push skipped because bleak on this platform does not support L2CAP CoC"`

## L2CAP CoC limitation

bleak 3.x's cross-platform `BleakClient` does not expose L2CAP CoC
writes. The spec requires L2CAP CoC only (no GATT-writes fallback), so
on hosts where bleak doesn't support CoC (currently all platforms
through bleak 3.0.2), the driver exits 0 with the skip message
"bleak on this platform does not support L2CAP CoC". This matches the
spec's skip pattern: clean exit, no error. The pytest suite exercises
the full push protocol with a mock bleak backend that implements
`write_l2cap_coc`, so the GATT/CoC call order is verified independently
of host BLE capability.

When bleak gains cross-platform L2CAP CoC support, no code changes
should be needed — the driver will detect it via
`BleakClient.write_l2cap_coc` and the skip path will simply not be
taken. Track upstream bleak issue trackers for the relevant PR.

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
launch it (which arms itself on `app.start`), run `ots_push push`, and
verify the new app is registered via `squidc app list`.

Note: when the wrapper sources `scripts/zephyr-env.sh`, the PATH
prepends `target/zephyr/venv/bin`. bleak must be importable from that
Python — install it with `pip install --target target/zephyr/venv/lib/python3.14/site-packages bleak`
if the system Python has bleak but the venv doesn't.

Flashing convention: the wrapper uses `west flash -d
build/zephyr/xiao-esp32c3-gdeq0426t82-sd` (after building via
`squidc target build --target xiao-esp32c3-gdeq0426t82-sd`). The
existing `squidc target build` command builds but does not flash; the
wrapper invokes `west flash` explicitly. The serial port is
auto-detected via `scripts/lib/serial-port.sh::resolve_esp.serial_port`
and exported as `ESPFLASH_PORT`.
