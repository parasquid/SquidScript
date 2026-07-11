# Target Profile Architecture

Target JSON describes concrete hardware and the native firmware artifact that
supports it. `targets/xteink-x4.target.json` is currently the only executable
target profile.

Portable SquidScript concepts remain independent of board wiring. Target JSON
owns MCU facts, pins, buses, devices, display geometry, input mappings, storage,
power, radios, runtime limits, capabilities, and build metadata. Native
firmware consumes validated/generated target constants; app code sees logical
events and `service.*` endpoints.

The `firmware` object is direct rather than backend-selecting. It identifies
the Rust package and target triple, required Cargo features, toolchain, ELF,
OTA image, partition table, bootloader, and target runtime settings.

Adding another target requires a working native firmware integration and
target-aware verification. Hardware research alone belongs in reference docs,
not an executable target JSON.
