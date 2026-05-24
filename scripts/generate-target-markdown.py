#!/usr/bin/env python3
import json
import re
import sys
from pathlib import Path


def die(message):
    print(f"generate-target-markdown: {message}", file=sys.stderr)
    sys.exit(1)


def pin_sort_key(name):
    match = re.fullmatch(r"GPIO(\d+)", name)
    if match:
        return (0, int(match.group(1)))
    return (1, name)


def availability_for(pin):
    status = pin.get("status", "")
    capabilities = pin.get("capabilities", [])
    if "gpio" not in capabilities:
        return "Truly unavailable"
    if status == "free-to-use":
        return "Free to use"
    if status.startswith("available-with") or status == "typical":
        return "Available with caveats"
    return status or "Unknown"


def table_value(value):
    if value is None:
        return ""
    if isinstance(value, list):
        return ", ".join(f"`{item}`" for item in value) if value else ""
    return f"`{value}`"


def write_markdown(target, target_path, out_path):
    pins = target.get("pins", {})
    devices = target.get("devices", {})
    relative_target = f"targets/{target_path.name}"

    lines = [
        f"# {target.get('name', target.get('id', 'Target'))}",
        "",
        f"Generated from `{relative_target}`. Do not hand-edit this file; update the target JSON and regenerate it.",
        "",
        "## Pin Availability",
        "",
        "| Pin | Availability | Capabilities | Used by | Status | Notes |",
        "| --- | --- | --- | --- | --- | --- |",
    ]

    for name in sorted(pins, key=pin_sort_key):
        pin = pins[name]
        lines.append(
            "| "
            + " | ".join(
                [
                    f"`{name}`",
                    availability_for(pin),
                    table_value(pin.get("capabilities", [])),
                    table_value(pin.get("usedBy", [])),
                    pin.get("status", ""),
                    pin.get("reason", ""),
                ]
            )
            + " |"
        )

    lines.extend(
        [
            "",
            "## Logical Devices",
            "",
            "| Device | Type | GPIO / Pins | Status | Notes |",
            "| --- | --- | --- | --- | --- |",
        ]
    )

    for name in sorted(devices):
        device = devices[name]
        gpio = device.get("gpio")
        pins_value = device.get("pins")
        if gpio is not None:
            pin_summary = f"`{gpio}`"
        elif isinstance(pins_value, dict):
            pin_summary = ", ".join(f"{key}=`{value}`" for key, value in pins_value.items())
        else:
            pin_summary = ""
        lines.append(
            "| "
            + " | ".join(
                [
                    f"`{name}`",
                    device.get("type", ""),
                    pin_summary,
                    device.get("status", ""),
                    device.get("reason", ""),
                ]
            )
            + " |"
        )

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main(argv):
    if len(argv) != 3:
        die("usage: generate-target-markdown.py <target.json> <out.md>")
    target_path = Path(argv[1])
    out_path = Path(argv[2])
    target = json.loads(target_path.read_text(encoding="utf-8"))
    write_markdown(target, target_path, out_path)


if __name__ == "__main__":
    main(sys.argv)
