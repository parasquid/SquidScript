# XTEINK X4 Target Notes

## Button Detector

The XTEINK X4 has two ADC ladder button inputs plus one direct GPIO power
button. SquidScript exposes a firmware diagnostic through `device resources`
while the app-facing input implementation is still pending.

Capture command:

```sh
DURATION_SECONDS=120 scripts/xteink-x4-capture-buttons.sh
```

The script writes `target/hardware-tests/xteink-x4-buttons/button-capture.log`.
Press each physical button three times, one button at a time. Keep POWER
presses short; the target metadata reserves a 2000 ms POWER long press for
system sleep.

Resource keys:

| Key | Meaning |
| --- | --- |
| `x4.input.adc_gpio1_raw` | GPIO1 12-bit ADC sample. |
| `x4.input.adc_gpio1_logical` | Decoded GPIO1 ladder button. |
| `x4.input.adc_gpio1_error` | Positive errno value from the GPIO1 ADC read, or `0`. |
| `x4.input.adc_gpio2_raw` | GPIO2 12-bit ADC sample. |
| `x4.input.adc_gpio2_logical` | Decoded GPIO2 ladder button. |
| `x4.input.adc_gpio2_error` | Positive errno value from the GPIO2 ADC read, or `0`. |
| `x4.input.power_raw` | Raw GPIO3 level, where `0` is the active-low pressed level. |
| `x4.input.power_pressed` | Logical POWER pressed flag. |
| `x4.input.power_error` | Positive errno value from the GPIO3 read, or `0`. |

Logical enum:

| Value | Button |
| --- | --- |
| `0` | None or outside the current target metadata range. |
| `1` | BACK |
| `2` | SELECT |
| `3` | LEFT |
| `4` | RIGHT |
| `5` | UP |
| `6` | DOWN |

Current decode thresholds come from `targets/xteink-x4.target.json`:

| Input | Button | Raw range |
| --- | --- | --- |
| GPIO1 ADC | BACK | `(3100, 3900]` |
| GPIO1 ADC | SELECT | `(2090, 3100]` |
| GPIO1 ADC | LEFT | `(750, 2090]` |
| GPIO1 ADC | RIGHT | `(-inf, 750]` |
| GPIO2 ADC | UP | `(1120, 3900]` |
| GPIO2 ADC | DOWN | `(-inf, 1120]` |
| GPIO3 GPIO | POWER | active-low digital input |

Observed capture:

| Source | Value |
| --- | --- |
| Log path | `target/hardware-tests/xteink-x4-buttons/button-capture.log` |
| User-reported press order | POWER, UP, DOWN, BACK, SELECT, LEFT, RIGHT |
| Sample interval | Script requested 100 ms polling; observed serial cadence was about 216-224 ms. |
| Idle GPIO1 ADC | `2684`, currently decoded as SELECT by the metadata thresholds. |
| Idle GPIO2 ADC | `2684`, currently decoded as UP by the metadata thresholds. |
| POWER | `power_raw=0`, `power_pressed=1` for two samples. |
| UP | `adc_gpio2_raw=1533..1537`, currently decoded as UP. |
| DOWN | `adc_gpio2_raw=0`, currently decoded as DOWN. |
| BACK | `adc_gpio1_raw=2378..2382` by press order, currently decoded as SELECT. |
| SELECT | `adc_gpio1_raw=1846` by press order, currently decoded as LEFT. |
| LEFT | `adc_gpio1_raw=1031` by press order, currently decoded as LEFT. |
| RIGHT | `adc_gpio1_raw=0`, currently decoded as RIGHT. |

The current thresholds are not sufficient as a runtime press detector. Idle
ladder values decode as pressed buttons, and the GPIO1 ladder order measured on
this board does not match the current metadata thresholds for BACK, SELECT, and
LEFT. The implementation needs a neutral state and calibrated target thresholds
before dispatching logical key events.

Implementation notes:

- Treat the captured raw values as the source of truth for threshold tuning.
- Keep ADC ladder decoding target-specific and expose portable logical button
  events to SquidScript.
- Debounce must compare stable decoded logical states, not only raw ADC deltas.
- Surface ambiguous or out-of-range ADC readings as diagnostics instead of
  silently mapping them to a neighboring button.
