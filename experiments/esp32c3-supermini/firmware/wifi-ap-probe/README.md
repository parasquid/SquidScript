# ESP32-C3 Super Mini Wi-Fi AP Probe

This experiment isolates ESP32-C3 SoftAP behavior from the SquidScript
reference firmware. It follows the release-matched esp-rs blocking
`access_point` example shape: keep the AP `WifiDevice`, attach it to a
smoltcp-backed blocking stack, start an open AP, and continuously pump
`socket.work()`.

Use this as the Rust AP baseline before changing the SquidScript runtime. AP
mode is the active Wi-Fi path for now; station mode is intentionally parked
until open AP behavior is reliable in Rust firmware.

The AP SSID is `esp-radio`, matching the upstream esp-rs example.

Build:

```sh
experiments/esp32c3-supermini/firmware/wifi-ap-probe/build.sh
```

Flash:

```sh
experiments/esp32c3-supermini/firmware/wifi-ap-probe/flash.sh
```

Use this only as a hardware/radio experiment. Production SquidScript firmware
lives under `firmware/squid-firmware`.
