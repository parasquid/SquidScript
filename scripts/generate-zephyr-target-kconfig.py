#!/usr/bin/env python3
import json
import sys
from pathlib import Path


def fail(message):
    print(f"generate-zephyr-target-kconfig: {message}", file=sys.stderr)
    return 1


def main(argv):
    if len(argv) != 3:
        return fail("usage: generate-zephyr-target-kconfig.py <target.json> <out.conf>")

    target_path = Path(argv[1])
    out_path = Path(argv[2])

    try:
        target = json.loads(target_path.read_text(encoding="utf-8"))
    except OSError as exc:
        return fail(f"cannot read target JSON: {exc}")
    except json.JSONDecodeError as exc:
        return fail(f"invalid target JSON: {exc}")

    features = target.get("features", [])
    if not isinstance(features, list):
        return fail("target JSON features must be a list")
    feature_set = {feature for feature in features if isinstance(feature, str)}
    device_name = target.get("name")
    if not isinstance(device_name, str) or device_name == "":
        device_name = target.get("id")
    if not isinstance(device_name, str) or device_name == "":
        device_name = "SquidScript"
    device_name = device_name.replace("\\", "\\\\").replace('"', '\\"')

    lines = [
        "# Generated from SquidScript target metadata.",
        "# Do not edit by hand.",
        "",
    ]

    if any(feature.startswith("service.wifi.") for feature in feature_set):
        lines.extend(
            [
                "CONFIG_WIFI=y",
                "CONFIG_WIFI_NM=y",
                "",
            ]
        )

    if "service.ble.file-transfer" in feature_set:
        lines.extend(
            [
                "CONFIG_BT=y",
                "CONFIG_BT_PERIPHERAL=y",
                f'CONFIG_BT_DEVICE_NAME="{device_name}"',
                # GATT write callbacks run on the BT RX thread and the
                # file-transfer service opens/writes the LittleFS staging file
                # there, which needs more stack than the 1536-byte BLE default.
                "CONFIG_BT_RX_STACK_SIZE=4096",
                "",
            ]
        )

    devices = target.get("devices", {})
    if isinstance(devices, dict) and any(
        isinstance(device, dict) and isinstance(device.get("pwm"), dict)
        for device in devices.values()
    ):
        lines.extend(
            [
                "CONFIG_PWM=y",
                "",
            ]
        )

    if "display.epaper.ssd1677" in feature_set:
        lines.extend(
            [
                "CONFIG_SQUIDSCRIPT_TARGET_DISPLAY_SSD1677_EXPECTED=y",
                "",
            ]
        )

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text("\n".join(lines), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
