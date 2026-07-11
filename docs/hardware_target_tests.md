# Hardware Target Tests

The supported hardware inventory belongs to XTEINK X4 and is listed by:

```bash
cargo run -p squidc -- hardware test --target xteink-x4 --list
```

Run the complete inventory with an explicit serial port when more than one
candidate device is attached:

```bash
cargo run -p squidc -- hardware test --target xteink-x4 --port <port>
```

The inventory covers portable app tests, app runtime and lifecycle, physical
input routing, planned sleep, serial OTA, SD storage, BinBook rendering,
grid-cursor display behavior, transfer regressions, and Wi-Fi/BLE coexistence.
Hardware-owning checks must run sequentially.

Protocol success is not visual evidence. Display claims require a fresh live
capture inspected according to the host-local webcam guidance.
