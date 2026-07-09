# SquidScript esp-radio patch

This directory vendors `esp-radio` 1.0.0-beta.0 because the published
ESP32-C3/S3 controller configuration encodes `TxPower` with the Rust enum
ordinal. Espressif controller power constants use a different numeric range,
so the default `P9` ordinal selects 0 dBm instead of +9 dBm.

The local patch passes `TxPower::idx()` to `txpwr_dft`. Remove this override
when the selected upstream `esp-radio` release contains the equivalent fix.
