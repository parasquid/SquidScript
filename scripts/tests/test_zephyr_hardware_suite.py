from pathlib import Path
import json
import os
import re
import stat
import subprocess
import sys
import tempfile

from scripts.tests.zephyr_test_utils import ROOT, ZephyrScriptTestCase


class ZephyrHardwareSuiteTests(ZephyrScriptTestCase):
    def test_hardware_suite_requires_real_zephyr_wifi_backend(self):
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('c3-supermini-test-wifi-state.sh" --require-real-wifi', suite)
        self.assertIn('c3-supermini-test-wifi-scan-api.sh" --require-real-wifi', suite)
        self.assertIn('c3-supermini-test-wifi-list-api.sh" --require-real-wifi', suite)
        self.assertIn("c3-supermini-test-wifi-ap-api.sh", suite)
        self.assertIn("c3-supermini-test-ble-smoke.sh", suite)
        self.assertIn("c3-supermini-test-blinky.sh", suite)
        self.assertLess(suite.index("c3-supermini-test-wifi-list-api.sh"), suite.index("c3-supermini-test-blinky.sh"))
        self.assertLess(suite.index("c3-supermini-test-wifi-list-api.sh"), suite.index("c3-supermini-test-wifi-ap-api.sh"))
        self.assertLess(suite.index("c3-supermini-test-wifi-ap-api.sh"), suite.index("c3-supermini-test-blinky.sh"))
        self.assertLess(suite.index("c3-supermini-test-wifi-ap-api.sh"), suite.index("c3-supermini-test-ble-smoke.sh"))
        self.assertLess(suite.index("c3-supermini-test-ble-smoke.sh"), suite.index("c3-supermini-test-blinky.sh"))

    def test_lazy_load_screen_benchmark_has_portable_contract(self):
        docs = self.read("docs/hardware_benchmarks.md")
        script = self.read("scripts/c3-supermini-benchmark-lazy-load-screen.sh")
        source = self.read("tests/hardware/c3-supermini/lazy-load-screen-benchmark/main.squid")
        worst_source = self.read("tests/hardware/c3-supermini/lazy-load-screen-worst-case/main.squid")

        self.assertIn("Lazy-Load Screen Transition", docs)
        for target in ["ESP32-C3 Super Mini", "nRF52", "RP2350"]:
            self.assertIn(target, docs)
        for metric in [
            "transition_count",
            "dispatch_elapsed_us_min",
            "dispatch_elapsed_us_median",
            "dispatch_elapsed_us_p95",
            "dispatch_elapsed_us_max",
            "sqbc_read_count_total",
            "sqbc_read_bytes_total",
        ]:
            self.assertIn(metric, docs)
            self.assertIn(metric, script)
        self.assertIn("lazy-load-screen-benchmark", script)
        self.assertIn("lazy-load-screen-worst-case", script)
        self.assertIn("MODE", script)
        self.assertIn("device resources", script)
        self.assertIn("last_dispatch_seq", script)
        self.assertNotIn("device key SELECT", script)
        self.assertIn('service.timer.every("timer.transition"', source)
        self.assertIn('service.timer.every("timer.transition"', worst_source)
        self.assertGreaterEqual(source.count('screen("s'), 10)
        self.assertGreaterEqual(worst_source.count('screen("s'), 10)

    def test_wifi_checks_can_require_real_zephyr_wifi_backend(self):
        status = self.read("scripts/c3-supermini-test-wifi-state.sh")
        scan = self.read("scripts/c3-supermini-test-wifi-scan-api.sh")
        list_check = self.read("scripts/c3-supermini-test-wifi-list-api.sh")
        runtime = self.read("firmware/zephyr/src/vm_runtime_wifi.c")

        self.assertIn("--require-real-wifi", status)
        self.assertIn("zephyr true", status)
        self.assertIn("unsupported", status)
        self.assertIn("--require-real-wifi", scan)
        self.assertIn("wifi scan true", scan)
        self.assertNotIn("wifi scan false ", scan)
        self.assertNotIn("clear app-visible scan error", scan)
        self.assertIn("--require-real-wifi", list_check)
        self.assertIn("wifi list true", list_check)
        self.assertNotIn("wifi list false ", list_check)
        self.assertNotIn("clear app-visible scan error", list_check)
        self.assertIn("wifi ap", list_check)
        self.assertIn("assert_no_raw_network_identifiers", list_check)
        self.assertNotIn("SQ_VM_RUNTIME_WIFI_SCAN_REQUEST_UNSAFE", runtime)
        scan_start = runtime.index("int32_t runtime_wifi_scan")
        scan_end = runtime.index("#else", scan_start)
        scan_body = runtime[scan_start:scan_end]
        self.assertIn("runtime_wifi_scan_driver_callback", runtime)
        self.assertIn("runtime_wifi_driver_ops", scan_body)
        self.assertIn("wifi_mgmt_api->scan", scan_body)
        self.assertIn("runtime_wifi_fill_operation(runtime, out)", scan_body)
        self.assertNotIn("k_sem_take", scan_body)
        self.assertNotIn("NET_REQUEST_WIFI_SCAN", scan_body)

    def test_wifi_list_fixture_iterates_redacted_network_records(self):
        source = self.read("tests/hardware/c3-supermini/wifi-list-summary/main.squid")

        self.assertIn("service.wifi.scan()", source)
        self.assertIn("service.wifi.result()", source)
        self.assertIn("service.wifi.scanNetwork(0)", source)
        self.assertIn("first.ssidLength", source)
        self.assertIn("first.channel", source)
        self.assertIn("first.rssi", source)
        self.assertIn("first.auth", source)
        self.assertIn("first.hidden", source)
        self.assertNotIn(".ssid,", source)
        self.assertNotIn(".bssid", source)

    def test_hardware_suite_runs_zephyr_app_lifecycle_before_visible_checks(self):
        lifecycle = self.read("scripts/c3-supermini-test-app-lifecycle.sh")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('source "${ROOT}/scripts/lib/serial-port.sh"', suite)
        self.assertIn('export ESPFLASH_PORT="$(resolve_esp_serial_port)"', suite)
        self.assertIn('source "${ROOT}/scripts/lib/serial-port.sh"', lifecycle)
        self.assertIn('export ESPFLASH_PORT="$(resolve_esp_serial_port)"', lifecycle)
        self.assertIn('cargo run --quiet -p squidc -- app install', lifecycle)
        self.assertIn('cargo run --quiet -p squidc -- app launch main', lifecycle)
        self.assertIn('cargo run --quiet -p squidc -- device lifecycle', lifecycle)
        self.assertIn('cargo run --quiet -p squidc -- --json device lifecycle', lifecycle)
        self.assertIn('process_stack[0]=main', lifecycle)
        self.assertIn('"processStack"', lifecycle)
        self.assertIn('armed_stack[0]=break-reminder timer.break', lifecycle)
        self.assertIn('"armedStack"', lifecycle)
        self.assertNotIn("obsolete", lifecycle.lower())

        diagnostic = suite.index('c3-supermini-zephyr-test-diagnostic.sh')
        lifecycle_check = suite.index('c3-supermini-test-app-lifecycle.sh')
        self.assertLess(diagnostic, lifecycle_check)

    def test_hardware_suite_runs_system_resource_script_before_stack_measurement(self):
        script = self.read("scripts/c3-supermini-test-system-resources.sh")
        app = self.read("tests/hardware/c3-supermini/system-resources/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn("system.memory()", app)
        self.assertIn('system.storage("apps")', app)
        self.assertIn('cargo run --quiet -p squidc -- app install "${SYSTEM_APP}"', script)
        self.assertIn('cargo run --quiet -p squidc -- app launch system-resources', script)
        self.assertIn("output=system memory RAM", script)
        self.assertIn("output=system apps Apps", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertLess(
            suite.index("c3-supermini-test-system-resources.sh"),
            suite.index("c3-supermini-measure-stack-usage.sh"),
        )

    def test_hardware_suite_runs_app_registry_api_after_lifecycle(self):
        script = self.read("scripts/c3-supermini-test-app-registry-api.sh")
        app = self.read("tests/hardware/c3-supermini/app-registry-summary/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")
        roadmap = self.read("ROADMAP.md")

        self.assertIn("app.registry()", app)
        self.assertIn("app.registry.get(apps, 0)", app)
        self.assertIn('source "${ROOT}/scripts/lib/serial-port.sh"', script)
        self.assertIn('export ESPFLASH_PORT="$(resolve_esp_serial_port)"', script)
        self.assertIn(
            'cargo run --quiet -p squidc -- app install "${REGISTRY_APP}"',
            script,
        )
        self.assertIn(
            "cargo run --quiet -p squidc -- app launch app-registry-summary",
            script,
        )
        self.assertIn("output=registry app app-registry-summary", script)
        self.assertIn("output=registry selected id app-registry-summary", script)
        self.assertIn("output=registry selected name app-registry-summary", script)
        self.assertNotIn(
            "output=registry selected app-registry-summary app-registry-summary",
            script,
        )
        self.assertIn("assert_file_empty_command", script)
        self.assertNotIn("app-registry hardware check returning an empty host", roadmap)
        self.assertIn("c3-supermini-test-app-registry-api.sh", suite)
        self.assertLess(
            suite.index("c3-supermini-test-app-lifecycle.sh"),
            suite.index("c3-supermini-test-app-registry-api.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-app-registry-api.sh"),
            suite.index("c3-supermini-measure-stack-usage.sh"),
        )

    def test_hardware_suite_runs_device_config_script_before_stack_measurement(self):
        script = self.read("scripts/c3-supermini-test-device-config.sh")
        app = self.read("tests/hardware/c3-supermini/device-config-summary/main.squid")
        resource = self.read(
            "tests/hardware/c3-supermini/device-config-summary/device/indicator.sqdevice"
        )
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn("device.config.load", app)
        self.assertIn("device.config.set", app)
        self.assertIn("device.config.rebind", app)
        self.assertIn("device.config.save", app)
        self.assertIn("SQDEVICE", resource)
        self.assertIn("indicator.default", resource)
        self.assertIn('cargo run --quiet -p squidc -- package "${DEVICE_CONFIG_APP}"', script)
        self.assertIn(
            'cargo run --quiet -p squidc -- app install "${DEVICE_CONFIG_PACKAGE}"',
            script,
        )
        self.assertIn(
            "cargo run --quiet -p squidc -- app launch device-config-summary",
            script,
        )
        self.assertIn("output=device load true null null", script)
        self.assertIn("output=device set true null null", script)
        self.assertIn("output=device rebind true null null", script)
        self.assertIn("output=device save true null null", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertLess(
            suite.index("c3-supermini-test-device-config.sh"),
            suite.index("c3-supermini-measure-stack-usage.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-device-config.sh"),
            suite.index("c3-supermini-test-blinky.sh"),
        )

    def test_hardware_suite_runs_file_pick_script_before_stack_measurement(self):
        script = self.read("scripts/c3-supermini-test-file-pick.sh")
        app = self.read("tests/hardware/c3-supermini/file-pick-summary/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('file.pickFile(".binbook")', app)
        self.assertIn('file.readText("notes.txt")', app)
        self.assertIn('file.readLines("notes.txt", 4)', app)
        self.assertIn(
            'cargo run --quiet -p squidc -- app install "${FILE_APP}"',
            script,
        )
        self.assertIn(
            "cargo run --quiet -p squidc -- app launch file-pick-summary",
            script,
        )
        self.assertIn("output=file pick false unsupported null", script)
        self.assertIn("output=file text false unsupported null", script)
        self.assertIn("output=file lines false unsupported <list>", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertLess(
            suite.index("c3-supermini-test-file-pick.sh"),
            suite.index("c3-supermini-measure-stack-usage.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-file-pick.sh"),
            suite.index("c3-supermini-test-blinky.sh"),
        )

    def test_hardware_suite_runs_top_level_device_binding_script(self):
        script = self.read("scripts/c3-supermini-test-device-binding.sh")
        app = self.read("tests/hardware/c3-supermini/device-binding-summary/main.squid")
        helper = self.read(
            "tests/hardware/c3-supermini/device-binding-summary/lib/indicator.squid"
        )
        resource = self.read(
            "tests/hardware/c3-supermini/device-binding-summary/device/indicator.sqdevice"
        )
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('device {', app)
        self.assertIn('import indicator from "lib/indicator.squid"', app)
        self.assertIn('indicator { use "device/indicator.sqdevice" }', app)
        self.assertIn('indicator.ready("device binding ready")', app)
        self.assertIn("service.indicator.write(true)", helper)
        self.assertIn("indicator.default", resource)
        self.assertIn("pinName string 5:GPIO8", resource)
        self.assertIn('cargo run --quiet -p squidc -- package "${DEVICE_BINDING_APP}"', script)
        self.assertIn(
            "cargo run --quiet -p squidc -- app launch device-binding-summary",
            script,
        )
        self.assertIn("output=device binding ready", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertIn("c3-supermini-test-device-binding.sh", suite)
        self.assertLess(
            suite.index("c3-supermini-test-system-resources.sh"),
            suite.index("c3-supermini-test-device-binding.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-device-binding.sh"),
            suite.index("c3-supermini-test-blinky.sh"),
        )

    def test_hardware_suite_runs_indicator_state_script(self):
        script = self.read("scripts/c3-supermini-test-indicator-state.sh")
        app = self.read("tests/hardware/c3-supermini/indicator-state-summary/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn("service.indicator.write(false)", app)
        self.assertIn("service.indicator.read()", app)
        self.assertIn("service.indicator.toggle()", app)
        self.assertIn('source "${ROOT}/scripts/lib/serial-port.sh"', script)
        self.assertIn('export ESPFLASH_PORT="$(resolve_esp_serial_port)"', script)
        self.assertIn(
            'cargo run --quiet -p squidc -- app install "${INDICATOR_STATE_APP}"',
            script,
        )
        self.assertIn(
            "cargo run --quiet -p squidc -- app launch indicator-state-summary",
            script,
        )
        self.assertIn("output=indicator read off false", script)
        self.assertIn("output=indicator read on true", script)
        self.assertIn("output=indicator read off again false", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertIn("c3-supermini-test-indicator-state.sh", suite)
        self.assertLess(
            suite.index("c3-supermini-test-indicator-state.sh"),
            suite.index("c3-supermini-test-device-binding.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-indicator-state.sh"),
            suite.index("c3-supermini-test-blinky.sh"),
        )

    def test_hardware_suite_runs_inline_gpio_device_binding_script(self):
        script = self.read("scripts/c3-supermini-test-inline-gpio-binding.sh")
        app = self.read(
            "tests/hardware/c3-supermini/inline-gpio-binding-summary/main.squid"
        )
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn("device {", app)
        self.assertIn('indicator { use "gpio:GPIO8" }', app)
        self.assertIn("service.indicator.write(true)", app)
        self.assertIn(
            'cargo run --quiet -p squidc -- app install "${INLINE_GPIO_APP}"',
            script,
        )
        self.assertIn(
            "cargo run --quiet -p squidc -- app launch inline-gpio-binding-summary",
            script,
        )
        self.assertIn("output=inline gpio binding ready", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertIn("c3-supermini-test-inline-gpio-binding.sh", suite)
        self.assertLess(
            suite.index("c3-supermini-test-device-binding.sh"),
            suite.index("c3-supermini-test-inline-gpio-binding.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-inline-gpio-binding.sh"),
            suite.index("c3-supermini-test-device-config.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-inline-gpio-binding.sh"),
            suite.index("c3-supermini-test-blinky.sh"),
        )

    def test_hardware_suite_runs_inline_gpio10_binding_script(self):
        script = self.read("scripts/c3-supermini-test-inline-gpio10-binding.sh")
        app = self.read(
            "tests/hardware/c3-supermini/inline-gpio10-binding-summary/main.squid"
        )
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('indicator { use "gpio:GPIO10" }', app)
        self.assertIn(
            'cargo run --quiet -p squidc -- app install "${INLINE_GPIO10_APP}"',
            script,
        )
        self.assertIn(
            "cargo run --quiet -p squidc -- app launch inline-gpio10-binding-summary",
            script,
        )
        self.assertIn("output=inline gpio10 binding ready", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertIn("c3-supermini-test-inline-gpio10-binding.sh", suite)
        self.assertLess(
            suite.index("c3-supermini-test-inline-gpio-binding.sh"),
            suite.index("c3-supermini-test-inline-gpio10-binding.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-inline-gpio10-binding.sh"),
            suite.index("c3-supermini-test-unsupported-inline-gpio-binding.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-inline-gpio10-binding.sh"),
            suite.index("c3-supermini-test-blinky.sh"),
        )

    def test_hardware_suite_runs_input_button_script(self):
        script = self.read("scripts/c3-supermini-test-input-button.sh")
        app = self.read("tests/hardware/c3-supermini/input-button-summary/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('input { use "gpio-button:GPIO9:key.SELECT:activeLow" }', app)
        self.assertIn('event.on("key.SELECT")', app)
        self.assertIn("service.indicator.blink(120, 80)", app)
        self.assertIn(
            'cargo run --quiet -p squidc -- app install "${INPUT_BUTTON_APP}"',
            script,
        )
        self.assertIn(
            "cargo run --quiet -p squidc -- app launch input-button-summary",
            script,
        )
        self.assertIn("Press and release the ESP32-C3 Super Mini BOOT/GPIO9 button now.", script)
        self.assertIn("output=count 1", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertIn('COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"', script)
        self.assertIn('WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"', script)
        self.assertIn("while (( SECONDS < deadline )); do", script)
        self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', script)
        self.assertIn('timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" "$@"', script)
        self.assertIn("c3-supermini-test-input-button.sh", suite)
        self.assertIn("[--skip-flash] [--skip-physical-input]", suite)
        self.assertIn("SKIP_PHYSICAL_INPUT=0", suite)
        self.assertIn("--skip-physical-input", suite)
        self.assertIn('if [[ "$SKIP_PHYSICAL_INPUT" != "1" ]]; then', suite)
        self.assertLess(
            suite.index("c3-supermini-test-inline-gpio10-binding.sh"),
            suite.index("c3-supermini-test-input-button.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-wifi-ap-api.sh"),
            suite.index("c3-supermini-test-input-button.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-input-button.sh"),
            suite.index("c3-supermini-test-blinky.sh"),
        )

    def test_gpio9_raw_probe_is_bounded_and_physical_input_only(self):
        script = self.read("scripts/c3-supermini-probe-gpio9-raw.sh")
        app = self.read("tests/hardware/c3-supermini/gpio9-raw-probe/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"', script)
        self.assertIn('WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-20}"', script)
        self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', script)
        self.assertIn("target/hardware-tests/gpio9-raw-probe", script)
        self.assertIn('cargo run --quiet -p squidc -- app install "${GPIO9_RAW_APP}"', script)
        self.assertIn("cargo run --quiet -p squidc -- device reset", script)
        self.assertIn("cargo run --quiet -p squidc -- app launch gpio9-raw-probe", script)
        self.assertIn("wait_for_gpio9_raw", script)
        self.assertIn('run_capture "${label}-launch-${attempt}"', script)
        self.assertIn('run_capture "${label}-output-${attempt}"', script)
        self.assertIn("Press and hold the ESP32-C3 Super Mini BOOT/GPIO9 button", script)
        self.assertIn("short GPIO9 to GND", script)
        self.assertIn("output=gpio9 true", script)
        self.assertIn("output=gpio9 false", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertIn('hardware.gpio.read("GPIO9")', app)
        self.assertIn('debug.print("gpio9", hardware.gpio.read("GPIO9"))', app)
        self.assertNotIn("wifi", script.lower())
        self.assertNotIn("ble", script.lower())
        self.assertNotIn("c3-supermini-probe-gpio9-raw.sh", suite)

    def test_boot_button_pin_scan_probe_is_bounded_and_physical_input_only(self):
        script = self.read("scripts/c3-supermini-probe-boot-button-pins.sh")
        app = self.read("tests/hardware/c3-supermini/boot-button-pin-scan/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"', script)
        self.assertIn('WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"', script)
        self.assertIn('STABLE_SAMPLE_COUNT="${STABLE_SAMPLE_COUNT:-3}"', script)
        self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', script)
        self.assertIn("target/hardware-tests/boot-button-pin-scan", script)
        self.assertIn('cargo run --quiet -p squidc -- app install "${PIN_SCAN_APP}"', script)
        self.assertIn("cargo run --quiet -p squidc -- device reset", script)
        self.assertIn("Press and hold the ESP32-C3 Super Mini BOOT button", script)
        self.assertIn("wait_for_changed_sample", script)
        self.assertIn("stable_count", script)
        self.assertIn("last_changed_sample", script)
        self.assertIn("latest_pin_sample", script)
        self.assertIn("errors-after-timeout", script)
        self.assertIn("output=pin", script)
        self.assertIn('hardware.gpio.read("GPIO9")', app)
        self.assertIn('hardware.gpio.read("GPIO10")', app)
        self.assertIn('debug.print("pin", "GPIO9", hardware.gpio.read("GPIO9"))', app)
        self.assertNotIn("wifi", script.lower())
        self.assertNotIn("bletransfer", script.lower())
        self.assertNotIn("bluetooth", script.lower())
        self.assertNotIn("c3-supermini-probe-boot-button-pins.sh", suite)

    def test_sw0_gpio_button_path_configures_pullup_and_uses_binding_polarity(self):
        runtime = self.read("firmware/zephyr/src/vm_runtime_indicator_gpio.c")
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")
        configure_start = runtime.rindex("int configure_input_button_gpio")
        read_start = runtime.rindex("static int read_input_button_gpio")
        configure_body = runtime[configure_start:read_start]
        read_end = runtime.index("int sq_vm_runtime_hardware_gpio_write")
        read_body = runtime[read_start:read_end]

        self.assertIn("gpio_pin_configure_dt(&input_sw0_gpio, GPIO_INPUT)", configure_body)
        self.assertIn(
            "int raw = gpio_pin_get_raw(input_sw0_gpio.port, input_sw0_gpio.pin)",
            read_body,
        )
        self.assertIn("*pressed = active_low ? raw == 0 : raw != 0", read_body)
        self.assertNotIn("gpio_pin_get_dt(&input_sw0_gpio)", read_body)
        self.assertIn("enum sq_vm_runtime_input_button_phase", runtime_h)
        self.assertIn("SQ_VM_RUNTIME_INPUT_DEBOUNCING_PRESS", runtime_h)
        self.assertIn("SQ_VM_RUNTIME_INPUT_DEBOUNCING_RELEASE", runtime_h)
        self.assertIn("enum sq_vm_runtime_input_button_phase phase;", runtime_h)
        self.assertIn("button->phase = SQ_VM_RUNTIME_INPUT_DEBOUNCING_PRESS", runtime)
        self.assertIn("button->phase = SQ_VM_RUNTIME_INPUT_PRESSED", runtime)
        self.assertIn("button->phase = SQ_VM_RUNTIME_INPUT_RELEASED", runtime)
        self.assertIn("test_input_button_phase_tracks_press_and_release_without_release_dispatch", ztest)

    def test_hardware_suite_runs_unsupported_inline_gpio_binding_script(self):
        script = self.read("scripts/c3-supermini-test-unsupported-inline-gpio-binding.sh")
        app = self.read(
            "tests/hardware/c3-supermini/unsupported-inline-gpio-binding/main.squid"
        )
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('indicator { use "gpio:GPIO18" }', app)
        self.assertIn(
            'cargo run --quiet -p squidc -- app install "${UNSUPPORTED_INLINE_GPIO_APP}"',
            script,
        )
        self.assertIn("wait_for_contains", script)
        self.assertIn("run_capture launch-unsupported-inline-gpio", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertIn('assert_file_contains "${errors_out}" "runtime=host_error"', script)
        self.assertIn("c3-supermini-test-unsupported-inline-gpio-binding.sh", suite)
        self.assertLess(
            suite.index("c3-supermini-test-inline-gpio-binding.sh"),
            suite.index("c3-supermini-test-unsupported-inline-gpio-binding.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-unsupported-inline-gpio-binding.sh"),
            suite.index("c3-supermini-test-device-config.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-unsupported-inline-gpio-binding.sh"),
            suite.index("c3-supermini-test-blinky.sh"),
        )

    def test_breathe_check_is_explicit_visible_indicator_script(self):
        script = self.read("scripts/c3-supermini-test-breathe.sh")
        app = self.read("examples/breathe-supermini/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")
        runtime = self.read("firmware/zephyr/src/vm_runtime_indicator_gpio.c")

        self.assertIn("service.indicator.breathe()", app)
        self.assertIn("#if IS_ENABLED(CONFIG_PWM) && DT_NODE_HAS_PROP(DT_ALIAS(indicator0), pwms)", runtime)
        self.assertNotIn("!defined(CONFIG_SOC_ESP32C3)", runtime)
        self.assertIn('service.timer.after("timer.breathe.marker"', app)
        self.assertIn('event.on("timer.breathe.marker")', app)
        self.assertIn("service.indicator.write(false)", app)
        self.assertIn("service.indicator.write(true)", app)
        self.assertIn("breathe peak marker", app)
        self.assertIn("breathe resume", app)
        self.assertIn('cargo run --quiet -p squidc -- app install "${BREATHE_APP}"', script)
        self.assertIn('cargo run --quiet -p squidc -- app launch breathe-supermini', script)
        self.assertIn("output=breathe ready", script)
        self.assertIn("output=breathe peak marker", script)
        self.assertIn("output=breathe resume", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertIn("breathe app left running", script)
        self.assertNotIn("c3-supermini-test-breathe.sh", suite)

    def test_blink_check_is_explicit_visible_indicator_script(self):
        script = self.read("scripts/c3-supermini-test-blink.sh")
        app = self.read("examples/blink-supermini/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn("service.indicator.blink(120, 80)", app)
        self.assertIn('debug.print("blink ready")', app)
        self.assertIn('cargo run --quiet -p squidc -- app install "${BLINK_APP}"', script)
        self.assertIn('cargo run --quiet -p squidc -- app launch blink-supermini', script)
        self.assertIn("output=blink ready", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertIn("blink app left running", script)
        self.assertNotIn("c3-supermini-test-blink.sh", suite)

    def test_hardware_suite_runs_display_drawlog_script(self):
        script = self.read("scripts/c3-supermini-test-display-drawlog.sh")
        app = self.read("tests/hardware/c3-supermini/display-drawlog/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('cargo run --quiet -p squidc -- app install "${DISPLAY_APP}"', script)
        self.assertIn('cargo run --quiet -p squidc -- app launch display-drawlog', script)
        self.assertIn('cargo run --quiet -p squidc -- device drawlog', script)
        self.assertIn("draw=clear color=gray0", script)
        self.assertIn("draw=select name=status", script)
        self.assertIn('draw=image path="data/icon.bmp" x=20 y=24', script)
        self.assertIn('draw=resource drawable="drawable/page" x=0 y=0', script)
        self.assertIn('service.display.select("status")', app)
        self.assertIn('service.display.image("data/icon.bmp"', app)
        self.assertIn('service.display.draw("drawable/page"', app)
        self.assertLess(
            suite.index("c3-supermini-test-app-lifecycle.sh"),
            suite.index("c3-supermini-test-display-drawlog.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-display-drawlog.sh"),
            suite.index("c3-supermini-test-system-resources.sh"),
        )

    def test_hardware_suite_runs_state_and_key_checks_before_lifecycle(self):
        state = self.read("scripts/c3-supermini-test-app-state.sh")
        foreground = self.read("scripts/c3-supermini-test-foreground-memory.sh")
        foreground_app = self.read("tests/hardware/c3-supermini/foreground-memory/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('cargo run --quiet -p squidc -- app install', state)
        self.assertIn('cargo run --quiet -p squidc -- app launch state-counter', state)
        self.assertIn('cargo run --quiet -p squidc -- device key SELECT', state)
        self.assertIn('cargo run --quiet -p squidc -- device state', state)
        self.assertIn('cargo run --quiet -p squidc -- device reset', state)
        self.assertIn('output=count 2', state)
        self.assertNotIn("obsolete", state.lower())

        self.assertIn('cargo run --quiet -p squidc -- app launch foreground-memory', foreground)
        self.assertIn('cargo run --quiet -p squidc -- device key SELECT', foreground)
        self.assertIn("output=memory start 1", foreground)
        self.assertIn("output=memory select 2", foreground)
        self.assertIn("output=memory select 3", foreground)
        self.assertNotIn("state.load", foreground_app)
        self.assertNotIn("state.save", foreground_app)

        lifecycle = self.read("scripts/c3-supermini-test-app-lifecycle.sh")
        lifecycle_reader = self.read("tests/hardware/c3-supermini/generic-events/reader-clock.squid")
        lifecycle_break = self.read("tests/hardware/c3-supermini/generic-events/break-reminder.squid")
        self.assertIn("output=reader start 1", lifecycle)
        self.assertIn("output=reader process main", lifecycle)
        self.assertIn("output=reader clock 1", lifecycle)
        self.assertIn("output=reader armed break-reminder timer.break", lifecycle)
        self.assertIn("output=reader armed selected break-reminder timer.break", lifecycle)
        self.assertIn("output=break fired 1", lifecycle)
        self.assertIn("output=break process main", lifecycle)
        self.assertIn("output=break process reader-clock", lifecycle)
        self.assertIn("output=break armed empty", lifecycle)
        self.assertIn("reader_start_count", lifecycle)
        self.assertIn("Expected app-exit return to start a fresh reader VM session", lifecycle)
        self.assertIn("app.processStack()", lifecycle_reader)
        self.assertIn("app.armedStack()", lifecycle_reader)
        self.assertIn("app.armedStack.get", lifecycle_reader)
        self.assertIn("app.processStack()", lifecycle_break)
        self.assertIn("app.armedStack()", lifecycle_break)
        self.assertNotIn("state.load", lifecycle_reader)
        self.assertNotIn("state.save", lifecycle_reader)
        self.assertNotIn("state.load", lifecycle_break)
        self.assertNotIn("state.save", lifecycle_break)

        state_check = suite.index('c3-supermini-test-app-state.sh')
        foreground_check = suite.index('c3-supermini-test-foreground-memory.sh')
        lifecycle_check = suite.index('c3-supermini-test-app-lifecycle.sh')
        self.assertLess(state_check, foreground_check)
        self.assertLess(foreground_check, lifecycle_check)
        self.assertLess(state_check, lifecycle_check)

    def test_hardware_suite_measures_stack_after_stateful_workloads(self):
        stack = self.read("scripts/c3-supermini-measure-stack-usage.sh")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('cargo run --quiet -p squidc -- device resources', stack)
        self.assertIn('vm_stack_size_bytes', stack)
        self.assertIn('vm_stack_used_bytes', stack)
        self.assertIn('vm_stack_unused_bytes', stack)
        self.assertIn('stack_used + stack_unused != stack_size', stack)

        lifecycle_check = suite.index('c3-supermini-test-app-lifecycle.sh')
        stack_check = suite.index('c3-supermini-measure-stack-usage.sh')
        system_check = suite.index('c3-supermini-test-system-resources.sh')
        self.assertLess(lifecycle_check, system_check)
        self.assertLess(system_check, stack_check)

    def test_hardware_scripts_use_shared_bounded_command_helper(self):
        scripts_dir = ROOT / "scripts"
        helper = self.read("scripts/lib/hardware-command.sh")
        self.assertIn('timeout "${timeout_seconds}s"', helper)
        self.assertIn('COMMAND_TIMEOUT_SECONDS:-20', helper)
        self.assertIn('Command failed or timed out', helper)
        self.assertIn('sed -n \'1,200p\' "${out}" >&2', helper)
        self.assertIn("capture_device_diagnostics", helper)
        self.assertIn("device resources", helper)
        self.assertIn("device errors", helper)
        self.assertIn("device lifecycle", helper)

        for script_path in sorted(scripts_dir.glob("c3-supermini-*.sh")):
            contents = script_path.read_text(encoding="utf-8")
            if "cargo run --quiet -p squidc -- device" not in contents and (
                "cargo run --quiet -p squidc -- app" not in contents
            ):
                continue

            with self.subTest(script=script_path.name):
                self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', contents)
                self.assertNotIn("\nrun_capture() {\n", contents)

    def test_shared_hardware_command_helper_captures_device_diagnostics_on_failure(self):
        helper = self.read("scripts/lib/hardware-command.sh")

        self.assertIn('capture_device_diagnostics "${name}-failure"', helper)
        self.assertIn('${label}-resources.out', helper)
        self.assertIn('${label}-errors.out', helper)
        self.assertIn('${label}-lifecycle.out', helper)
        self.assertNotIn("device output", helper)

    def test_hardware_scripts_use_deadline_based_polling(self):
        scripts_dir = ROOT / "scripts"

        for script_path in sorted(scripts_dir.glob("c3-supermini-*.sh")):
            contents = script_path.read_text(encoding="utf-8")
            if "cargo run --quiet -p squidc -- device" not in contents and (
                "cargo run --quiet -p squidc -- app" not in contents
            ):
                continue

            with self.subTest(script=script_path.name):
                self.assertNotIn("for _ in $(seq", contents)
                self.assertNotIn("for ((attempt =", contents)
                if "wait_for_" in contents and "while (( SECONDS < deadline )); do" in contents:
                    self.assertIn('WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-', contents)

    def test_hardware_command_helper_reports_captured_output_on_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            script = tmp_path / "probe.sh"
            script.write_text(
                f"""#!/usr/bin/env bash
set -euo pipefail
ROOT={ROOT}
WORK_DIR={tmp_path}
source "${{ROOT}}/scripts/lib/hardware-command.sh"
run_capture failing bash -c 'printf "diagnostic-line\\n"; exit 7'
""",
                encoding="utf-8",
            )
            script.chmod(script.stat().st_mode | stat.S_IXUSR)

            result = subprocess.run(
                [str(script)],
                cwd=ROOT,
                text=True,
                capture_output=True,
            )

        self.assertEqual(result.returncode, 7)
        self.assertIn("Command failed or timed out", result.stderr)
        self.assertIn("--- ", result.stderr)
        self.assertIn("diagnostic-line", result.stderr)

    def test_hardware_command_helper_runs_best_effort_diagnostics_on_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            cargo_log = tmp_path / "cargo.log"
            fake_bin = tmp_path / "bin"
            fake_bin.mkdir()
            self.write_executable(
                fake_bin / "cargo",
                f"""#!/usr/bin/env bash
printf '%s\\n' "$*" >>"{cargo_log}"
exit 0
""",
            )
            script = tmp_path / "probe.sh"
            script.write_text(
                f"""#!/usr/bin/env bash
set -euo pipefail
ROOT={ROOT}
WORK_DIR={tmp_path}
PATH={fake_bin}:$PATH
source "${{ROOT}}/scripts/lib/hardware-command.sh"
run_capture failing bash -c 'printf "diagnostic-line\\n"; exit 7'
""",
                encoding="utf-8",
            )
            script.chmod(script.stat().st_mode | stat.S_IXUSR)

            result = subprocess.run(
                [str(script)],
                cwd=ROOT,
                text=True,
                capture_output=True,
            )

            self.assertEqual(result.returncode, 7)
            log = cargo_log.read_text(encoding="utf-8")
            self.assertIn("run --quiet -p squidc -- device resources", log)
            self.assertIn("run --quiet -p squidc -- device errors", log)
            self.assertIn("run --quiet -p squidc -- device lifecycle", log)
            self.assertTrue((tmp_path / "failing-failure-resources.out").exists())
            self.assertTrue((tmp_path / "failing-failure-errors.out").exists())
            self.assertTrue((tmp_path / "failing-failure-lifecycle.out").exists())

    def test_hardware_output_helper_bounds_device_output_command(self):
        helper = self.read("scripts/lib/hardware-output.sh")

        self.assertIn('timeout "${timeout_seconds}s"', helper)
        self.assertIn('COMMAND_TIMEOUT_SECONDS:-20', helper)
        self.assertIn('HARDWARE_TEST_OUTPUT_WAIT_SECONDS:-20', helper)
        self.assertIn('local deadline=$((SECONDS + wait_seconds))', helper)
        self.assertIn("while (( SECONDS < deadline )); do", helper)
        self.assertNotIn('HARDWARE_TEST_OUTPUT_ATTEMPTS', helper)
        self.assertNotIn("for ((attempt =", helper)
        self.assertIn('cargo run -p squidc -- device output --port "$port"', helper)

    def test_hardware_suite_leaves_blinky_visible_check_last(self):
        blinky = self.read("scripts/c3-supermini-test-blinky.sh")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('examples/blinky-supermini/main.squid', blinky)
        self.assertIn('cargo run --quiet -p squidc -- app launch blinky-supermini', blinky)
        self.assertIn('output=blink true', blinky)
        self.assertIn('output=blink false', blinky)
        self.assertIn('device errors', blinky)

        stack_check = suite.index('c3-supermini-measure-stack-usage.sh')
        blinky_check = suite.index('c3-supermini-test-blinky.sh')
        self.assertLess(stack_check, blinky_check)
        self.assertEqual(suite.strip().splitlines()[-1], '"$ROOT/scripts/c3-supermini-test-blinky.sh"')

    def test_hardware_suite_runs_redacted_wifi_scan_before_blinky(self):
        wifi = self.read("scripts/c3-supermini-test-wifi-scan-api.sh")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('tests/hardware/c3-supermini/wifi-scan-summary/main.squid', wifi)
        self.assertIn('cargo run --quiet -p squidc -- app launch wifi-scan-summary', wifi)
        self.assertIn('output=wifi scan', wifi)
        self.assertIn('assert_no_raw_network_identifiers', wifi)
        self.assertIn("capture_device_diagnostics", wifi)
        self.assertNotIn("capture_timeout_diagnostics", wifi)
        self.assertNotIn("obsolete", wifi.lower())
        self.assertNotIn("wifi ap", wifi)
        self.assertNotIn("app.exit()", self.read("tests/hardware/c3-supermini/wifi-scan-summary/main.squid"))

        stack_check = suite.index('c3-supermini-measure-stack-usage.sh')
        wifi_check = suite.index('c3-supermini-test-wifi-scan-api.sh')
        blinky_check = suite.index('c3-supermini-test-blinky.sh')
        self.assertLess(stack_check, wifi_check)
        self.assertLess(wifi_check, blinky_check)

    def test_ble_smoke_is_target_feature_driven(self):
        target = json.loads(self.read("targets/esp32c3-super-mini.target.json"))
        script = self.read("scripts/c3-supermini-test-ble-smoke.sh")
        docs = self.read("docs/hardware_target_tests.md")

        self.assertIn("service.ble.object-transfer", target["features"])
        self.assertEqual(target["radios"]["ble"]["status"], "runtime-supported-reference")
        self.assertIn("BLE advertising started: ${DEVICE_NAME}", script)
        self.assertIn("bluetoothctl", script)
        self.assertIn("host scan skipped", script)
        self.assertIn("--require-host-scan", script)
        self.assertIn("host scan did not discover", script)
        self.assertNotIn("ble-smoke.conf", script)
        self.assertIn("target metadata", docs)

    def test_wifi_list_timeout_captures_resource_diagnostics(self):
        wifi = self.read("scripts/c3-supermini-test-wifi-list-api.sh")

        self.assertIn('output=wifi list', wifi)
        self.assertIn("capture_device_diagnostics", wifi)
        self.assertNotIn("capture_timeout_diagnostics", wifi)
        self.assertIn('assert_no_raw_network_identifiers', wifi)
        self.assertIn('assert_no_null_auth_rows', wifi)

    def test_input_button_timeout_captures_shared_device_diagnostics(self):
        script = self.read("scripts/c3-supermini-test-input-button.sh")

        self.assertIn("capture_device_diagnostics", script)
        self.assertIn("${label}-timeout", script)
        self.assertIn("${WORK_DIR}/${label}-timeout-resources.out", script)
        self.assertIn("${WORK_DIR}/${label}-timeout-errors.out", script)
        self.assertIn("${WORK_DIR}/${label}-timeout-lifecycle.out", script)

    def test_hardware_suite_runs_redacted_wifi_status_before_scan(self):
        status = self.read("scripts/c3-supermini-test-wifi-state.sh")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('tests/hardware/c3-supermini/wifi-status-summary/main.squid', status)
        self.assertIn('cargo run --quiet -p squidc -- app launch wifi-status-summary', status)
        self.assertIn('output=wifi status', status)
        self.assertIn('assert_no_raw_network_identifiers', status)
        self.assertNotIn("obsolete", status.lower())
        self.assertNotIn("wifi ap", status)
        self.assertNotIn("app.exit()", self.read("tests/hardware/c3-supermini/wifi-status-summary/main.squid"))

        stack_check = suite.index('c3-supermini-measure-stack-usage.sh')
        status_check = suite.index('c3-supermini-test-wifi-state.sh')
        scan_check = suite.index('c3-supermini-test-wifi-scan-api.sh')
        self.assertLess(stack_check, status_check)
        self.assertLess(status_check, scan_check)

    def test_non_scan_hardware_suite_skips_scan_list_and_leaves_blinky_last(self):
        suite = self.read("scripts/c3-supermini-test-hardware-non-scan.sh")
        docs = self.read("docs/hardware_target_tests.md")

        self.assertIn("[--skip-flash] [--skip-physical-input]", suite)
        self.assertIn("SKIP_PHYSICAL_INPUT=0", suite)
        self.assertIn("--skip-physical-input", suite)
        self.assertIn('c3-supermini-test-input-button.sh', suite)
        self.assertIn('if [[ "$SKIP_PHYSICAL_INPUT" != "1" ]]; then', suite)
        self.assertIn('c3-supermini-test-wifi-state.sh" --require-real-wifi', suite)
        self.assertIn('c3-supermini-test-wifi-ap-api.sh', suite)
        self.assertNotIn("c3-supermini-test-wifi-scan-api.sh", suite)
        self.assertNotIn("c3-supermini-test-wifi-list-api.sh", suite)

        stack_check = suite.index('c3-supermini-measure-stack-usage.sh')
        status_check = suite.index('c3-supermini-test-wifi-state.sh')
        ap_check = suite.index('c3-supermini-test-wifi-ap-api.sh')
        blinky_check = suite.index('c3-supermini-test-blinky.sh')
        self.assertLess(stack_check, status_check)
        self.assertLess(status_check, ap_check)
        self.assertLess(ap_check, blinky_check)
        self.assertEqual(suite.strip().splitlines()[-1], '"$ROOT/scripts/c3-supermini-test-blinky.sh"')
        self.assertIn("RAM-confidence subset", docs)
        self.assertIn("does not validate Wi-Fi scan/list", docs)
        self.assertIn("does not validate physical input dispatch", docs)

    def test_wifi_station_check_is_explicit_credentials_only_and_redacted(self):
        station = self.read("scripts/c3-supermini-test-wifi-station-api.sh")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('tests/hardware/c3-supermini/wifi-station-summary/main.squid', station)
        self.assertIn("SQUID_WIFI_STATION_SSID", station)
        self.assertIn("SQUID_WIFI_STATION_PASSWORD", station)
        self.assertIn("credentials not provided; skipping", station)
        self.assertIn('cargo run --quiet -p squidc -- device wifi-profile', station)
        self.assertIn('--ssid-env SQUID_WIFI_STATION_SSID', station)
        self.assertIn('--password-env SQUID_WIFI_STATION_PASSWORD', station)
        self.assertIn('cargo run --quiet -p squidc -- app launch wifi-station-summary', station)
        self.assertIn('output=wifi connect true null', station)
        self.assertIn('output=wifi station dev true', station)
        self.assertIn("unsupported", station)
        self.assertIn('assert_no_raw_network_identifiers', station)
        self.assertNotIn("obsolete", station.lower())
        self.assertNotIn("wifi ap", station)
        self.assertNotIn("SQUID_WIFI_STATION_PASSWORD}", station)
        self.assertNotIn("c3-supermini-test-wifi-station-api.sh", suite)
        self.assertNotIn("app.exit()", self.read("tests/hardware/c3-supermini/wifi-station-summary/main.squid"))

    def test_wifi_ap_check_is_current_redacted_and_in_default_suite(self):
        ap = self.read("scripts/c3-supermini-test-wifi-ap-api.sh")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")
        runtime_c = self.read("firmware/zephyr/src/vm_runtime_wifi.c")

        self.assertIn('tests/hardware/c3-supermini/wifi-ap-summary/main.squid', ap)
        self.assertIn('cargo run --quiet -p squidc -- app launch wifi-ap-summary', ap)
        self.assertIn('cargo run --quiet -p squidc -- device key SELECT', ap)
        self.assertIn('output=wifi start true null', ap)
        self.assertIn('output=wifi ap ip null', ap)
        self.assertIn('output=wifi stop true null', ap)
        self.assertIn("unsupported", ap)
        self.assertIn('assert_no_raw_network_identifiers', ap)
        self.assertNotIn("obsolete", ap.lower())
        self.assertIn("c3-supermini-test-wifi-ap-api.sh", suite)
        self.assertLess(suite.index("c3-supermini-test-wifi-list-api.sh"), suite.index("c3-supermini-test-wifi-ap-api.sh"))
        self.assertLess(suite.index("c3-supermini-test-wifi-ap-api.sh"), suite.index("c3-supermini-test-blinky.sh"))
        self.assertIn("#include <zephyr/net/dhcpv4_server.h>", runtime_c)
        self.assertIn("net_dhcpv4_server_start(iface", runtime_c)
        self.assertIn("net_dhcpv4_server_stop(iface)", runtime_c)

        fixture = self.read("tests/hardware/c3-supermini/wifi-ap-summary/main.squid")
        self.assertIn('service.wifi.startAP("SquidScript")', fixture)
        self.assertIn("service.wifi.getAPIP()", fixture)
        self.assertIn("service.wifi.stopAP()", fixture)
        self.assertNotIn("ip.ip", fixture)
        self.assertNotIn("ip.gw", fixture)
        self.assertNotIn("ip.netmask", fixture)
        self.assertNotIn("status.ssid", fixture)
        self.assertNotIn("app.exit()", fixture)
