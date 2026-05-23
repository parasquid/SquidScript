from pathlib import Path
import os
import stat
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class ZephyrToolingScriptTests(unittest.TestCase):
    def read(self, relative_path):
        return (ROOT / relative_path).read_text(encoding="utf-8")

    def write_executable(self, path, contents):
        path.write_text(contents, encoding="utf-8")
        path.chmod(path.stat().st_mode | stat.S_IXUSR)

    def test_setup_script_uses_project_local_west_and_homebrew_tools(self):
        setup = self.read("scripts/zephyr-setup.sh")

        self.assertIn('SQUID_ZEPHYR_HOME="${SQUID_ZEPHYR_HOME:-${ROOT}/target/zephyr}"', setup)
        self.assertIn('VENV_DIR="${SQUID_ZEPHYR_HOME}/venv"', setup)
        self.assertIn('WORKSPACE_DIR="${SQUID_ZEPHYR_HOME}/workspace"', setup)
        self.assertIn("brew install", setup)
        self.assertIn("cmake", setup)
        self.assertIn("ninja", setup)
        self.assertIn("dtc", setup)
        self.assertIn("wget", setup)
        self.assertIn("xz", setup)
        self.assertIn("pip install", setup)
        self.assertIn("west", setup)
        self.assertIn("west init", setup)
        self.assertIn("west update", setup)
        self.assertIn("west blobs fetch hal_espressif", setup)
        self.assertIn("west sdk install", setup)
        self.assertIn("riscv64-zephyr-elf", setup)
        self.assertNotIn("rpm-ostree", setup)

    def test_env_script_exports_local_west_workspace_and_default_board(self):
        env = self.read("scripts/zephyr-env.sh")

        self.assertIn('SQUID_ZEPHYR_HOME="${SQUID_ZEPHYR_HOME:-${ROOT}/target/zephyr}"', env)
        self.assertIn('export ZEPHYR_BOARD="${ZEPHYR_BOARD:-esp32c3_devkitm/esp32c3}"', env)
        self.assertIn('export ZEPHYR_BUILD_DIR="${ZEPHYR_BUILD_DIR:-${ROOT}/build/zephyr/c3-supermini}"', env)
        self.assertIn('PATH="${SQUID_ZEPHYR_HOME}/venv/bin:${PATH}"', env)
        self.assertIn('ZEPHYR_BASE="${SQUID_ZEPHYR_HOME}/workspace/zephyr"', env)

    def test_zephyr_wrappers_source_shared_env(self):
        for script in [
            "scripts/c3-supermini-zephyr-build.sh",
            "scripts/c3-supermini-zephyr-flash.sh",
            "scripts/c3-supermini-zephyr-monitor.sh",
        ]:
            with self.subTest(script=script):
                contents = self.read(script)
                self.assertIn('source "${ROOT}/scripts/zephyr-env.sh"', contents)

    def test_build_wrapper_applies_supermini_overlay(self):
        build = self.read("scripts/c3-supermini-zephyr-build.sh")

        self.assertIn("DTC_OVERLAY_FILE", build)
        self.assertIn("esp32c3_supermini.overlay", build)

    def test_default_config_enables_real_wifi_scan_status_backend(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        for option in [
            "CONFIG_NETWORKING=y",
            "CONFIG_WIFI=y",
            "CONFIG_WIFI_USAGE_MODE_SCAN_ONLY=y",
            "CONFIG_NET_MGMT=y",
            "CONFIG_NET_MGMT_EVENT=y",
            "CONFIG_NET_MGMT_EVENT_INFO=y",
            "CONFIG_NET_L2_WIFI_MGMT=y",
        ]:
            self.assertIn(option, prj_conf)
        self.assertNotIn("CONFIG_NET_DHCPV4=y", prj_conf)

    def test_hardware_suite_requires_real_zephyr_wifi_backend(self):
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('c3-supermini-test-wifi-state.sh" --require-real-wifi', suite)
        self.assertIn('c3-supermini-test-wifi-scan-api.sh" --require-real-wifi', suite)
        self.assertIn("c3-supermini-test-blinky.sh", suite)

    def test_wifi_checks_can_require_real_zephyr_wifi_backend(self):
        status = self.read("scripts/c3-supermini-test-wifi-state.sh")
        scan = self.read("scripts/c3-supermini-test-wifi-scan-api.sh")

        self.assertIn("--require-real-wifi", status)
        self.assertIn("zephyr true", status)
        self.assertIn("unsupported", status)
        self.assertIn("--require-real-wifi", scan)
        self.assertIn("wifi scan true", scan)
        self.assertIn("unsupported", scan)

    def test_docs_point_to_setup_script(self):
        firmware_readme = self.read("firmware/README.md")
        build_architecture = self.read("docs/firmware_build_architecture.md")

        for contents in [firmware_readme, build_architecture]:
            self.assertIn("scripts/zephyr-setup.sh", contents)
            self.assertIn("SQUID_ZEPHYR_HOME", contents)
            self.assertIn("Homebrew", contents)

    def test_app_kconfig_sources_zephyr_kconfig_tree(self):
        kconfig = self.read("firmware/zephyr/Kconfig")

        self.assertIn('source "Kconfig.zephyr"', kconfig)

    def test_ram_audit_default_guard_tracks_current_esp32c3_budget(self):
        audit = self.read("scripts/zephyr-ram-audit.sh")

        self.assertIn("dram_limit=266240", audit)

    def test_ram_audit_reports_structured_top_symbols(self):
        audit = self.read("scripts/zephyr-ram-audit.sh")

        self.assertIn('SQUID_ZEPHYR_RAM_SYMBOL_COUNT:-20', audit)
        self.assertIn('ram_static_top_symbols=', audit)
        self.assertIn('ram_static_top_bytes=', audit)
        self.assertIn('ram_symbol[', audit)
        self.assertIn('addr=0x', audit)
        self.assertIn('size=', audit)
        self.assertIn('type=', audit)
        self.assertIn('name=', audit)

    def test_ram_audit_derives_default_guard_from_target_sram_percentage(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            elf = tmp_path / "zephyr.elf"
            elf.write_bytes(b"fake")
            size_tool = tmp_path / "fake-size"
            nm_tool = tmp_path / "fake-nm"
            self.write_executable(
                size_tool,
                "#!/usr/bin/env bash\n"
                "cat <<'EOF'\n"
                "section size addr\n"
                "dram0_0_seg 101448 1070071808\n"
                "EOF\n",
            )
            self.write_executable(
                nm_tool,
                "#!/usr/bin/env bash\n"
                "cat <<'EOF'\n"
                "3fc90000 00000200 b z_idle_stacks\n"
                "EOF\n",
            )

            env = os.environ.copy()
            env.update(
                {
                    "SIZE": str(size_tool),
                    "NM": str(nm_tool),
                    "SQUID_ZEPHYR_TARGET_JSON": str(ROOT / "targets/esp32c3-super-mini.target.json"),
                }
            )
            env.pop("SQUID_ZEPHYR_DRAM_LIMIT_BYTES", None)

            result = subprocess.run(
                [str(ROOT / "scripts/zephyr-ram-audit.sh"), str(elf)],
                cwd=ROOT,
                env=env,
                text=True,
                capture_output=True,
                check=True,
            )

        self.assertIn("dram0_0_seg=101448 bytes limit=266240 bytes", result.stdout)
        self.assertIn("target_ram_total_bytes=409600", result.stdout)
        self.assertIn("target_ram_profile_percent=65", result.stdout)
        self.assertIn("target_ram_used_percent=24.8", result.stdout)

    def test_zephyr_main_stack_tracks_measured_protocol_work(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        self.assertIn("CONFIG_MAIN_STACK_SIZE=4096", prj_conf)

    def test_default_runtime_gates_wifi_scan_buffers_from_static_ram(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")

        guard = "#if IS_ENABLED(CONFIG_NET_L2_WIFI_MGMT) && IS_ENABLED(CONFIG_NET_MGMT_EVENT) &&"
        self.assertIn(guard, runtime_h)
        guard_index = runtime_h.index(guard, runtime_h.index("struct sq_vm_runtime"))
        scan_index = runtime_h.index("wifi_scan_networks")
        end_index = runtime_h.index("#endif", scan_index)
        self.assertLess(guard_index, scan_index)
        self.assertLess(runtime_h.index("wifi_scan_sem_initialized"), end_index)

    def test_vm_context_reserve_tracks_current_ffi_size(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")

        self.assertIn("#define SQ_VM_RUNTIME_CONTEXT_BYTES 11264", runtime_h)
        self.assertIn("SQ_VM_RUNTIME_CONTEXT_BYTES <= 11264", ztest)

    def test_repeated_line_responses_use_rust_encoder_without_c_payload_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int repeated_runtime_lines_response")
        end = protocol.index("static int lifecycle_response")
        body = protocol[start:end]

        self.assertIn("sqdp_encode_line_response", body)
        self.assertNotIn("uint8_t payload[512]", body)
        self.assertNotIn("append_string_field(payload", body)

    def test_lifecycle_response_uses_rust_encoder_without_c_payload_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int lifecycle_response")
        end = protocol.index("static int state_get_response")
        body = protocol[start:end]

        self.assertIn("sqdp_encode_lifecycle_response", body)
        self.assertNotIn("uint8_t payload[256]", body)
        self.assertNotIn("append_string_field(payload", body)

    def test_resources_response_uses_rust_encoder_without_c_record_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int resources_response")
        end = protocol.index("static void clear_runtime_context")
        body = protocol[start:end]

        self.assertIn("sqdp_encode_resources_response", body)
        self.assertNotIn("uint8_t record[96]", body)
        self.assertNotIn("append_record_field(payload", body)

    def test_key_dispatch_uses_rust_parser_without_c_payload_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int dispatch_key")
        end = protocol.index("int sq_device_protocol_handle_frame")
        body = protocol[start:end]

        self.assertIn("sqdp_prepare_key_event", body)
        self.assertNotIn("uint8_t event_payload[64]", body)
        self.assertNotIn("append_string_field(event_payload", body)

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

    def test_hardware_suite_runs_state_and_key_checks_before_lifecycle(self):
        state = self.read("scripts/c3-supermini-test-app-state.sh")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('cargo run --quiet -p squidc -- app install', state)
        self.assertIn('cargo run --quiet -p squidc -- app launch state-counter', state)
        self.assertIn('cargo run --quiet -p squidc -- device key SELECT', state)
        self.assertIn('cargo run --quiet -p squidc -- device state', state)
        self.assertIn('cargo run --quiet -p squidc -- device reset', state)
        self.assertIn('output=count 2', state)
        self.assertNotIn("obsolete", state.lower())

        state_check = suite.index('c3-supermini-test-app-state.sh')
        lifecycle_check = suite.index('c3-supermini-test-app-lifecycle.sh')
        self.assertLess(state_check, lifecycle_check)

    def test_hardware_suite_measures_stack_after_stateful_workloads(self):
        stack = self.read("scripts/c3-supermini-measure-stack-usage.sh")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('cargo run --quiet -p squidc -- device resources', stack)
        self.assertIn('vm_worker_stack_size_bytes', stack)
        self.assertIn('vm_worker_stack_used_bytes', stack)
        self.assertIn('vm_worker_stack_unused_bytes', stack)
        self.assertIn('stack_used + stack_unused != stack_size', stack)

        lifecycle_check = suite.index('c3-supermini-test-app-lifecycle.sh')
        stack_check = suite.index('c3-supermini-measure-stack-usage.sh')
        self.assertLess(lifecycle_check, stack_check)

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
        self.assertNotIn("obsolete", wifi.lower())
        self.assertNotIn("wifi ap", wifi)
        self.assertNotIn("app.exit()", self.read("tests/hardware/c3-supermini/wifi-scan-summary/main.squid"))

        stack_check = suite.index('c3-supermini-measure-stack-usage.sh')
        wifi_check = suite.index('c3-supermini-test-wifi-scan-api.sh')
        blinky_check = suite.index('c3-supermini-test-blinky.sh')
        self.assertLess(stack_check, wifi_check)
        self.assertLess(wifi_check, blinky_check)

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
        self.assertIn('output=wifi connect', station)
        self.assertIn('assert_no_raw_network_identifiers', station)
        self.assertNotIn("obsolete", station.lower())
        self.assertNotIn("wifi ap", station)
        self.assertNotIn("SQUID_WIFI_STATION_PASSWORD}", station)
        self.assertNotIn("c3-supermini-test-wifi-station-api.sh", suite)
        self.assertNotIn("app.exit()", self.read("tests/hardware/c3-supermini/wifi-station-summary/main.squid"))

    def test_wifi_ap_check_is_current_redacted_and_not_in_default_suite(self):
        ap = self.read("scripts/c3-supermini-test-wifi-ap-api.sh")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('tests/hardware/c3-supermini/wifi-ap-summary/main.squid', ap)
        self.assertIn('cargo run --quiet -p squidc -- app launch wifi-ap-summary', ap)
        self.assertIn('cargo run --quiet -p squidc -- device key SELECT', ap)
        self.assertIn('output=wifi start', ap)
        self.assertIn('output=wifi ap ip', ap)
        self.assertIn('output=wifi stop', ap)
        self.assertIn('assert_no_raw_network_identifiers', ap)
        self.assertNotIn("obsolete", ap.lower())
        self.assertNotIn("c3-supermini-test-wifi-ap-api.sh", suite)

        fixture = self.read("tests/hardware/c3-supermini/wifi-ap-summary/main.squid")
        self.assertIn('service.wifi.startAP("SquidScript")', fixture)
        self.assertIn("service.wifi.getAPIP()", fixture)
        self.assertIn("service.wifi.stopAP()", fixture)
        self.assertNotIn("ip.ip", fixture)
        self.assertNotIn("ip.gw", fixture)
        self.assertNotIn("ip.netmask", fixture)
        self.assertNotIn("status.ssid", fixture)
        self.assertNotIn("app.exit()", fixture)


if __name__ == "__main__":
    unittest.main()
