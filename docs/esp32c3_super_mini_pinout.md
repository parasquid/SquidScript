# ESP32-C3 Super Mini Pinout

This note records the common ESP32-C3 Super Mini header layout used for
SquidScript firmware bring-up. ESP32-C3 Super Mini boards are clone-heavy, so
verify against the silkscreen on the physical board before wiring power,
displays, sensors, or straps.

This is retained hardware research for a possible future native target. The
board has no current SquidScript firmware target. A condensed pin and device
table lives in `docs/targets/esp32c3-super-mini.md`.

## Top-Side Hardware Layout

Orientation: USB-C connector at the top, component side facing up. This is the
side with the USB-C connector, BOOT/RST buttons, LEDs, ESP32-C3 chip, and PCB
antenna.

```text
                    USB-C
                 .--------.
                 |        |
        .--------'        '--------.
        | PWR LED     BOOT    RST  |
        | no GPIO    GPIO9   EN/RST|
        |    *       [    ] [    ] |
 GPIO5  | o                    o   | 5V
 GPIO6  | o                    o   | GND
 GPIO7  | o       BLUE LED     o   | 3V3
 GPIO8  | o        GPIO8*      o   | GPIO4
 GPIO9  | o       BOOT*        o   | GPIO3
 GPIO10 | o                    o   | GPIO2
 GPIO20 | o                    o   | GPIO1
 GPIO21 | o                    o   | GPIO0
        |                          |
        |        PCB ANTENNA       |
        '--------------------------'
```

`BOOT` is commonly wired to `GPIO9`. `RST` is the reset/enable button and is not
a normal GPIO input. The `PWR LED` is a red power indicator and is not normally
software-controllable. The blue onboard LED is commonly wired to `GPIO8`.

## Bottom-Side Header Reference

Some public pinout diagrams show the bottom/silkscreen side instead. With USB-C
still at the top, the header order appears mirrored:

```text
                    USB-C
                 .--------.
                 |        |
        .--------'        '--------.
        |                          |
  5V    | o                    o   | GPIO5   A5 / MISO
  GND   | o                    o   | GPIO6   MOSI
  3V3   | o                    o   | GPIO7   SS
  GPIO4 | o                    o   | GPIO8   SDA / LED*
  GPIO3 | o                    o   | GPIO9   SCL / BOOT*
  GPIO2 | o                    o   | GPIO10
  GPIO1 | o                    o   | GPIO20  RX
  GPIO0 | o                    o   | GPIO21  TX
        |                          |
        '--------------------------'
```

## Pin Notes

- `GPIO8` is commonly connected to the onboard blue LED, often active-low. It is
  also a boot strapping pin.
- `GPIO9` is commonly connected to the BOOT button and is also a boot strapping
  pin.
- `GPIO2`, `GPIO3`, `GPIO8`, and `GPIO9` affect ESP32-C3 boot mode at reset.
  Avoid external circuits that force unsafe levels on those pins during reset.
- `GPIO0` through `GPIO5` are commonly exposed as ADC-capable pins.
- `GPIO20` and `GPIO21` are commonly used as UART RX/TX. ESP32-C3 boards with
  native USB Serial/JTAG may still enumerate over USB-C as `/dev/ttyACM*`.
- Peripheral labels such as `SDA`, `SCL`, `MISO`, `MOSI`, `SS`, `RX`, and `TX`
  are conventional defaults. The ESP32-C3 GPIO matrix can route many peripheral
  functions to other pins, subject to firmware and board constraints.
- ESP32-C3 GPIOs can be software-configured with weak internal pull-up or
  pull-down bias. Use that for diagnostics or simple local inputs when an
  external resistor is not present, but treat it as a weak bias rather than a
  robust substitute for a board-level resistor in noisy wiring. For the Super
  Mini BOOT button path, keep GPIO9 modeled as active-low with pull-up bias;
  do not switch the confirmed BOOT binding to pull-down.

## Suggested E-Paper Wiring

For preliminary e-paper controller bring-up on a breadboard, prefer this
conservative SPI mapping:

```text
EPD pin        Super Mini pin       Notes
-------------  -------------------  ---------------------------------------
SCK / CLK      GPIO4                common SPI clock label
MOSI / DIN     GPIO6                common SPI MOSI label
MISO / DOUT    GPIO5                only needed if the controller supports readback
CS             GPIO7                common SPI CS/SS label
DC             GPIO0                general GPIO output
RST / RES      GPIO10               general GPIO output, avoids strapping pins
BUSY           GPIO1                general GPIO input
VCC            3V3                  use 3.3 V unless the display board requires otherwise
GND            GND                  common ground
```

Avoid using `GPIO8` and `GPIO9` for e-paper control signals because they are
commonly shared with the onboard LED and BOOT button, and both participate in
boot strapping. Avoid `GPIO2` and `GPIO3` where possible for the same boot-mode
reason. Keep `GPIO20` and `GPIO21` free for UART/debug unless a project needs
those pins more than serial access.

Some public examples wire e-paper reset to `GPIO2`. That can work on a specific
board, but `GPIO10` is the preferred reset pin for this repo's Super Mini
bring-up notes because it avoids a boot strapping pin.

## Source Notes

The layout above is based on these references:

- [MakerGuides ESP32-C3 SuperMini Board](https://www.makerguides.com/esp32-c3-supermini-board/):
  front/back photos, top-side BOOT/RST/LED placement, and example e-paper SPI
  wiring.
- [ESPBoards ESP32 C3 Super Mini](https://www.espboards.dev/esp32/esp32-c3-super-mini/):
  board specs, common pinout table, flash size, and onboard LED notes.
- [Random Nerd Tutorials ESP32-C3 Super Mini guide](https://randomnerdtutorials.com/getting-started-esp32-c3-super-mini/):
  common pin mapping, boot button behavior, LED GPIO, and clone-board caveat.
- [Espressif ESP32-C3 Technical Reference Manual](https://documentation.espressif.com/esp32-c3_technical_reference_manual_en.pdf):
  boot-mode strapping behavior for `GPIO2`, `GPIO3`, `GPIO8`, and `GPIO9`.
