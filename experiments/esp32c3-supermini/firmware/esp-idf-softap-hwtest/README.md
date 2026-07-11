# ESP-IDF Wi-Fi Hardware Test

This is an aside experiment for ruling out ESP32-C3 SuperMini RF/hardware
problems. It intentionally does not use SquidScript, Rust, `esp-radio`, or the
reference firmware runtime.

ESP-IDF is an external developer dependency for this experiment. Do not vendor
ESP-IDF, generated SDK files, container layers, or build outputs into the
SquidScript repository. The scripts use a local `idf.py` when available, or an
official Espressif IDF container image via Podman/Docker.

The default build follows Espressif's ESP-IDF SoftAP example shape:

- initialize NVS
- initialize `esp_netif`
- create the default Wi-Fi AP netif
- register Wi-Fi AP connect/disconnect event handlers
- configure open SoftAP mode
- start the Wi-Fi driver
- periodically print connected station count

The station-mode build follows Espressif's station example shape and adds scan
diagnostics, disconnect reason labels, password-length logging, and power-save
disablement for hardware isolation.

## Test Network

- SSID: `ESP32C3-HWTEST`
- Password: open network
- Channel: `6`
- Expected AP IP: `192.168.4.1`

Change `HWTEST_WIFI_CHANNEL` in `main/softap_hwtest_main.c` to test channels
`1`, `6`, and `11`.

## Build

With ESP-IDF on PATH:

```bash
./build.sh
```

Without a local ESP-IDF install, the script can use the official Espressif IDF
container image through `podman` or `docker`:

```bash
./build.sh
```

The default image is:

```text
docker.io/espressif/idf:release-v5.5
```

Override with `ESP_IDF_CONTAINER_IMAGE` if needed.

Generated `build/`, `sdkconfig`, and `sdkconfig.old` files are ignored by the
repository.

## Station Build

Use station mode when the board should join an existing 2.4 GHz network. Put
the SSID and password in a local env file such as `~/.env`; do not commit that
file or print the secret:

```bash
HWTEST_STA_SSID='2.4 GHz SSID'
HWTEST_STA_PASSWORD='wifi password here'
```

Then build with:

```bash
./build-station.sh
```

You can also provide the SSID with an environment variable rather than editing
source:

```bash
HWTEST_STA_SSID='2.4 GHz SSID' ./build-station.sh
```

The station build logs SSID/password lengths but never logs either configured
value. It scans for the target SSID before connecting, logs matching AP
BSSID/channel, RSSI, and auth mode, disables Wi-Fi power save, and labels common
disconnect reason codes such as `AUTH_EXPIRE` and `CONNECTION_FAIL`.

## Flash And Monitor

```bash
./flash-monitor.sh /dev/ttyACM0
```

If containerized flashing cannot open `/dev/ttyACM0`, prefer flashing from a
local ESP-IDF install. Rootless Podman/Docker can lose access to the host serial
device even when the host user is in `dialout`.

There is also a host-side helper that flashes the ESP-IDF ELF image with
`espflash` and the experiment's matching partition table:

```bash
./flash-with-espflash.sh /dev/ttyACM0
```

That helper is a convenience for this development environment. Treat
Espressif's own `idf.py flash` as the reference flashing path for this
hardware-isolation test.

The expected serial output includes:

- `ESP32-C3 SoftAP hardware test using ESP-IDF`
- `SoftAP hardware test started`
- `SSID:ESP32C3-HWTEST password:<open> channel:6`
- `AP MAC:...`
- `connected stations:0`

For a station-mode image, expected success output includes:

- `ESP32-C3 station hardware test using ESP-IDF`
- `scan found ... matching AP record(s)`
- `scan[0] ssid_len:... channel:... rssi:... auth:...`
- `got ip:...`
- `connected rssi:... channel:... authmode:...`

When a phone or laptop joins, the log should show:

- `station <mac> join`
- `connected stations:1`

## Interpretation

- SSID visible and devices join: the board RF path is probably fine; focus back
  on SquidScript native firmware, driver configuration, and scheduler/network
  integration.
- SSID invisible with this ESP-IDF test too: suspect board/RF/antenna,
  placement, power, or environment.
- SSID visible only very close to the board, when touching the antenna area, or
  only off a breadboard: suspect the known ESP32-C3 SuperMini antenna/layout
  sensitivity seen on some clone boards.
- Station scan sees the AP but authentication expires or reports
  `CONNECTION_FAIL`: the receiver can hear the router, but auth did not
  complete. Suspect router security compatibility, password handling, or a weak
  transmit/RF path before blaming SquidScript.
- Station gets IP: station RX/TX/auth/DHCP are probably working; focus back on
  SquidScript native firmware, driver configuration, and scheduler/network
  integration.
- Station joins but DHCP or traffic fails: auth works; next isolate IP stack,
  DHCP, or AP configuration.

## Developer Troubleshooting Flow

Use this test when SquidScript Wi-Fi diagnostics claim AP start success but no
client can see the SSID.

1. Build and flash this ESP-IDF test.
2. Scan from a phone and a laptop for `ESP32C3-HWTEST`.
3. Join the open AP and watch serial logs for station join/count changes.
4. Repeat on channels `1`, `6`, and `11` if the AP is not visible.
5. Move the board away from USB hubs, metal, breadboards, and other antennas;
   try very short range as a sanity check.
6. If ESP-IDF also cannot produce a visible AP, treat board/RF/power/environment
   as the current suspect before making SquidScript firmware changes.

Use station mode when the board should prove it can join a known network:

1. Confirm the host or phone can see and join the same 2.4 GHz SSID.
2. Put `HWTEST_STA_SSID` and `HWTEST_STA_PASSWORD` in an untracked env file.
3. Run `./build-station.sh`, then flash and monitor.
4. Check scan RSSI/channel/auth output before interpreting connection failure.
5. If a mixed WPA/WPA2 router fails, retry with a simple 2.4 GHz WPA2-only
   phone hotspot before drawing a hardware conclusion.
