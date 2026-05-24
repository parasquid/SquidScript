#!/usr/bin/env python3
import json
import re
import sys
from pathlib import Path


def fail(message):
    print(f"generate-zephyr-target-defaults: {message}", file=sys.stderr)
    return 1


def c_bool(value):
    return "1" if value else "0"


def main(argv):
    if len(argv) != 3:
        return fail("usage: generate-zephyr-target-defaults.py <target.json> <out.h>")

    target_path = Path(argv[1])
    out_path = Path(argv[2])
    try:
        target = json.loads(target_path.read_text(encoding="utf-8"))
    except OSError as exc:
        return fail(f"cannot read target JSON: {exc}")
    except json.JSONDecodeError as exc:
        return fail(f"invalid target JSON: {exc}")

    devices = target.get("devices")
    if not isinstance(devices, dict):
        return fail("target JSON missing devices object")

    indicator = devices.get("indicator.default")
    has_gpio = False
    gpio_pin = 0
    active_low = False
    pwm_frequency_hz = 0

    if isinstance(indicator, dict):
        gpio = indicator.get("gpio")
        if isinstance(gpio, str):
            match = re.fullmatch(r"GPIO([0-9]{1,3})", gpio)
            if match is None:
                return fail("indicator.default gpio must use GPIO<n> form")
            gpio_pin = int(match.group(1))
            if gpio_pin > 255:
                return fail("indicator.default gpio pin must fit in uint8_t")
            has_gpio = True
        active_low = bool(indicator.get("activeLow", False))
        pwm = indicator.get("pwm")
        if isinstance(pwm, dict):
            frequency = pwm.get("frequencyHz", 0)
            if isinstance(frequency, int) and frequency >= 0:
                pwm_frequency_hz = frequency

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(
        "\n".join(
            [
                "/* Generated from SquidScript target metadata. */",
                "#ifndef SQUIDSCRIPT_TARGET_DEFAULTS_H",
                "#define SQUIDSCRIPT_TARGET_DEFAULTS_H",
                "",
                f"#define SQ_TARGET_INDICATOR_DEFAULT_HAS_GPIO {c_bool(has_gpio)}",
                f"#define SQ_TARGET_INDICATOR_DEFAULT_GPIO_PIN {gpio_pin}",
                f"#define SQ_TARGET_INDICATOR_DEFAULT_ACTIVE_LOW {c_bool(active_low)}",
                f"#define SQ_TARGET_INDICATOR_DEFAULT_PWM_FREQUENCY_HZ {pwm_frequency_hz}",
                "",
                "#endif",
                "",
            ]
        ),
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
