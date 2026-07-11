# Firmware Build Architecture

SquidScript has one firmware architecture: native Rust firmware for XTEINK X4.
The workspace lives under `firmware/native`, and
`targets/xteink-x4.target.json` is the canonical target description.

`squidc target build` validates the target and partition table, builds the
configured Rust package for `riscv32imc-unknown-none-elf`, and uses `espflash
save-image` to create the OTA application image. `target flash` builds first,
then writes the bootloader, partition table, and application through
`espflash`. `target monitor` opens the serial monitor.

The target JSON contains a direct `firmware` object with these public fields:
`package`, `workingDir`, `target`, `chip`, `elf`, `otaImage`,
`partitionTable`, `bootloader`, `features`, `release`, `rustupToolchain`, and
`bleConnectionWatchdogMs`.

There is no selectable firmware backend. Additional boards require their own
real native firmware integration before they can be added as targets.
