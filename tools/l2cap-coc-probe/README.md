# BLE L2CAP CoC transport spike

Throwaway kit to answer one question before committing the BLE app-install
transport architecture: **can a host drive an LE L2CAP connection-oriented
channel (CoC) to this device?** That is the data path `bleak` cannot provide and
that the standard OTS transport depends on.

## Why this matters (the decision it informs)

| Transport | Phone interop (nRF Connect) | Linux host test | macOS host test | Windows host test |
| --- | --- | --- | --- | --- |
| **OTS + L2CAP CoC** | yes | yes (this probe, BlueZ socket) | maybe (CoreBluetooth `CBL2CAPChannel`, **not bleak** — needs native/PyObjC) | **no** general LE-CoC API in WinRT |
| **Custom GATT writes** | no | yes (bleak) | yes (bleak) | yes (bleak) |

So:
- If this probe **works**, the OTS/CoC path is testable on Linux — but it is still
  not portable to Windows, and macOS needs non-bleak native code.
- If host-side testing must run on **Windows/macOS**, the custom GATT transport
  is required regardless of the probe outcome.

The probe only proves the **Linux** corner. Treat a clean run as "Linux CoC
works," not "CoC is cross-platform."

## Run it

1. Build + flash firmware with the spike CoC sink enabled:
   ```
   # add to firmware/zephyr/prj.conf (temporarily) or pass via overlay:
   CONFIG_SQUIDSCRIPT_BLE_L2CAP_PROBE=y
   ```
   then `cargo run -p squidc -- target flash --target xiao-esp32c3-gdeq0426t82-sd`
   (this select-enables `CONFIG_BT_L2CAP_DYNAMIC_CHANNEL`). The device logs
   `L2CAP CoC probe server registered on PSM 0x0025` at boot.

2. Find the device address (`bluetoothctl scan on`, look for the advertised name)
   and run the probe (needs `CAP_NET_RAW`, so usually `sudo`):
   ```
   sudo python3 tools/l2cap-coc-probe/probe.py --addr <AA:BB:..> --bytes 4096
   ```
   The XIAO advertises a random address by default; if connect fails, try
   `--addr-type public`.

3. Success looks like:
   - host: `OK L2CAP CoC streamed 4096 bytes ...`
   - device serial log: `L2CAP CoC recv ... (total 4096)` then `disconnected`.

## Interpreting the result

- **Host prints `OK ... streamed` and the device logs a matching total** → LE CoC
  works on this Linux host. The OTS/CoC transport is Linux-testable; decide
  single (OTS-only) vs dual (add custom GATT) based on the Windows/macOS row above.
- **Host exits 2 (`skipped`/connect `ENOTSUP`/`EINVAL`)** → this Python/kernel
  can't drive LE CoC. The custom GATT transport is needed for automated testing.
- **Host exits 1** → a real connection error (wrong addr/type, device not
  advertising, pairing/security). Retry; not a verdict on CoC support.

## Cleanup

This is throwaway. Once the decision is made, revert:
`CONFIG_SQUIDSCRIPT_BLE_L2CAP_PROBE`, `firmware/zephyr/src/ble_l2cap_probe.{c,h}`,
its CMakeLists entry, the `sq_ble_l2cap_probe_init()` call in `ble_smoke.c`, and
this `tools/l2cap-coc-probe/` directory.
