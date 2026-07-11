# XTEINK X4 Bootloader

This ESP-IDF bootloader enables application rollback for the native X4 OTA
health-confirmation flow. The tracked `bootloader.bin` is the image used by
`squidc target flash --target xteink-x4`.

Rebuild it with:

```bash
./firmware/native/bootloader/xteink-x4/build.sh
```

The script uses a local ESP-IDF installation when available and otherwise uses
the official ESP-IDF 5.5 container image. Generated ESP-IDF configuration and
build files are ignored; commit `bootloader.bin` whenever its source or defaults
change.
