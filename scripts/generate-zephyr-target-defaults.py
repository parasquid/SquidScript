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


def parse_args(argv):
    if len(argv) not in (3, 5):
        raise ValueError(
            "usage: generate-zephyr-target-defaults.py <target.json> <out.h> "
            "[--zephyr-overlay <overlay>]"
        )
    overlay_path = None
    if len(argv) == 5:
        if argv[3] != "--zephyr-overlay":
            raise ValueError(f"unknown argument: {argv[3]}")
        overlay_path = Path(argv[4])
    return Path(argv[1]), Path(argv[2]), overlay_path


def validate_overlay(overlay_path, gpio_pin, active_low, pwm_frequency_hz):
    try:
        overlay = overlay_path.read_text(encoding="utf-8")
    except OSError as exc:
        raise ValueError(f"cannot read Zephyr overlay: {exc}") from exc

    gpio_match = re.search(r"gpios\s*=\s*<\s*&gpio0\s+([0-9]+)\s+(GPIO_ACTIVE_LOW|GPIO_ACTIVE_HIGH)\s*>", overlay)
    if gpio_match is None:
        raise ValueError("Zephyr overlay missing indicator gpio-led gpios entry")
    overlay_gpio_pin = int(gpio_match.group(1))
    overlay_active_low = gpio_match.group(2) == "GPIO_ACTIVE_LOW"
    if overlay_gpio_pin != gpio_pin:
        raise ValueError(
            f"indicator.default gpio GPIO{gpio_pin} does not match Zephyr overlay GPIO{overlay_gpio_pin}"
        )
    if overlay_active_low != active_low:
        raise ValueError("indicator.default activeLow does not match Zephyr overlay gpios polarity")

    pinmux_match = re.search(r"LEDC_CH[0-9]+_GPIO([0-9]+)", overlay)
    if pinmux_match is None:
        raise ValueError("Zephyr overlay missing LEDC pinmux for indicator PWM")
    overlay_pwm_pin = int(pinmux_match.group(1))
    if overlay_pwm_pin != gpio_pin:
        raise ValueError(
            f"indicator.default gpio GPIO{gpio_pin} does not match Zephyr overlay PWM GPIO{overlay_pwm_pin}"
        )

    pwm_match = re.search(r"pwms\s*=\s*<\s*&ledc0\s+[0-9]+\s+([0-9]+)\s+PWM_POLARITY_NORMAL\s*>", overlay)
    if pwm_match is None:
        raise ValueError("Zephyr overlay missing indicator PWM entry")
    period_ns = int(pwm_match.group(1))
    overlay_frequency_hz = 0 if period_ns == 0 else 1_000_000_000 // period_ns
    if overlay_frequency_hz != pwm_frequency_hz:
        raise ValueError(
            f"indicator.default PWM frequency {pwm_frequency_hz} does not match Zephyr overlay {overlay_frequency_hz}"
        )


def main(argv):
    try:
        target_path, out_path, overlay_path = parse_args(argv)
    except ValueError as exc:
        return fail(str(exc))

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

    if overlay_path is not None and has_gpio:
        try:
            validate_overlay(overlay_path, gpio_pin, active_low, pwm_frequency_hz)
        except ValueError as exc:
            return fail(str(exc))

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
