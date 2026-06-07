#!/usr/bin/env python3
import json
import sys
from pathlib import Path


def fail(message):
    print(f"generate-zephyr-target-kconfig: {message}", file=sys.stderr)
    return 1


SECTION_PREFIX = {
    "vm_runtime": "SQ_VM_RUNTIME",
    "app_store": "SQ_APP_STORE",
    "device_protocol": "SQ_DEVICE",
    "serial_transport": "SQ_SERIAL",
    "ffi": "SQVM",
}

KEY_OVERRIDES = {
    ("vm_runtime", "wifi_ssid_len"): "SQ_VM_RUNTIME_WIFI_SSID_LEN",
    ("vm_runtime", "wifi_bssid_len"): "SQ_VM_RUNTIME_WIFI_BSSID_LEN",
    ("vm_runtime", "wifi_auth_len"): "SQ_VM_RUNTIME_WIFI_AUTH_LEN",
    ("vm_runtime", "wifi_ipv4_len"): "SQ_VM_RUNTIME_WIFI_IPV4_LEN",
    ("vm_runtime", "wifi_profile_name_bytes"): "SQ_VM_RUNTIME_WIFI_PROFILE_NAME_BYTES",
    ("vm_runtime", "wifi_profile_ssid_bytes"): "SQ_VM_RUNTIME_WIFI_PROFILE_SSID_BYTES",
    ("vm_runtime", "wifi_profile_password_bytes"): "SQ_VM_RUNTIME_WIFI_PROFILE_PASSWORD_BYTES",
    ("vm_runtime", "work_stack_size"): "SQ_VM_RUNTIME_WORK_STACK_SIZE",
    ("device_protocol", "response_bytes"): "SQ_DEVICE_RESPONSE_BYTES",
    ("device_protocol", "staging_path_bytes"): "SQ_DEVICE_STAGING_PATH_BYTES",
    ("device_protocol", "resource_path_bytes"): "SQ_DEVICE_RESOURCE_PATH_BYTES",
    ("device_protocol", "install_max_bytes"): "SQ_DEVICE_INSTALL_MAX_BYTES",
    ("serial_transport", "max_frame_len"): "SQ_SERIAL_MAX_FRAME_LEN",
    ("ffi", "storage_transfer_capacity"): "SQVM_STORAGE_TRANSFER_CAPACITY",
    ("ffi", "saved_state_capacity"): "SQVM_SAVED_STATE_CAPACITY",
}


def macro_name(section, key):
    override = KEY_OVERRIDES.get((section, key))
    if override is not None:
        return override
    return f"{SECTION_PREFIX[section]}_{key.upper()}"


def emit_runtime_limits(target_path, target, lines):
    zephyr = target.get("firmware", {}).get("zephyr", {})
    runtime_limits = zephyr.get("runtimeLimits")
    if runtime_limits is None:
        return None
    if not isinstance(runtime_limits, str) or runtime_limits == "":
        return "firmware.zephyr.runtimeLimits must be a non-empty path"
    limits_path = (target_path.parent / runtime_limits).resolve()
    if not limits_path.is_file():
        limits_path = (Path.cwd() / runtime_limits).resolve()
    try:
        limits = json.loads(limits_path.read_text(encoding="utf-8"))
    except OSError as exc:
        return f"cannot read runtime limits JSON: {exc}"
    except json.JSONDecodeError as exc:
        return f"invalid runtime limits JSON: {exc}"

    lines.append(f"# Runtime limits from {runtime_limits}.")
    for section in SECTION_PREFIX:
        values = limits.get(section)
        if not isinstance(values, dict):
            continue
        for key in sorted(values):
            if key.startswith("_"):
                continue
            value = values[key]
            if not isinstance(value, int):
                return f"{runtime_limits}: {section}.{key} must be an integer"
            lines.append(f"CONFIG_{macro_name(section, key)}={value}")
    lines.append("")
    return None


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

    runtime_error = emit_runtime_limits(target_path, target, lines)
    if runtime_error is not None:
        return fail(runtime_error)

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
