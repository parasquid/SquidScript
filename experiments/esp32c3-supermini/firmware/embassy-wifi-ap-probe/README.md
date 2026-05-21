# ESP32-C3 Super Mini Embassy Wi-Fi AP Probe

This experiment isolates the current esp-rs Embassy access-point path from the
SquidScript reference firmware. It follows the upstream `embassy_access_point`
shape using `esp-radio` 0.18, `esp-rtos`, `embassy-net`, DHCP, and an HTTP
socket on `192.168.2.1:8080`.

Use this after the blocking `wifi-ap-probe` when checking whether AP beaconing
depends on the newer async runner architecture. It is still a hardware/radio
experiment, not production SquidScript firmware.

The AP SSID is `esp-radio`, matching the upstream esp-rs example. The probe is
currently pinned to channel 2 for same-channel host scanning while AP
visibility is under investigation.

Build:

```sh
experiments/esp32c3-supermini/firmware/embassy-wifi-ap-probe/build.sh
```

Flash:

```sh
experiments/esp32c3-supermini/firmware/embassy-wifi-ap-probe/flash.sh
```
