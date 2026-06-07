from pathlib import Path
import json
import os
import re
import subprocess
import sys
import tempfile

from scripts.tests.zephyr_test_utils import ROOT, ZephyrScriptTestCase


class ZephyrTargetMetadataTests(ZephyrScriptTestCase):
    def generate_target_kconfig(self, target_name):
        generator = ROOT / "scripts/generate-zephyr-target-kconfig.py"
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "target.conf"
            subprocess.run(
                [
                    str(generator),
                    str(ROOT / "targets" / target_name),
                    str(out),
                ],
                cwd=ROOT,
                text=True,
                capture_output=True,
                check=True,
            )
            return out.read_text(encoding="utf-8")

    def test_target_kconfig_enables_declared_radio_backends(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")
        target_conf = self.generate_target_kconfig("esp32c3-super-mini.target.json")

        for option in [
            "CONFIG_WIFI=y",
            "CONFIG_WIFI_NM=y",
            "CONFIG_BT=y",
            "CONFIG_BT_PERIPHERAL=y",
            'CONFIG_BT_DEVICE_NAME="ESP32-C3 Super Mini"',
            "CONFIG_BT_RX_STACK_SIZE=4096",
        ]:
            self.assertIn(option, target_conf)

        for option in [
            "CONFIG_NETWORKING=y",
            "CONFIG_WIFI_USAGE_MODE_STA_AP=y",
            "CONFIG_NET_MGMT=y",
            "CONFIG_NET_MGMT_EVENT=y",
            "CONFIG_NET_MGMT_EVENT_INFO=y",
            "CONFIG_NET_L2_WIFI_MGMT=y",
            "CONFIG_NET_DHCPV4=y",
            "CONFIG_NET_DHCPV4_SERVER=y",
            "CONFIG_NET_UDP=y",
        ]:
            self.assertIn(option, prj_conf)

    def test_target_kconfig_enables_pwm_only_for_declared_pwm_devices(self):
        supermini_conf = self.generate_target_kconfig("esp32c3-super-mini.target.json")
        xiao_conf = self.generate_target_kconfig("xiao-esp32c3-gdeq0426t82-sd.target.json")
        prj_conf = self.read("firmware/zephyr/prj.conf")

        self.assertIn("CONFIG_PWM=y", supermini_conf)
        self.assertNotIn("CONFIG_PWM=y", xiao_conf)
        self.assertNotIn("CONFIG_PWM=y", prj_conf)

    def test_xiao_exposes_pwm_capable_gpio_without_default_pwm_device(self):
        target = json.loads(self.read("targets/xiao-esp32c3-gdeq0426t82-sd.target.json"))
        pins = target["pins"]

        for pin in ["GPIO2", "GPIO3", "GPIO4", "GPIO5", "GPIO6", "GPIO7", "GPIO8", "GPIO9", "GPIO10", "GPIO20", "GPIO21"]:
            with self.subTest(pin=pin):
                self.assertIn("pwm", pins[pin]["capabilities"])

        indicator = target["devices"]["indicator.default"]
        self.assertEqual(indicator["type"], "not-present")
        self.assertFalse(indicator["softwareControllable"])

    def test_zephyr_target_defaults_generator_emits_indicator_metadata(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "squidscript_target_defaults.h"

            subprocess.run(
                [
                    str(ROOT / "scripts/generate-zephyr-target-defaults.py"),
                    str(ROOT / "targets/esp32c3-super-mini.target.json"),
                    str(out),
                ],
                cwd=ROOT,
                text=True,
                capture_output=True,
                check=True,
            )

            header = out.read_text(encoding="utf-8")

        self.assertIn("#define SQ_TARGET_INDICATOR_DEFAULT_HAS_GPIO 1", header)
        self.assertIn("#define SQ_TARGET_INDICATOR_DEFAULT_GPIO_PIN 8", header)
        self.assertIn("#define SQ_TARGET_INDICATOR_DEFAULT_ACTIVE_LOW 1", header)
        self.assertIn("#define SQ_TARGET_INDICATOR_DEFAULT_PWM_FREQUENCY_HZ 1000", header)
        self.assertIn("#define SQ_TARGET_GPIO_CAPABLE_MASK 0x00000000003007ffULL", header)

    def test_supermini_target_json_is_canonical_for_pin_availability(self):
        target = json.loads(self.read("targets/esp32c3-super-mini.target.json"))
        pins = target["pins"]

        for pin in ["GPIO0", "GPIO1", "GPIO4", "GPIO5", "GPIO6", "GPIO7", "GPIO10"]:
            with self.subTest(pin=pin):
                self.assertIn("gpio", pins[pin]["capabilities"])
                self.assertEqual(pins[pin]["status"], "free-to-use")

        for pin in ["GPIO2", "GPIO3", "GPIO8", "GPIO9", "GPIO20", "GPIO21"]:
            with self.subTest(pin=pin):
                self.assertIn("gpio", pins[pin]["capabilities"])
                self.assertTrue(pins[pin]["status"].startswith("available-with"))

        for pin in ["GPIO11", "GPIO12", "GPIO13", "GPIO14", "GPIO15", "GPIO16", "GPIO17", "GPIO18", "GPIO19"]:
            with self.subTest(pin=pin):
                self.assertNotIn("gpio", pins[pin].get("capabilities", []))
                self.assertIn(pins[pin]["status"], ["not-exposed", "reserved"])

    def test_target_markdown_is_generated_from_target_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "esp32c3-super-mini.md"

            subprocess.run(
                [
                    str(ROOT / "scripts/generate-target-markdown.py"),
                    str(ROOT / "targets/esp32c3-super-mini.target.json"),
                    str(out),
                ],
                cwd=ROOT,
                text=True,
                capture_output=True,
                check=True,
            )

            generated = out.read_text(encoding="utf-8")

        checked_in = self.read("docs/targets/esp32c3-super-mini.md")
        self.assertEqual(generated, checked_in)
        self.assertIn("Generated from `targets/esp32c3-super-mini.target.json`", checked_in)
        self.assertIn("| `GPIO10` | Free to use | `gpio` |", checked_in)
        self.assertIn("| `GPIO18` | Truly unavailable | `usb_d-` |", checked_in)
        self.assertIn("| `indicator.default` | pwm-led | `GPIO8` | typical |", checked_in)

    def test_agent_guidance_keeps_target_json_canonical(self):
        agents = self.read("AGENTS.md")

        self.assertIn("Target JSON files are the canonical target descriptions", agents)
        self.assertIn("generate-target-markdown.py", agents)
        self.assertIn("Do not hand-edit generated target Markdown tables", agents)

    def test_zephyr_target_defaults_generator_validates_indicator_overlay(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            out = tmp_path / "squidscript_target_defaults.h"
            bad_overlay = tmp_path / "bad.overlay"
            bad_overlay.write_text(
                self.read("firmware/zephyr/boards/esp32c3_supermini.overlay").replace(
                    "LEDC_CH0_GPIO8", "LEDC_CH0_GPIO10"
                ),
                encoding="utf-8",
            )

            subprocess.run(
                [
                    str(ROOT / "scripts/generate-zephyr-target-defaults.py"),
                    str(ROOT / "targets/esp32c3-super-mini.target.json"),
                    str(out),
                    "--zephyr-overlay",
                    str(ROOT / "firmware/zephyr/boards/esp32c3_supermini.overlay"),
                ],
                cwd=ROOT,
                text=True,
                capture_output=True,
                check=True,
            )
            failed = subprocess.run(
                [
                    str(ROOT / "scripts/generate-zephyr-target-defaults.py"),
                    str(ROOT / "targets/esp32c3-super-mini.target.json"),
                    str(out),
                    "--zephyr-overlay",
                    str(bad_overlay),
                ],
                cwd=ROOT,
                text=True,
                capture_output=True,
            )

        self.assertNotEqual(failed.returncode, 0)
        self.assertIn("indicator.default gpio GPIO8 does not match", failed.stderr)

    def test_zephyr_builds_generate_target_defaults_from_target_json(self):
        app_cmake = self.read("firmware/zephyr/CMakeLists.txt")
        test_cmake = self.read("firmware/zephyr/tests/protocol/CMakeLists.txt")
        runtime = self.read("firmware/zephyr/src/vm_runtime_device_config.c")

        for cmake in [app_cmake, test_cmake]:
            self.assertIn("SQUID_ZEPHYR_TARGET_JSON", cmake)
            self.assertIn("generate-zephyr-target-defaults.py", cmake)
            self.assertIn("squidscript_target_defaults.h", cmake)
            self.assertIn("--zephyr-overlay", cmake)

        self.assertIn('#include "squidscript_target_defaults.h"', runtime)
        self.assertIn("SQ_TARGET_INDICATOR_DEFAULT_GPIO_PIN", runtime)
        self.assertIn("SQ_TARGET_INDICATOR_DEFAULT_ACTIVE_LOW", runtime)
        self.assertNotIn("indicator_gpio.pin;\n\truntime->indicator_binding_active_low", runtime)

    def test_target_default_indicator_binding_does_not_rebind_generated_sqdc(self):
        runtime = self.read("firmware/zephyr/src/vm_runtime_device_config.c")
        default_start = runtime.index(
            "int sq_vm_runtime_apply_target_default_indicator_binding"
        )
        default_body = runtime[
            default_start : runtime.index("void runtime_clear_active_bindings", default_start)
        ]

        self.assertIn("runtime_apply_indicator_gpio_binding", runtime)
        self.assertIn("runtime_device_config_append_string", default_body)
        self.assertIn("runtime_apply_indicator_gpio_binding(runtime,", default_body)
        self.assertNotIn("sq_vm_runtime_device_config_rebind", default_body)
        self.assertNotIn("SqvmDeviceConfigResult result", default_body)

    def test_runtime_limits_header_is_generated_from_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            generated = Path(tmp) / "runtime_limits.h"

            subprocess.run(
                [
                    str(ROOT / "scripts/generate-runtime-limits-header.py"),
                    str(ROOT / "firmware/zephyr/runtime_limits.json"),
                    str(generated),
                ],
                cwd=ROOT,
                text=True,
                capture_output=True,
                check=True,
            )

            generated_text = generated.read_text(encoding="utf-8")

        checked_in = self.read("firmware/zephyr/src/runtime_limits.h")
        self.assertEqual(generated_text, checked_in)
