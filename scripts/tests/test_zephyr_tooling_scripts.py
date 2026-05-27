from pathlib import Path
import json
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

    def test_obsolete_rust_firmware_tree_and_scripts_are_removed(self):
        obsolete_paths = [
            "firmware/squid-firmware",
            "scripts/c3-supermini-test-reference-firmware.sh",
            "scripts/c3-supermini-test-generic-triggered-apps.sh",
            "scripts/c3-supermini-test-persistent-app-registry.sh",
            "scripts/c3-supermini-test-timer-armed-app.sh",
            "experiments/esp32c3-supermini/firmware/wifi-ap-probe",
            "experiments/esp32c3-supermini/firmware/embassy-wifi-ap-probe",
        ]

        for relative_path in obsolete_paths:
            with self.subTest(path=relative_path):
                self.assertFalse((ROOT / relative_path).exists())

        for relative_path in [
            "AGENTS.md",
            "ROADMAP.md",
            "README.md",
            "firmware/README.md",
            "docs/firmware_build_architecture.md",
            "docs/hardware_target_tests.md",
        ]:
            with self.subTest(path=relative_path):
                contents = self.read(relative_path)
                self.assertNotIn("firmware/squid-firmware", contents)
                self.assertNotIn("Rust firmware scripts", contents)

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

    def test_setup_script_installs_twister_protocol_test_dependencies(self):
        setup = self.read("scripts/zephyr-setup.sh")
        docs = self.read("docs/firmware_build_architecture.md")
        roadmap = self.read("ROADMAP.md")
        requirements = self.read("firmware/zephyr/requirements-twister.txt")

        self.assertIn("requirements-build-test.txt", setup)
        self.assertIn("firmware/zephyr/requirements-twister.txt", setup)
        self.assertIn("natsort", requirements)
        self.assertIn("tabulate", requirements)
        self.assertIn("twister", docs.lower())
        self.assertNotIn("missing Twister Python dependencies", roadmap)

    def test_env_script_exports_local_west_workspace_and_default_board(self):
        env = self.read("scripts/zephyr-env.sh")

        self.assertIn('SQUID_ZEPHYR_HOME="${SQUID_ZEPHYR_HOME:-${ROOT}/target/zephyr}"', env)
        self.assertIn('export ZEPHYR_BOARD="${ZEPHYR_BOARD:-esp32c3_supermini}"', env)
        self.assertIn('export ZEPHYR_BUILD_DIR="${ZEPHYR_BUILD_DIR:-${ROOT}/build/zephyr/c3-supermini}"', env)
        self.assertIn('export SQUID_ZEPHYR_TARGET_JSON="${SQUID_ZEPHYR_TARGET_JSON:-${ROOT}/targets/esp32c3-super-mini.target.json}"', env)
        self.assertIn('PATH="${SQUID_ZEPHYR_HOME}/venv/bin:${PATH}"', env)
        self.assertIn('ZEPHYR_BASE="${SQUID_ZEPHYR_HOME}/workspace/zephyr"', env)

    def test_zephyr_wrappers_source_shared_env(self):
        for script in [
            "scripts/c3-supermini-zephyr-build.sh",
            "scripts/c3-supermini-zephyr-flash.sh",
            "scripts/c3-supermini-zephyr-monitor.sh",
            "scripts/zephyr-test-protocol.sh",
        ]:
            with self.subTest(script=script):
                contents = self.read(script)
                self.assertIn('source "${ROOT}/scripts/zephyr-env.sh"', contents)

    def test_protocol_twister_wrapper_uses_64_bit_native_platform(self):
        script = self.read("scripts/zephyr-test-protocol.sh")
        docs = self.read("docs/firmware_build_architecture.md")

        self.assertIn("west twister", script)
        self.assertIn("firmware/zephyr/tests/protocol", script)
        self.assertIn("native_sim/native/64", script)
        self.assertIn("scripts/zephyr-test-protocol.sh", docs)

    def test_build_wrapper_applies_supermini_overlay(self):
        build = self.read("scripts/c3-supermini-zephyr-build.sh")

        self.assertIn("DTC_OVERLAY_FILE", build)
        self.assertIn("esp32c3_supermini.overlay", build)
        self.assertIn("ZEPHYR_PRISTINE", build)
        self.assertNotIn("unverified default", build)

    def test_default_config_enables_real_wifi_scan_status_backend(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        for option in [
            "CONFIG_NETWORKING=y",
            "CONFIG_WIFI=y",
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

    def test_default_config_uses_measured_low_throughput_network_pools(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        for option in [
            "CONFIG_NET_PKT_RX_COUNT=10",
            "CONFIG_NET_PKT_TX_COUNT=10",
            "CONFIG_NET_BUF_RX_COUNT=24",
            "CONFIG_NET_BUF_TX_COUNT=24",
            "CONFIG_NET_MGMT_EVENT_QUEUE_SIZE=8",
        ]:
            self.assertIn(option, prj_conf)

    def test_default_config_uses_measured_wifi_stack_budgets(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        self.assertIn("CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE=1600", prj_conf)
        self.assertIn("CONFIG_NET_MGMT_EVENT_STACK_SIZE=1536", prj_conf)

    def test_default_config_uses_measured_timer_and_rx_stack_budgets(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        self.assertIn("CONFIG_ESP32_TIMER_TASK_STACK_SIZE=3072", prj_conf)
        self.assertIn("CONFIG_NET_RX_STACK_SIZE=1536", prj_conf)

    def test_default_config_uses_bounded_logger_buffer(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        self.assertIn("CONFIG_LOG_BUFFER_SIZE=512", prj_conf)
        self.assertNotIn("CONFIG_LOG_BUFFER_SIZE=1024", prj_conf)

    def test_default_config_uses_bounded_logger_stack(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        self.assertIn("CONFIG_LOG_PROCESS_THREAD_STACK_SIZE=512", prj_conf)
        self.assertNotIn("CONFIG_LOG_PROCESS_THREAD_STACK_SIZE=768", prj_conf)

    def test_default_config_uses_bounded_littlefs_file_pool(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        self.assertIn("CONFIG_FS_LITTLEFS_NUM_FILES=2", prj_conf)
        self.assertNotIn("CONFIG_FS_LITTLEFS_NUM_FILES=4", prj_conf)
        self.assertNotIn("CONFIG_FS_LITTLEFS_NUM_DIRS=2", prj_conf)

    def test_default_config_enables_live_heap_resource_telemetry(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int __noinline resources_response")
        end = protocol.index("static void clear_runtime_context")
        body = protocol[start:end]

        self.assertIn("CONFIG_SYS_HEAP_RUNTIME_STATS=y", prj_conf)
        self.assertIn("sys_heap_runtime_stats_get", body)
        self.assertIn("heap_count", body)
        self.assertIn("heap_free_bytes", body)
        self.assertIn("heap_alloc_bytes", body)
        self.assertIn("heap_max_alloc_bytes", body)
        self.assertIn("proto_stack_pre_unused_bytes", body)
        self.assertIn("proto_stack_pre_used_bytes", body)
        self.assertNotIn("proto_stack_pre_res_unused_bytes", body)
        self.assertNotIn("proto_stack_pre_res_used_bytes", body)
        self.assertNotIn("ram_heap_allocated_bytes", body)
        self.assertNotIn("protocol_thread_stack_pre_resources_unused_bytes", body)
        self.assertNotIn("context->resource_metrics", body)
        self.assertNotIn("context->resource_metric_cap", body)
        self.assertNotIn("SqdpResourceMetric metrics[]", body)
        self.assertNotIn("install_session_bytes", body)
        self.assertNotIn("temp_session_bytes", body)
        self.assertNotIn("resource_session_bytes", body)

    def test_resources_include_last_dispatch_lazy_load_metrics(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        runtime_c = self.read("firmware/zephyr/src/vm_runtime.c")
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")

        for metric in [
            "last_dispatch_seq",
            "last_dispatch_us",
            "last_sqbc_reads",
            "last_sqbc_bytes",
        ]:
            self.assertIn(metric, protocol)
        for field in [
            "last_dispatch_sequence",
            "last_dispatch_elapsed_us",
            "last_dispatch_sqbc_read_count",
            "last_dispatch_sqbc_read_bytes",
        ]:
            self.assertIn(field, runtime_h)
        self.assertIn("k_cycle_get_64", runtime_c)
        self.assertIn("k_cyc_to_us_floor64", runtime_c)
        self.assertIn("dispatch_sqbc_read_count++", runtime_c)
        self.assertIn("dispatch_sqbc_read_bytes += out_len", runtime_c)
        self.assertIn("dispatch_sequence++", runtime_c)

    def test_default_config_uses_measured_system_heap_budget(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        self.assertIn("CONFIG_HEAP_MEM_POOL_SIZE=36864", prj_conf)
        self.assertIn("CONFIG_HEAP_MEM_POOL_IGNORE_MIN=y", prj_conf)
        self.assertNotIn("CONFIG_HEAP_MEM_POOL_ADD_SIZE_ESP_WIFI=", prj_conf)

    def test_hardware_suite_requires_real_zephyr_wifi_backend(self):
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('c3-supermini-test-wifi-state.sh" --require-real-wifi', suite)
        self.assertIn('c3-supermini-test-wifi-scan-api.sh" --require-real-wifi', suite)
        self.assertIn('c3-supermini-test-wifi-list-api.sh" --require-real-wifi', suite)
        self.assertIn("c3-supermini-test-wifi-ap-api.sh", suite)
        self.assertIn("c3-supermini-test-blinky.sh", suite)
        self.assertLess(suite.index("c3-supermini-test-wifi-list-api.sh"), suite.index("c3-supermini-test-blinky.sh"))
        self.assertLess(suite.index("c3-supermini-test-wifi-list-api.sh"), suite.index("c3-supermini-test-wifi-ap-api.sh"))
        self.assertLess(suite.index("c3-supermini-test-wifi-ap-api.sh"), suite.index("c3-supermini-test-blinky.sh"))

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

        self.assertIn("--require-real-wifi", status)
        self.assertIn("zephyr true", status)
        self.assertIn("unsupported", status)
        self.assertIn("--require-real-wifi", scan)
        self.assertIn("wifi scan true", scan)
        self.assertIn("unsupported", scan)
        self.assertIn("--require-real-wifi", list_check)
        self.assertIn("wifi list true", list_check)
        self.assertIn("wifi ap", list_check)
        self.assertIn("assert_no_raw_network_identifiers", list_check)

    def test_wifi_list_fixture_iterates_redacted_network_records(self):
        source = self.read("tests/hardware/c3-supermini/wifi-list-summary/main.squid")

        self.assertIn("service.wifi.scan()", source)
        self.assertIn("for network in scan.networks max 8", source)
        self.assertIn("network.ssidLength", source)
        self.assertIn("network.channel", source)
        self.assertIn("network.rssi", source)
        self.assertIn("network.auth", source)
        self.assertIn("network.hidden", source)
        self.assertNotIn("network.ssid,", source)
        self.assertNotIn("network.bssid", source)

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
        runtime = self.read("firmware/zephyr/src/vm_runtime.c")

        for cmake in [app_cmake, test_cmake]:
            self.assertIn("SQUID_ZEPHYR_TARGET_JSON", cmake)
            self.assertIn("generate-zephyr-target-defaults.py", cmake)
            self.assertIn("squidscript_target_defaults.h", cmake)
            self.assertIn("--zephyr-overlay", cmake)

        self.assertIn('#include "squidscript_target_defaults.h"', runtime)
        self.assertIn("SQ_TARGET_INDICATOR_DEFAULT_GPIO_PIN", runtime)
        self.assertIn("SQ_TARGET_INDICATOR_DEFAULT_ACTIVE_LOW", runtime)
        self.assertNotIn("indicator_gpio.pin;\n\truntime->indicator_binding_active_low", runtime)

    def test_zephyr_main_stack_tracks_measured_protocol_work(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        self.assertIn("CONFIG_MAIN_STACK_SIZE=3264", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=3328", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=3584", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=4096", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=5120", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=6144", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=8192", prj_conf)

    def test_stack_usage_harness_tracks_current_vm_worker_budget(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        stack_script = self.read("scripts/c3-supermini-measure-stack-usage.sh")

        self.assertIn("#define SQ_VM_RUNTIME_WORK_STACK_SIZE 18016", runtime_h)
        self.assertIn('Expected vm_stack_size_bytes=18016', stack_script)
        self.assertNotIn("#define SQ_VM_RUNTIME_WORK_STACK_SIZE 18048", runtime_h)
        self.assertNotIn('Expected vm_worker_stack_size_bytes=18048', stack_script)
        self.assertNotIn("#define SQ_VM_RUNTIME_WORK_STACK_SIZE 18432", runtime_h)
        self.assertNotIn('Expected vm_worker_stack_size_bytes=18432', stack_script)
        self.assertNotIn("#define SQ_VM_RUNTIME_WORK_STACK_SIZE 19456", runtime_h)
        self.assertNotIn('Expected vm_worker_stack_size_bytes=19456', stack_script)
        self.assertNotIn('Expected vm_worker_stack_size_bytes=20480', stack_script)
        self.assertNotIn('Expected vm_worker_stack_size_bytes=16384', stack_script)
        self.assertIn("proto_stack_size_bytes", stack_script)
        self.assertIn('Expected proto_stack_size_bytes=3264', stack_script)
        self.assertNotIn('Expected protocol_thread_stack_size_bytes=3328', stack_script)
        self.assertNotIn('Expected protocol_thread_stack_size_bytes=3584', stack_script)
        self.assertNotIn('Expected protocol_thread_stack_size_bytes=4096', stack_script)
        self.assertNotIn('Expected protocol_thread_stack_size_bytes=5120', stack_script)
        self.assertNotIn('Expected protocol_thread_stack_size_bytes=6144', stack_script)
        self.assertNotIn('Expected protocol_thread_stack_size_bytes=8192', stack_script)
        self.assertIn("proto_stack_pre_used_bytes", stack_script)
        self.assertNotIn("proto_stack_pre_res_used_bytes", stack_script)
        self.assertIn('PROTOCOL_STACK_MIN_UNUSED_BYTES="${PROTOCOL_STACK_MIN_UNUSED_BYTES:-768}"', stack_script)
        self.assertIn('WORKER_STACK_MIN_UNUSED_BYTES="${WORKER_STACK_MIN_UNUSED_BYTES:-384}"', stack_script)
        self.assertIn("Protocol stack headroom below", stack_script)
        self.assertIn("VM worker stack headroom below", stack_script)
        self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', stack_script)
        self.assertIn(
            'resources_out="$(run_capture resources-after-workloads cargo run --quiet -p squidc -- device resources)"',
            stack_script,
        )
        self.assertNotIn(
            'timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" \\\n  cargo run --quiet -p squidc -- device resources',
            stack_script,
        )

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
        limits_rs = self.read("compiler/rust/crates/squidvm-core/src/limits.rs")
        ffi_rs = self.read("compiler/rust/crates/squidvm-ffi/src/lib.rs")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")

        self.assertIn("pub const MAX_RUNTIME_RECORD_FIELDS: usize = 26;", limits_rs)
        self.assertNotIn("pub const MAX_RUNTIME_RECORD_FIELDS: usize = 32;", limits_rs)
        self.assertIn("const ZEPHYR_RUNTIME_CONTEXT_BYTES: usize = 10_400;", ffi_rs)
        self.assertNotIn("const ZEPHYR_RUNTIME_CONTEXT_BYTES: usize = 10_880;", ffi_rs)
        self.assertIn("#define SQ_VM_RUNTIME_CONTEXT_BYTES 10400", runtime_h)
        self.assertIn("SQ_VM_RUNTIME_CONTEXT_BYTES <= 10400", ztest)
        self.assertNotIn("#define SQ_VM_RUNTIME_CONTEXT_BYTES 10880", runtime_h)
        self.assertNotIn("SQ_VM_RUNTIME_CONTEXT_BYTES <= 10880", ztest)
        self.assertNotIn("#define SQ_VM_RUNTIME_CONTEXT_BYTES 11776", runtime_h)
        self.assertNotIn("SQ_VM_RUNTIME_CONTEXT_BYTES <= 11776", ztest)
        self.assertNotIn("#define SQ_VM_RUNTIME_CONTEXT_BYTES 12032", runtime_h)
        self.assertNotIn("SQ_VM_RUNTIME_CONTEXT_BYTES <= 12032", ztest)
        self.assertNotIn("#define SQ_VM_RUNTIME_CONTEXT_BYTES 12288", runtime_h)
        self.assertNotIn("SQ_VM_RUNTIME_CONTEXT_BYTES <= 12288", ztest)

    def test_runtime_reuses_transfer_storage_for_init_scratch_and_completion(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        limits_rs = self.read("compiler/rust/crates/squidvm-core/src/limits.rs")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")
        runtime_body = runtime_h[
            runtime_h.index("struct sq_vm_runtime {") : runtime_h.index(
                "void sq_vm_runtime_init", runtime_h.index("struct sq_vm_runtime {")
            )
        ]

        self.assertIn("#define SQVM_STORAGE_TRANSFER_CAPACITY 768", ffi_h)
        self.assertIn("pub const MAX_CODE_CHUNK_BYTES: usize = 768;", limits_rs)
        self.assertNotIn("#define SQVM_STORAGE_TRANSFER_CAPACITY 1024", ffi_h)
        self.assertNotIn("pub const MAX_CODE_CHUNK_BYTES: usize = 1024;", limits_rs)
        self.assertIn("union sq_vm_runtime_transfer", runtime_h)
        self.assertIn("uint8_t init_scratch[SQ_VM_RUNTIME_SCRATCH_BYTES]", runtime_h)
        self.assertIn("SqvmStorageCompletion completion", runtime_h)
        self.assertNotIn("uint8_t scratch[SQ_VM_RUNTIME_SCRATCH_BYTES];", runtime_body)
        self.assertNotIn("SqvmStorageCompletion completion;", runtime_body)
        self.assertIn("sizeof(runtime.transfer.init_scratch)", ztest)
        self.assertIn("SQVM_STORAGE_TRANSFER_CAPACITY <= 768", ztest)
        self.assertIn("runtime_static <= 14720", ztest)
        self.assertNotIn("runtime_static <= 14736", ztest)
        self.assertNotIn("runtime_static <= 15264", ztest)
        self.assertNotIn("runtime_static <= 16160", ztest)
        self.assertNotIn("runtime_static <= 16240", ztest)
        self.assertNotIn("runtime_static <= 16312", ztest)
        self.assertNotIn("runtime_static <= 16320", ztest)
        self.assertNotIn("runtime_static <= 16344", ztest)
        self.assertNotIn("runtime_static <= 16408", ztest)
        self.assertNotIn("runtime_static <= 16512", ztest)

    def test_runtime_does_not_keep_launch_binding_scratch_resident(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        runtime_body = runtime_h[
            runtime_h.index("struct sq_vm_runtime {") : runtime_h.index(
                "void sq_vm_runtime_init", runtime_h.index("struct sq_vm_runtime {")
            )
        ]

        self.assertNotIn("SqvmDeviceBinding device_binding_scratch;", runtime_body)
        self.assertNotIn("SqdcDeviceBindingPlan device_binding_plan;", runtime_body)
        self.assertNotIn("SqvmDeviceConfigResult device_config_result;", runtime_body)
        self.assertIn("SqdcConfig device_config_draft;", runtime_body)
        runtime_c = self.read("firmware/zephyr/src/vm_runtime.c")
        apply_start = runtime_c.index("static int __noinline sq_vm_runtime_apply_device_bindings")
        apply_body = runtime_c[
            apply_start : runtime_c.index("static int32_t runtime_device_config_load", apply_start)
        ]
        self.assertIn("struct sq_vm_runtime_binding_scratch", runtime_c)
        self.assertIn("sizeof(*scratch) <= sizeof(runtime->transfer.init_scratch)", apply_body)
        self.assertIn(
            "struct sq_vm_runtime_binding_scratch *scratch =",
            apply_body,
        )
        self.assertNotIn("SqvmDeviceBinding binding_storage", apply_body)
        self.assertNotIn("SqdcDeviceBindingPlan plan_storage", apply_body)

    def test_vm_runtime_layout_keeps_small_flags_out_of_alignment_gaps(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        body = runtime_h[
            runtime_h.index("struct sq_vm_runtime {") : runtime_h.index(
                "void sq_vm_runtime_init", runtime_h.index("struct sq_vm_runtime {")
            )
        ]

        self.assertLess(body.index("context_words"), body.index("work_initialized"))
        self.assertLess(body.index("work_initialized"), body.index("context_ready"))
        self.assertLess(body.index("context_ready"), body.index("transfer"))
        self.assertLess(body.index("dispatch_sequence"), body.index("dispatch_exited"))
        self.assertLess(body.index("dispatch_exited"), body.index("current_app"))

    def test_vm_runtime_uses_byte_sized_counts_for_small_fixed_arrays(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        body = runtime_h[
            runtime_h.index("struct sq_vm_runtime {") : runtime_h.index(
                "void sq_vm_runtime_init", runtime_h.index("struct sq_vm_runtime {")
            )
        ]

        for count_field in (
            "return_stack_count",
            "armed_timer_count",
            "active_binding_count",
            "input_button_count",
            "trace_count",
            "output_count",
            "drawlog_count",
        ):
            self.assertIn(f"uint8_t {count_field};", body)
            self.assertNotIn(f"size_t {count_field};", body)

    def test_app_start_binding_setup_runs_on_worker_stack(self):
        runtime_c = self.read("firmware/zephyr/src/vm_runtime.c")
        start_body = runtime_c[
            runtime_c.index("int sq_vm_runtime_start")
            : runtime_c.index(
                "int sq_vm_runtime_record_output",
                runtime_c.index("int sq_vm_runtime_start"),
            )
        ]
        work_body = runtime_c[
            runtime_c.index("static void runtime_work_handler")
            : runtime_c.index("void sq_vm_runtime_init", runtime_c.index("static void runtime_work_handler"))
        ]

        self.assertIn("sq_vm_runtime_prepare_app_start", runtime_c)
        self.assertIn("static int __noinline sq_vm_runtime_prepare_app_start", runtime_c)
        self.assertIn("static int __noinline sq_vm_runtime_apply_saved_device_config", runtime_c)
        self.assertIn("static int __noinline sq_vm_runtime_apply_device_bindings", runtime_c)
        self.assertIn("sq_vm_runtime_prepare_app_start(runtime)", work_body)
        self.assertIn("runtime->start_setup_done", start_body)
        self.assertNotIn("sq_vm_runtime_apply_device_bindings(runtime)", start_body)
        self.assertNotIn("sq_vm_runtime_apply_saved_device_config(runtime)", start_body)
        self.assertNotIn("sq_vm_runtime_apply_target_default_indicator_binding(runtime)", start_body)

    def test_runtime_keeps_bounded_diagnostic_history(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")

        self.assertIn("#define SQ_VM_RUNTIME_TRACE_MAX 4", runtime_h)
        self.assertIn("#define SQ_VM_RUNTIME_TRACE_LEN 26", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_TRACE_MAX 6", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_TRACE_MAX 8", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_TRACE_LEN 25", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_TRACE_LEN 32", runtime_h)
        self.assertIn("#define SQ_VM_RUNTIME_OUTPUT_MAX 5", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_OUTPUT_MAX 6", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_OUTPUT_MAX 8", runtime_h)
        self.assertIn("SQ_VM_RUNTIME_OUTPUT_MAX >= 5", ztest)
        self.assertNotIn("SQ_VM_RUNTIME_OUTPUT_MAX >= 6", ztest)
        self.assertIn("#define SQ_VM_RUNTIME_OUTPUT_LEN 54", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_OUTPUT_MAX 12", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_OUTPUT_LEN 56", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_OUTPUT_LEN 64", runtime_h)
        self.assertIn("#define SQ_VM_RUNTIME_DRAWLOG_MAX 4", runtime_h)
        self.assertIn("#define SQ_VM_RUNTIME_DRAWLOG_LEN 48", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_DRAWLOG_LEN 64", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_DRAWLOG_LEN 96", runtime_h)

    def test_runtime_keeps_physical_input_slots_bounded(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")

        self.assertIn("#define SQ_VM_RUNTIME_ACTIVE_BINDING_MAX 3", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_ACTIVE_BINDING_MAX 4", runtime_h)
        self.assertIn("#define SQ_VM_RUNTIME_INPUT_BUTTON_MAX 2", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_INPUT_BUTTON_MAX 4", runtime_h)
        self.assertIn("#define SQ_VM_RUNTIME_EVENT_LEN 24", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_EVENT_LEN 32", runtime_h)
        self.assertIn('strlen("timer.breathe.marker") < SQ_VM_RUNTIME_EVENT_LEN', ztest)
        self.assertIn("#define SQ_VM_RUNTIME_TIMER_MAX 2", runtime_h)
        self.assertNotIn("#define SQ_VM_RUNTIME_TIMER_MAX 4", runtime_h)

    def test_repeated_line_responses_use_rust_encoder_without_c_payload_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int repeated_runtime_lines_response")
        end = protocol.index("static int __noinline lifecycle_response")
        body = protocol[start:end]

        self.assertIn("sqdp_encode_line_response", body)
        self.assertNotIn("uint8_t payload[512]", body)
        self.assertNotIn("append_string_field(payload", body)

    def test_lifecycle_response_uses_rust_encoder_without_c_payload_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int __noinline lifecycle_response")
        end = protocol.index("static int __noinline state_get_response")
        body = protocol[start:end]

        self.assertIn("sqdp_encode_lifecycle_response", body)
        self.assertIn("offsetof(struct sq_vm_runtime_armed_timer, active)", body)
        self.assertIn("offsetof(struct sq_vm_runtime_armed_timer, app_id)", body)
        self.assertIn("offsetof(struct sq_vm_runtime_armed_timer, event)", body)
        self.assertNotIn("SqdpLifecycleTimer armed_timers[SQ_VM_RUNTIME_ARMED_TIMER_MAX];", body)
        self.assertNotIn("uint8_t payload[256]", body)
        self.assertNotIn("append_string_field(payload", body)

    def test_state_import_uses_rust_parser_without_c_tlv_loop(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        start = protocol.index("static int __noinline state_import")
        end = protocol.index("static int __noinline resources_response")
        body = protocol[start:end]

        self.assertIn("SqdpStateImport", ffi_h)
        self.assertIn("sqdp_parse_state_import_request", ffi_h)
        self.assertIn("sqdp_parse_state_import_request", body)
        self.assertNotIn("sq_protocol_next_field", body)
        self.assertNotIn("struct sq_protocol_field", body)

    def test_state_get_response_uses_rust_encoder_without_c_payload_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        start = protocol.index("static int __noinline state_get_response")
        end = protocol.index("static int __noinline state_import")
        body = protocol[start:end]

        self.assertIn("sqdp_encode_state_response", ffi_h)
        self.assertIn("sqdp_encode_state_response", body)
        self.assertIn("runtime->transfer.completion.bytes", body)
        self.assertNotIn("payload[0]", body)
        self.assertNotIn("SQ_DEVICE_STATE_FIELD_BYTES", body)
        self.assertNotIn("sq_protocol_encode_frame_header", body)

    def test_temp_run_state_uses_file_backed_storage_without_resident_state_buffer(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("struct temp_storage_backend")
        end = protocol.index("static int __noinline commit_install")
        body = protocol[start:end]

        self.assertIn("struct sq_vm_fs_storage fs_storage;", body)
        self.assertIn("sq_vm_fs_storage_backend", body)
        self.assertIn("temp-run.state.tmp", body)
        self.assertNotIn("uint8_t state[SQ_DEVICE_TEMP_STATE_BYTES]", body)
        self.assertNotIn("temp_load_state", body)
        self.assertNotIn("temp_save_state", body)
        self.assertNotIn("temp_reset_state", body)

    def test_resources_response_encodes_without_resident_metric_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int __noinline resources_response")
        end = protocol.index("static void clear_runtime_context")
        body = protocol[start:end]

        self.assertIn("append_resource_metric", body)
        self.assertIn("encode_resource_metrics_header", body)
        self.assertNotIn("sqdp_encode_resources_response", body)
        self.assertNotIn("uint8_t record[96]", body)
        self.assertNotIn("SqdpResourceMetric", body)

    def test_resources_response_reports_input_binding_state(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        header = self.read("firmware/zephyr/src/device_protocol.h")
        stack = self.read("scripts/c3-supermini-measure-input-stack-isolation.sh")
        ffi_rs = self.read("compiler/rust/crates/squidvm-ffi/src/lib.rs")

        self.assertIn("#define SQ_DEVICE_RESPONSE_BYTES 826u", header)
        self.assertNotIn("#define SQ_DEVICE_RESPONSE_BYTES 834u", header)
        self.assertNotIn("#define SQ_DEVICE_RESPONSE_BYTES 848u", header)
        self.assertNotIn("#define SQ_DEVICE_RESPONSE_BYTES 960u", header)
        self.assertNotIn("#define SQ_DEVICE_RESPONSE_BYTES 976u", header)
        self.assertNotIn("#define SQ_DEVICE_RESPONSE_BYTES 928u", header)
        self.assertNotIn("#define SQ_DEVICE_RESPONSE_BYTES 984u", header)
        self.assertNotIn("#define SQ_DEVICE_RESPONSE_BYTES 992u", header)
        self.assertNotIn("#define SQ_DEVICE_RESPONSE_BYTES 1024u", header)
        self.assertIn("#define SQ_DEVICE_RESOURCE_PATH_BYTES 80u", header)
        self.assertNotIn("char resource_path[SQ_APP_STORE_PATH_MAX];", header)
        self.assertIn("const SQDP_PATH_CAP: usize = 80;", ffi_rs)
        self.assertNotIn("const SQDP_PATH_CAP: usize = 128;", ffi_rs)
        self.assertNotIn("SQ_DEVICE_RESOURCE_METRIC_MAX", header)
        self.assertNotIn("SqdpResourceMetric *resource_metrics", header)
        self.assertNotIn('SQ_RESOURCE_METRIC("active_binding_count"', protocol)
        self.assertIn("input_button_state", protocol)
        self.assertNotIn('SQ_RESOURCE_METRIC("input_button_count"', protocol)
        self.assertNotIn('SQ_RESOURCE_METRIC("input_button_pressed_count"', protocol)
        self.assertIn("input_button_state", stack)
        self.assertNotIn("input_button_count", stack)
        self.assertNotIn("input_button_pressed_count", stack)

    def test_app_registry_keeps_constrained_firmware_capacity_bounded(self):
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")

        self.assertIn("#define SQ_APP_STORE_MAX_APPS 8", app_store_h)
        self.assertIn("uint8_t count;", app_store_h)
        self.assertIn("uint32_t sqbc_len;", app_store_h)
        self.assertNotIn("size_t sqbc_len;", app_store_h)
        self.assertIn("format_test_app_store()", ztest)
        self.assertNotIn("#define SQ_APP_STORE_MAX_APPS 4", app_store_h)
        self.assertNotIn("#define SQ_APP_STORE_MAX_APPS 10", app_store_h)
        self.assertNotIn("#define SQ_APP_STORE_MAX_APPS 11", app_store_h)
        self.assertNotIn("#define SQ_APP_STORE_MAX_APPS 12", app_store_h)
        self.assertNotIn("#define SQ_APP_STORE_MAX_APPS 16", app_store_h)
        self.assertNotIn("size_t count;", app_store_h)

    def test_app_id_capacity_is_bounded_across_firmware_protocol_and_ffi(self):
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        ffi_rs = self.read("compiler/rust/crates/squidvm-ffi/src/lib.rs")
        protocol_rs = self.read("compiler/rust/crates/squid-device-protocol/src/lib.rs")

        self.assertIn("#define SQ_APP_STORE_APP_ID_MAX 40", app_store_h)
        self.assertIn("uint8_t app_id[40];", ffi_h)
        self.assertIn("const SQDP_APP_ID_CAP: usize = 40;", ffi_rs)
        self.assertIn("pub const MAX_APP_ID_LEN: usize = 40;", protocol_rs)
        self.assertNotIn("#define SQ_APP_STORE_APP_ID_MAX 48", app_store_h)
        self.assertNotIn("uint8_t app_id[48];", ffi_h)
        self.assertNotIn("const SQDP_APP_ID_CAP: usize = 48;", ffi_rs)
        self.assertNotIn("pub const MAX_APP_ID_LEN: usize = 48;", protocol_rs)

    def test_app_list_entry_uses_32_bit_sqbc_length_across_firmware_and_ffi(self):
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        app_store_c = self.read("firmware/zephyr/src/app_store.c")
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        ffi_rs = self.read("compiler/rust/crates/squidvm-ffi/src/lib.rs")
        protocol_c = self.read("firmware/zephyr/src/device_protocol.c")
        build_doc = self.read("docs/firmware_build_architecture.md")

        self.assertIn("uint32_t sqbc_len;", app_store_h)
        self.assertIn("uint32_t sqbc_len;", ffi_h)
        self.assertIn("pub sqbc_len: u32,", ffi_rs)
        self.assertIn("#if defined(CONFIG_SOC_ESP32C3)", app_store_c)
        self.assertIn("BUILD_ASSERT(sizeof(size_t) == sizeof(uint32_t));", app_store_c)
        self.assertIn("#endif", app_store_c)
        self.assertIn("BUILD_ASSERT(sizeof(struct sq_app_registry_entry) == sizeof(SqdpAppListEntry));", protocol_c)
        self.assertIn("ESP32-C3 firmware target uses a 32-bit `size_t`", build_doc)
        self.assertNotIn("size_t sqbc_len;", app_store_h)
        self.assertNotIn("size_t sqbc_len;", ffi_h)
        self.assertNotIn("pub sqbc_len: usize,", ffi_rs)

    def test_serial_transport_uses_reduced_frame_budget(self):
        serial_h = self.read("firmware/zephyr/src/serial_transport.h")
        cli_serial = self.read("compiler/rust/crates/squidc-cli/src/serial.rs")

        self.assertIn("#define SQ_SERIAL_MAX_FRAME_LEN 256u", serial_h)
        self.assertIn("const FIRMWARE_SERIAL_FRAME_BUDGET: usize = 256;", cli_serial)
        self.assertNotIn("#define SQ_SERIAL_MAX_FRAME_LEN 320u", serial_h)
        self.assertNotIn("const FIRMWARE_SERIAL_FRAME_BUDGET: usize = 320;", cli_serial)
        self.assertNotIn("#define SQ_SERIAL_MAX_FRAME_LEN 384u", serial_h)
        self.assertNotIn("const FIRMWARE_SERIAL_FRAME_BUDGET: usize = 384;", cli_serial)
        self.assertNotIn("#define SQ_SERIAL_MAX_FRAME_LEN 512u", serial_h)
        self.assertNotIn("const FIRMWARE_SERIAL_FRAME_BUDGET: usize = 512;", cli_serial)

    def test_key_dispatch_uses_rust_parser_without_c_payload_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int __noinline dispatch_key")
        end = protocol.index("int sq_device_protocol_handle_frame")
        body = protocol[start:end]

        self.assertIn("sqdp_prepare_key_event", body)
        self.assertNotIn("uint8_t event_payload[64]", body)
        self.assertNotIn("append_string_field(event_payload", body)

    def test_protocol_frame_handler_keeps_error_formatting_out_of_dispatch_frame(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("int sq_device_protocol_handle_frame")
        body = protocol[start:]

        self.assertIn("static int __noinline errors_response", protocol)
        self.assertIn("errors_response(&frame, context->runtime", body)
        self.assertNotIn("char error_line[48]", body)
        self.assertNotIn("const char *lines[1]", body)

    def test_protocol_opcode_handlers_are_out_of_line_to_bound_dispatch_stack(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        handlers = [
            "begin_install",
            "append_install_chunk",
            "commit_install",
            "begin_resource_install",
            "append_resource_chunk",
            "commit_resource_install",
            "commit_temp_run",
            "launch_app",
            "state_get_response",
            "state_import",
            "resources_response",
            "lifecycle_response",
            "reset_runtime",
            "storage_format",
            "dispatch_event_request",
            "dispatch_key",
            "wifi_profile_set",
            "errors_response",
        ]

        for handler in handlers:
            with self.subTest(handler=handler):
                self.assertIn(f"static int __noinline {handler}", protocol)

    def test_app_launch_uses_rust_parser_without_c_tlv_loop(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        start = protocol.index("static int __noinline launch_app")
        end = protocol.index("static int start_installed_app", start)
        body = protocol[start:end]

        self.assertIn("SqdpAppLaunch", ffi_h)
        self.assertIn("sqdp_parse_app_launch_request", ffi_h)
        self.assertIn("sqdp_parse_app_launch_request", body)
        self.assertNotIn("sq_protocol_next_field", body)
        self.assertNotIn("struct sq_protocol_field", body)

    def test_event_dispatch_uses_rust_parser_without_c_tlv_loop(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        start = protocol.index("static int __noinline dispatch_event_request")
        end = protocol.index("static int __noinline dispatch_key")
        body = protocol[start:end]

        self.assertIn("SqdpEventDispatch", ffi_h)
        self.assertIn("sqdp_parse_event_dispatch_request", ffi_h)
        self.assertIn("sqdp_parse_event_dispatch_request", body)
        self.assertNotIn("sq_protocol_next_field", body)
        self.assertNotIn("struct sq_protocol_field", body)

    def test_production_protocol_exposes_only_decode_and_crc_helpers(self):
        protocol_c = self.read("firmware/zephyr/src/protocol.c")
        protocol_h = self.read("firmware/zephyr/src/protocol.h")
        production = protocol_c + "\n" + protocol_h

        self.assertIn("sq_protocol_crc32", production)
        self.assertIn("sq_protocol_decode_frame", production)
        self.assertNotIn("sq_protocol_next_field", production)
        self.assertNotIn("sq_protocol_read_u64_le", production)
        self.assertNotIn("sq_protocol_append_bytes_field", production)
        self.assertNotIn("sq_protocol_append_string_field", production)
        self.assertNotIn("sq_protocol_append_u64_field", production)
        self.assertNotIn("sq_protocol_encode_frame_header", production)

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

    def test_hardware_suite_runs_app_registry_api_script_before_stack_measurement(self):
        script = self.read("scripts/c3-supermini-test-app-registry-api.sh")
        app = self.read("tests/hardware/c3-supermini/app-registry-summary/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

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
        self.assertIn("output=registry selected app-registry-summary app-registry-summary", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertLess(
            suite.index("c3-supermini-test-app-registry-api.sh"),
            suite.index("c3-supermini-measure-stack-usage.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-app-registry-api.sh"),
            suite.index("c3-supermini-test-blinky.sh"),
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

    def test_hardware_suite_runs_content_pick_script_before_stack_measurement(self):
        script = self.read("scripts/c3-supermini-test-content-pick.sh")
        app = self.read("tests/hardware/c3-supermini/content-pick-summary/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('content.pickFile(".binbook")', app)
        self.assertIn('content.readText("notes.txt")', app)
        self.assertIn('content.readLines("notes.txt", 4)', app)
        self.assertIn(
            'cargo run --quiet -p squidc -- app install "${CONTENT_APP}"',
            script,
        )
        self.assertIn(
            "cargo run --quiet -p squidc -- app launch content-pick-summary",
            script,
        )
        self.assertIn("output=content pick false unsupported null", script)
        self.assertIn("output=content text false unsupported null", script)
        self.assertIn("output=content lines false unsupported <list>", script)
        self.assertIn("assert_file_empty_command", script)
        self.assertLess(
            suite.index("c3-supermini-test-content-pick.sh"),
            suite.index("c3-supermini-measure-stack-usage.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-content-pick.sh"),
            suite.index("c3-supermini-test-blinky.sh"),
        )

    def test_hardware_suite_runs_top_level_device_binding_script(self):
        script = self.read("scripts/c3-supermini-test-device-binding.sh")
        app = self.read("tests/hardware/c3-supermini/device-binding-summary/main.squid")
        resource = self.read(
            "tests/hardware/c3-supermini/device-binding-summary/device/indicator.sqdevice"
        )
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn('device {', app)
        self.assertIn('indicator { use "device/indicator.sqdevice" }', app)
        self.assertIn("service.indicator.write(true)", app)
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
            suite.index("c3-supermini-test-app-registry-api.sh"),
            suite.index("c3-supermini-test-indicator-state.sh"),
        )
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
        self.assertLess(
            suite.index("c3-supermini-test-inline-gpio10-binding.sh"),
            suite.index("c3-supermini-test-input-button.sh"),
        )
        self.assertLess(
            suite.index("c3-supermini-test-input-button.sh"),
            suite.index("c3-supermini-test-unsupported-inline-gpio-binding.sh"),
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
        runtime = self.read("firmware/zephyr/src/vm_runtime.c")
        configure_start = runtime.rindex("static int configure_input_button_gpio")
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
        self.assertIn("assert_command_fails_contains", script)
        self.assertIn("unsupported (-95)", script)
        self.assertIn("assert_file_empty_command", script)
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

    def test_device_binding_planning_stays_in_rust_ffi(self):
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        runtime = self.read("firmware/zephyr/src/vm_runtime.c")
        ffi_rs = self.read("compiler/rust/crates/squidvm-ffi/src/lib.rs")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")

        self.assertIn("SqdcDeviceBindingPlan", ffi_h)
        self.assertIn("sqdc_plan_device_binding", ffi_h)
        self.assertIn("sqdc_plan_device_binding", runtime)
        self.assertIn("plan_device_binding_bytes", ffi_rs)
        self.assertIn("test_sqdc_ffi_plans_device_binding_resources", ztest)
        self.assertNotIn("inline_config;", ffi_h)
        self.assertNotIn("sq_vm_runtime_apply_inline_gpio_indicator_binding", runtime)
        ffi_wrapper = ffi_rs[
            ffi_rs.index("pub unsafe extern \"C\" fn sqdc_plan_device_binding")
            : ffi_rs.index("#[repr(C)]\npub struct SqvmContext")
        ]
        planner = ffi_rs[
            ffi_rs.index("fn plan_device_binding_bytes")
            : ffi_rs.index("fn valid_device_binding_name")
        ]
        self.assertNotIn("let mut plan = SqdcDeviceBindingPlan::default();", ffi_wrapper)
        self.assertNotIn("let mut alias = [0u8; SQVM_DEVICE_BINDING_NAME_CAP];", planner)

    def test_breathe_check_is_explicit_visible_indicator_script(self):
        script = self.read("scripts/c3-supermini-test-breathe.sh")
        app = self.read("examples/breathe-supermini/main.squid")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")

        self.assertIn("service.indicator.breathe()", app)
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

    def test_hardware_workload_ram_measurement_captures_attributed_snapshots(self):
        stack = self.read("scripts/c3-supermini-measure-ram-workloads.sh")

        self.assertIn('COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"', stack)
        self.assertIn('WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"', stack)
        self.assertIn("target/hardware-tests/ram-workloads", stack)
        self.assertIn('snapshot_resources after-format', stack)
        self.assertIn('snapshot_resources input-after-install', stack)
        self.assertIn('snapshot_resources input-after-launch', stack)
        self.assertIn('snapshot_resources input-after-select', stack)
        self.assertIn('snapshot_resources display-after-launch', stack)
        self.assertIn('snapshot_resources system-after-launch', stack)
        self.assertIn('snapshot_resources wifi-ap-after-start', stack)
        self.assertIn('snapshot_resources wifi-ap-after-stop', stack)
        self.assertIn('cargo run --quiet -p squidc -- device key SELECT', stack)
        self.assertIn('tests/hardware/c3-supermini/display-drawlog/main.squid', stack)
        self.assertIn('tests/hardware/c3-supermini/system-resources/main.squid', stack)
        self.assertIn('tests/hardware/c3-supermini/wifi-ap-summary/main.squid', stack)
        self.assertIn('proto_stack_used_bytes', stack)
        self.assertIn('proto_stack_pre_used_bytes', stack)
        self.assertNotIn('proto_stack_pre_res_used_bytes', stack)
        self.assertIn('vm_stack_used_bytes', stack)
        self.assertIn('heap_alloc_bytes', stack)
        self.assertIn('heap_max_alloc_bytes', stack)
        self.assertIn('summary.tsv', stack)
        self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', stack)
        self.assertIn('local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))', stack)
        self.assertIn("while (( SECONDS < deadline )); do", stack)
        self.assertNotIn("for _ in $(seq 1 80)", stack)

    def test_input_stack_isolation_measurement_is_bounded_and_input_only(self):
        stack = self.read("scripts/c3-supermini-measure-input-stack-isolation.sh")

        self.assertIn('COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"', stack)
        self.assertIn('WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"', stack)
        self.assertIn('INPUT_BUTTON_APP="${INPUT_BUTTON_APP:-', stack)
        self.assertIn('INPUT_BUTTON_APP_ID="${INPUT_BUTTON_APP_ID:-input-button-summary}"', stack)
        self.assertIn('INPUT_BUTTON_LABEL="${INPUT_BUTTON_LABEL:-ESP32-C3 Super Mini BOOT/GPIO9 button}"', stack)
        self.assertIn("target/hardware-tests/input-stack-isolation", stack)
        self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', stack)
        self.assertIn('source "${ROOT}/scripts/lib/serial-port.sh"', stack)
        self.assertIn('c3-supermini-zephyr-test-diagnostic.sh', stack)
        self.assertIn('snapshot_resources after-boot', stack)
        self.assertIn('snapshot_resources after-format', stack)
        self.assertIn('snapshot_resources after-install', stack)
        self.assertIn('snapshot_resources after-launch', stack)
        self.assertIn('wait_for_input_released', stack)
        self.assertIn('wait_for_input_pressed', stack)
        self.assertIn('snapshot_resources after-release', stack)
        self.assertIn('snapshot_resources after-release-timeout', stack)
        self.assertIn('snapshot_resources after-press-observed', stack)
        self.assertIn('snapshot_resources after-press', stack)
        self.assertIn('snapshot_resources after-press-timeout', stack)
        self.assertIn('snapshot_resources after-dispatch-timeout', stack)
        self.assertIn('snapshot_resources after-final-release-timeout', stack)
        self.assertIn('errors-after-release-timeout', stack)
        self.assertIn('errors-after-press-timeout', stack)
        self.assertIn('errors-after-dispatch-timeout', stack)
        self.assertIn('errors-after-final-release-timeout', stack)
        self.assertIn('tests/hardware/c3-supermini/input-button-summary/main.squid', stack)
        self.assertIn('tests/hardware/c3-supermini/input-button-gpio5-summary/main.squid', stack)
        self.assertIn(
            'Press and hold ${INPUT_BUTTON_LABEL}, or short GPIO9 to GND, until this script asks you to release it.',
            stack,
        )
        self.assertIn('or short GPIO9 to GND', stack)
        self.assertIn('Release %s now.', stack)
        self.assertNotIn('cargo run --quiet -p squidc -- device key SELECT', stack)
        self.assertIn('proto_stack_pre_used_bytes', stack)
        self.assertNotIn('proto_stack_pre_res_used_bytes', stack)
        self.assertIn('vm_stack_used_bytes', stack)
        self.assertIn('heap_alloc_bytes', stack)
        self.assertIn('summary.tsv', stack)
        self.assertNotIn("wifi", stack.lower())

    def test_hardware_scripts_use_shared_bounded_command_helper(self):
        scripts_dir = ROOT / "scripts"
        helper = self.read("scripts/lib/hardware-command.sh")
        self.assertIn('timeout "${timeout_seconds}s"', helper)
        self.assertIn('COMMAND_TIMEOUT_SECONDS:-20', helper)
        self.assertIn('Command failed or timed out', helper)
        self.assertIn('sed -n \'1,200p\' "${out}" >&2', helper)

        for script_path in sorted(scripts_dir.glob("c3-supermini-*.sh")):
            contents = script_path.read_text(encoding="utf-8")
            if "cargo run --quiet -p squidc -- device" not in contents and (
                "cargo run --quiet -p squidc -- app" not in contents
            ):
                continue

            with self.subTest(script=script_path.name):
                self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', contents)
                self.assertNotIn("\nrun_capture() {\n", contents)

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

    def test_c3_stack_usage_attribution_is_opt_in_and_reportable(self):
        build = self.read("scripts/c3-supermini-zephyr-build.sh")
        cmake = self.read("firmware/zephyr/CMakeLists.txt")
        docs = self.read("docs/firmware_build_architecture.md")
        report = ROOT / "scripts/c3-supermini-stack-usage-report.sh"

        self.assertTrue(report.exists())
        self.assertIn("SQUID_ZEPHYR_STACK_USAGE", build)
        self.assertIn("-DSQUID_ZEPHYR_STACK_USAGE=ON", build)
        self.assertIn("SQUID_ZEPHYR_STACK_USAGE", cmake)
        self.assertIn("-fstack-usage", cmake)
        self.assertIn("scripts/c3-supermini-stack-usage-report.sh", docs)
        self.assertIn("SQUID_ZEPHYR_STACK_USAGE=1 scripts/c3-supermini-build.sh", docs)

    def test_c3_stack_usage_report_sorts_largest_functions(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            (tmp_path / "small.su").write_text(
                "src/main.c:10:1:small\t32\tstatic\n",
                encoding="utf-8",
            )
            (tmp_path / "large.su").write_text(
                "src/device_protocol.c:20:1:large\t160\tstatic\n",
                encoding="utf-8",
            )

            result = subprocess.run(
                [
                    str(ROOT / "scripts/c3-supermini-stack-usage-report.sh"),
                    str(tmp_path),
                    "2",
                ],
                cwd=ROOT,
                text=True,
                capture_output=True,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        lines = [line for line in result.stdout.splitlines() if line.strip()]
        self.assertEqual(lines[0], "bytes\tfunction\tlocation\tmode")
        self.assertTrue(lines[1].startswith("160\tlarge\tsrc/device_protocol.c:20:1\tstatic"))
        self.assertTrue(lines[2].startswith("32\tsmall\tsrc/main.c:10:1\tstatic"))

    def test_c3_stack_usage_report_limit_does_not_trip_pipefail(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            lines = [
                f"src/file.c:{index}:1:function_{index}\t{index + 1}\tstatic\n"
                for index in range(10000)
            ]
            (tmp_path / "many.su").write_text("".join(lines), encoding="utf-8")

            result = subprocess.run(
                [
                    str(ROOT / "scripts/c3-supermini-stack-usage-report.sh"),
                    str(tmp_path),
                    "5",
                ],
                cwd=ROOT,
                text=True,
                capture_output=True,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        lines = [line for line in result.stdout.splitlines() if line.strip()]
        self.assertEqual(len(lines), 6)
        self.assertTrue(lines[1].startswith("10000\tfunction_9999\t"))
        self.assertTrue(lines[5].startswith("9996\tfunction_9995\t"))

    def test_app_registry_scan_uses_narrow_path_scratch_after_opening_directory(self):
        app_store = self.read("firmware/zephyr/src/app_store.c")
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        start = app_store.index("int sq_app_store_scan_registry")
        end = app_store.index("static int delete_files_under")
        body = app_store[start:end]

        self.assertIn("#define SQ_APP_STORE_APP_FILE_PATH_MAX 64", app_store_h)
        self.assertNotIn("#define SQ_APP_STORE_APP_FILE_PATH_MAX 72", app_store_h)
        self.assertIn("char path[SQ_APP_STORE_APP_FILE_PATH_MAX];", body)
        self.assertNotIn("char path[SQ_APP_STORE_PATH_MAX];", body)
        self.assertNotIn("char apps_path[SQ_APP_STORE_PATH_MAX];", body)
        self.assertNotIn("char sqbc_path[SQ_APP_STORE_PATH_MAX];", body)
        self.assertNotIn("struct fs_dirent sqbc_entry", body)
        self.assertIn('join_path2(path, sizeof(path), mount_point, "apps")', body)
        self.assertIn("fs_opendir(&dir, path)", body)
        self.assertIn('format_app_path(path, sizeof(path), mount_point, entry.name,', body)
        self.assertIn("fs_stat(path, &entry)", body)
        self.assertIn("struct sq_app_registry_entry *record = NULL;", body)
        self.assertIn("record = &registry->apps[registry->count];", body)
        self.assertIn("registry->count++", body)

    def test_app_store_vm_storage_uses_narrow_installed_app_path_buffers(self):
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        app_store_c = self.read("firmware/zephyr/src/app_store.c")

        self.assertIn("#define SQ_APP_STORE_APP_FILE_PATH_MAX 64", app_store_h)
        self.assertNotIn("#define SQ_APP_STORE_APP_FILE_PATH_MAX 72", app_store_h)
        self.assertIn("#define SQ_APP_STORE_APP_STATE_PATH_MAX 60", app_store_h)
        self.assertNotIn("#define SQ_APP_STORE_APP_STATE_PATH_MAX 64", app_store_h)
        self.assertIn("char sqbc_path[SQ_APP_STORE_APP_FILE_PATH_MAX];", app_store_h)
        self.assertIn("char state_path[SQ_APP_STORE_APP_STATE_PATH_MAX];", app_store_h)
        self.assertIn("format_app_path(storage->sqbc_path, sizeof(storage->sqbc_path)", app_store_c)
        self.assertIn("format_state_path(storage->state_path, sizeof(storage->state_path)", app_store_c)
        self.assertNotIn("char sqbc_path[SQ_APP_STORE_PATH_MAX];", app_store_h)
        self.assertNotIn("char state_path[SQ_APP_STORE_PATH_MAX];", app_store_h)

    def test_trigger_registration_uses_narrow_app_sqbc_path_scratch(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        start = protocol.index("static int __noinline register_app_triggers")
        end = protocol.index("int sq_device_protocol_poll")
        body = protocol[start:end]

        self.assertIn("#define SQ_APP_STORE_APP_FILE_PATH_MAX 64", app_store_h)
        self.assertNotIn("#define SQ_APP_STORE_APP_FILE_PATH_MAX 72", app_store_h)
        self.assertIn("char sqbc_path[SQ_APP_STORE_APP_FILE_PATH_MAX];", body)
        self.assertNotIn("char sqbc_path[SQ_APP_STORE_PATH_MAX];", body)

    def test_resource_install_paths_reuse_single_path_scratch(self):
        app_store = self.read("firmware/zephyr/src/app_store.c")

        commit_start = app_store.index("int sq_app_store_commit_staged_resource")
        commit_end = app_store.index("int sq_app_store_resource_path")
        commit_body = app_store[commit_start:commit_end]
        self.assertIn("char path[SQ_APP_STORE_PATH_MAX];", commit_body)
        self.assertIn("struct fs_file_t main_sqbc;", commit_body)
        self.assertNotIn("struct fs_dirent entry;", commit_body)
        self.assertNotIn("char sqbc_path[SQ_APP_STORE_PATH_MAX];", commit_body)
        self.assertNotIn("char final_path[SQ_APP_STORE_PATH_MAX];", commit_body)
        self.assertIn('format_app_path(path, sizeof(path), mount_point, app_id, "main.sqbc")', commit_body)
        self.assertIn("fs_open(&main_sqbc, path, FS_O_READ)", commit_body)
        self.assertIn("sq_app_store_resource_path(mount_point, app_id, resource_path, path,", commit_body)
        self.assertIn("fs_rename(staging_path, path)", commit_body)

        install_start = app_store.index("int sq_app_store_install_resource")
        install_end = app_store.index("int sq_app_store_scan_registry")
        install_body = app_store[install_start:install_end]
        self.assertIn("char path[SQ_APP_STORE_PATH_MAX];", install_body)
        self.assertIn("struct fs_file_t main_sqbc;", install_body)
        self.assertNotIn("struct fs_dirent entry;", install_body)
        self.assertNotIn("char sqbc_path[SQ_APP_STORE_PATH_MAX];", install_body)
        self.assertIn('format_app_path(path, sizeof(path), mount_point, app_id, "main.sqbc")', install_body)
        self.assertIn("fs_open(&main_sqbc, path, FS_O_READ)", install_body)
        self.assertIn("sq_app_store_resource_path(mount_point, app_id, resource_path, path,", install_body)
        self.assertIn("write_file(path, bytes, len)", install_body)

    def test_file_backed_state_and_config_reads_avoid_dirent_size_probe(self):
        storage = self.read("firmware/zephyr/src/vm_fs_storage.c")
        runtime = self.read("firmware/zephyr/src/vm_runtime.c")

        storage_start = storage.index("static int read_optional_file")
        storage_end = storage.index("static int write_file")
        storage_body = storage[storage_start:storage_end]
        self.assertIn("struct fs_file_t file;", storage_body)
        self.assertNotIn("struct fs_dirent entry;", storage_body)
        self.assertNotIn("fs_stat(path, &entry)", storage_body)
        self.assertIn("uint8_t overflow;", storage_body)
        self.assertIn("fs_read(&file, &overflow, sizeof(overflow))", storage_body)

        runtime_start = runtime.index("static int runtime_device_config_read_file")
        runtime_end = runtime.index("static int runtime_device_config_write_file")
        runtime_body = runtime[runtime_start:runtime_end]
        self.assertIn("struct fs_file_t file;", runtime_body)
        self.assertNotIn("struct fs_dirent entry;", runtime_body)
        self.assertNotIn("fs_stat(path, &entry)", runtime_body)
        self.assertIn("uint8_t overflow;", runtime_body)
        self.assertIn("fs_read(&file, &overflow, sizeof(overflow))", runtime_body)

    def test_app_install_paths_reuse_scratch_and_unlink_without_stat_probe(self):
        app_store = self.read("firmware/zephyr/src/app_store.c")

        install_start = app_store.index("int sq_app_store_install_app")
        install_end = app_store.index("int sq_app_store_begin_staged_install")
        install_body = app_store[install_start:install_end]
        self.assertIn("char path[SQ_APP_STORE_APP_FILE_PATH_MAX];", install_body)
        self.assertNotIn("char path[SQ_APP_STORE_PATH_MAX];", install_body)
        self.assertNotIn("char app_dir[SQ_APP_STORE_PATH_MAX];", install_body)
        self.assertNotIn("char sqbc_path[SQ_APP_STORE_PATH_MAX];", install_body)
        self.assertIn("format_app_dir(path, sizeof(path), mount_point, app_id)", install_body)
        self.assertIn("ensure_directory(path)", install_body)
        self.assertIn('format_app_path(path, sizeof(path), mount_point, app_id, "main.sqbc")', install_body)
        self.assertIn("write_file(path, sqbc, sqbc_len)", install_body)

        commit_start = app_store.index("int sq_app_store_commit_staged_install")
        commit_end = app_store.index("int sq_app_store_commit_staged_resource")
        commit_body = app_store[commit_start:commit_end]
        self.assertIn("char final_path[SQ_APP_STORE_APP_FILE_PATH_MAX];", commit_body)
        self.assertNotIn("char final_path[SQ_APP_STORE_PATH_MAX];", commit_body)
        self.assertNotIn("struct fs_dirent existing", commit_body)
        self.assertIn("result = fs_unlink(final_path);", commit_body)
        self.assertIn("if (result != 0 && result != -ENOENT)", commit_body)
        self.assertIn("return fs_rename(staging_path, final_path);", commit_body)

    def test_format_delete_walk_reuses_caller_path_buffer_for_recursion(self):
        app_store = self.read("firmware/zephyr/src/app_store.c")
        start = app_store.index("static int delete_files_under")
        end = app_store.index("int sq_app_store_format_filesystem")
        body = app_store[start:end]

        self.assertIn("static int delete_files_under(char *path, size_t path_cap,", body)
        self.assertNotIn("char child[SQ_APP_STORE_PATH_MAX];", body)
        self.assertIn("size_t path_len = strlen(path);", body)
        self.assertIn("path[path_len] = '/';", body)
        self.assertIn("path[path_len] = '\\0';", body)
        self.assertIn("result = delete_files_under(path, path_cap, deleted_any);", body)
        self.assertIn("result = fs_unlink(path);", body)

        format_start = app_store.index("int sq_app_store_format_filesystem")
        format_body = app_store[format_start:]
        self.assertIn("delete_files_under(path, sizeof(path), &deleted_any)", format_body)

    def test_protocol_poll_uses_runtime_scratch_instead_of_stack_arrays(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        runtime = self.read("firmware/zephyr/src/vm_runtime.c")
        start = protocol.index("int sq_device_protocol_poll")
        end = protocol.index("static int repeated_runtime_lines_response")
        body = protocol[start:end]

        self.assertNotIn("char target[SQ_APP_STORE_APP_ID_MAX];", body)
        self.assertNotIn("char armed_event[SQ_VM_RUNTIME_EVENT_LEN];", body)
        self.assertIn("pop_return_app(runtime, runtime->lifecycle_target_app,", body)
        self.assertIn(
            "sq_vm_runtime_next_due_armed_timer(runtime, runtime->lifecycle_target_app,",
            body,
        )
        self.assertIn("runtime->event, sizeof(runtime->event)", body)
        self.assertIn("runtime->event, true);", body)
        self.assertIn("memmove(runtime->event, event, event_len + 1);", runtime)

    def test_trigger_registration_uses_sqbc_only_storage(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        app_store_c = self.read("firmware/zephyr/src/app_store.c")
        start = protocol.index("static int __noinline register_app_triggers")
        end = protocol.index("int sq_device_protocol_poll")
        body = protocol[start:end]

        self.assertIn("sq_app_store_sqbc_path", app_store_h)
        self.assertIn("int sq_app_store_sqbc_path", app_store_c)
        self.assertNotIn("struct sq_app_store_vm_storage trigger_storage", body)
        self.assertIn("char sqbc_path[SQ_APP_STORE_APP_FILE_PATH_MAX];", body)
        self.assertIn("struct sq_vm_fs_storage trigger_storage", body)
        self.assertIn("sq_app_store_sqbc_path(context->store_mount_point, app_id,", body)
        self.assertIn(".sqbc_path = sqbc_path", body)
        self.assertIn("sq_vm_fs_storage_backend(&trigger_storage)", body)
        self.assertIn("static int __noinline register_app_triggers", protocol)
        self.assertIn("static int __noinline register_app_trigger_timer", protocol)
        self.assertNotIn("SqvmTriggerTimer timer = {0};", body)

    def test_device_config_package_load_formats_resource_path_from_bytes(self):
        runtime = self.read("firmware/zephyr/src/vm_runtime.c")
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        app_store_c = self.read("firmware/zephyr/src/app_store.c")
        start = runtime.index("static int sq_vm_runtime_device_config_load_resource")
        end = runtime.index("int sq_vm_runtime_device_config_load(")
        body = runtime[start:end]

        self.assertIn("sq_app_store_resource_path_bytes", app_store_h)
        self.assertIn("int sq_app_store_resource_path_bytes", app_store_c)
        self.assertNotIn("char resource[SQ_APP_STORE_PATH_MAX];", body)
        self.assertIn("char path[SQ_APP_STORE_PATH_MAX];", body)
        self.assertIn("sq_app_store_resource_path_bytes(runtime->store_mount_point", body)
        self.assertIn("resource_bytes, resource_len, path,", body)
        self.assertIn("sizeof(path)", body)
        self.assertNotIn("memcpy(resource, resource_bytes, resource_len);", body)

    def test_device_config_flash_path_uses_fixed_path_bound_without_temp_dir(self):
        runtime = self.read("firmware/zephyr/src/vm_runtime.c")
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        app_store_c = self.read("firmware/zephyr/src/app_store.c")
        path_start = app_store_c.index("int sq_app_store_device_config_path")
        path_end = app_store_c.index("int sq_app_store_install_resource")
        path_body = app_store_c[path_start:path_end]
        load_start = runtime.index("static int __noinline sq_vm_runtime_apply_saved_device_config")
        load_end = runtime.index("static int __noinline sq_vm_runtime_apply_device_bindings")
        load_body = runtime[load_start:load_end]
        save_start = runtime.index("int sq_vm_runtime_device_config_save")
        save_end = runtime.index("void sq_vm_runtime_reset_vm_context")
        save_body = runtime[save_start:save_end]

        self.assertIn("#define SQ_APP_STORE_DEVICE_CONFIG_PATH_MAX 40", app_store_h)
        self.assertNotIn("char system_dir[SQ_APP_STORE_PATH_MAX];", path_body)
        self.assertIn('"%s/system/device-config.sqdc"', path_body)
        self.assertIn("char path[SQ_APP_STORE_DEVICE_CONFIG_PATH_MAX];", load_body)
        self.assertIn("char path[SQ_APP_STORE_DEVICE_CONFIG_PATH_MAX];", save_body)
        self.assertNotIn("char path[SQ_APP_STORE_PATH_MAX];", load_body)
        self.assertNotIn("char path[SQ_APP_STORE_PATH_MAX];", save_body)

    def test_vm_dispatch_uses_static_callbacks_and_separate_user_data(self):
        runtime = self.read("firmware/zephyr/src/vm_runtime.c")
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        ffi_rs = self.read("compiler/rust/crates/squidvm-ffi/src/lib.rs")
        start = runtime.index("int sq_vm_runtime_dispatch")
        end = runtime.index("int sq_vm_runtime_start")
        body = runtime[start:end]

        self.assertIn("static const SqvmCallbacks runtime_callbacks", runtime)
        self.assertNotIn("SqvmCallbacks callbacks;", body)
        self.assertNotIn("callbacks = (SqvmCallbacks)", body)
        self.assertIn("sqvm_context_init_in_place(runtime->context_words, runtime,", body)
        self.assertIn("&runtime_callbacks", body)
        self.assertIn(
            "SqvmStatus sqvm_dispatch_start_resumable(\n\tvoid *context,\n\tvoid *user_data,\n\tconst SqvmCallbacks *callbacks,",
            ffi_h,
        )
        self.assertIn("callbacks: *const SqvmCallbacks", ffi_rs)
        self.assertIn("user_data: *mut c_void", ffi_rs)
        self.assertIn("FfiHost::new(user_data, callbacks, true)", ffi_rs)

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
        self.assertIn('output=wifi connect true null', station)
        self.assertIn('output=wifi station dev true', station)
        self.assertIn("unsupported", station)
        self.assertIn('assert_no_raw_network_identifiers', station)
        self.assertNotIn("obsolete", station.lower())
        self.assertNotIn("wifi ap", station)
        self.assertNotIn("SQUID_WIFI_STATION_PASSWORD}", station)
        self.assertNotIn("c3-supermini-test-wifi-station-api.sh", suite)
        self.assertNotIn("app.exit()", self.read("tests/hardware/c3-supermini/wifi-station-summary/main.squid"))

    def test_zephyr_wifi_profile_opcode_stores_bounded_volatile_runtime_profile(self):
        protocol_h = self.read("firmware/zephyr/src/device_protocol.h")
        protocol_c = self.read("firmware/zephyr/src/device_protocol.c")
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")

        self.assertIn("SQ_DEVICE_WIFI_PROFILE_NAME_BYTES 16", protocol_h)
        self.assertIn("SQ_DEVICE_WIFI_PROFILE_SSID_BYTES 32", protocol_h)
        self.assertIn("SQ_DEVICE_WIFI_PROFILE_PASSWORD_BYTES 64", protocol_h)
        self.assertIn("SqdpWifiProfile", ffi_h)
        self.assertIn("sqdp_parse_wifi_profile_set_request", ffi_h)
        self.assertIn("sqdp_parse_wifi_profile_set_request", protocol_c)
        self.assertIn("sq_vm_runtime_set_wifi_profile", runtime_h)
        self.assertIn("wifi_profile_set(&frame, request, request_len", protocol_c)
        wifi_profile_body = protocol_c[
            protocol_c.index("static int __noinline wifi_profile_set") : protocol_c.index(
                "int sq_device_protocol_handle_frame"
            )
        ]
        self.assertNotIn("sq_protocol_next_field", wifi_profile_body)
        self.assertNotIn("struct sq_protocol_field", wifi_profile_body)
        self.assertNotIn("case SQ_OPCODE_WIFI_PROFILE_SET:\n\t\tresult = -ENOTSUP;", protocol_c)

    def test_zephyr_vm_runtime_wires_system_resource_callbacks(self):
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        runtime_c = self.read("firmware/zephyr/src/vm_runtime.c")
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")

        self.assertIn("SqvmSystemMemoryTextCallback system_memory_text", ffi_h)
        self.assertIn("SqvmSystemStorageTextCallback system_storage_text", ffi_h)
        self.assertIn("runtime_system_memory_text", runtime_c)
        self.assertIn("runtime_system_storage_text", runtime_c)
        self.assertIn(".system_memory_text = runtime_system_memory_text", runtime_c)
        self.assertIn(".system_storage_text = runtime_system_storage_text", runtime_c)
        self.assertIn("sq_vm_runtime_set_store_mount_point", runtime_h)

    def test_zephyr_header_exposes_bounded_rust_device_config_core(self):
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        ffi_rs = self.read("compiler/rust/crates/squidvm-ffi/src/lib.rs")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")

        self.assertIn("#define SQDC_CONFIG_MAX_RECORDS 5", ffi_h)
        self.assertNotIn("#define SQDC_CONFIG_MAX_RECORDS 6", ffi_h)
        self.assertNotIn("#define SQDC_CONFIG_MAX_RECORDS 8", ffi_h)
        self.assertIn("pub const SQDC_CONFIG_MAX_RECORDS: usize = 5;", ffi_rs)
        self.assertNotIn("pub const SQDC_CONFIG_MAX_RECORDS: usize = 6;", ffi_rs)
        self.assertIn("#define SQDC_CONFIG_KEY_CAP 32", ffi_h)
        self.assertIn("#define SQDC_CONFIG_STRING_CAP 48", ffi_h)
        self.assertNotIn("#define SQDC_CONFIG_STRING_CAP 64", ffi_h)
        self.assertIn("typedef struct {\n\tSqdcRecord records[SQDC_CONFIG_MAX_RECORDS];", ffi_h)
        self.assertIn("sqdc_parse_sqdevice", ffi_h)
        self.assertIn("sqdc_config_set_string", ffi_h)
        self.assertIn("sqdc_encode_sqdc", ffi_h)
        self.assertIn("sqdc_decode_sqdc", ffi_h)
        self.assertIn("test_sqdc_ffi_parses_and_encodes_device_config", ztest)

    def test_zephyr_runtime_preserves_foreground_vm_context_between_non_lifecycle_events(self):
        runtime_c = self.read("firmware/zephyr/src/vm_runtime.c")
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        protocol_c = self.read("firmware/zephyr/src/device_protocol.c")

        dispatch_body = runtime_c[
            runtime_c.index("int sq_vm_runtime_dispatch") : runtime_c.index(
                "int sq_vm_runtime_start"
            )
        ]
        start_definition = protocol_c.index(
            "static int start_installed_app(const struct sq_device_protocol_context *context,",
            protocol_c.index("static int __noinline launch_app"),
        )
        start_body = protocol_c[
            start_definition : protocol_c.index("static void clear_foreground_timers", start_definition)
        ]

        self.assertIn("bool context_ready", runtime_h)
        self.assertIn("void sq_vm_runtime_reset_vm_context", runtime_h)
        self.assertIn("if (!runtime->context_ready)", dispatch_body)
        self.assertIn("sqvm_context_init_in_place", dispatch_body)
        self.assertNotIn("clear_dispatch_state(runtime);", dispatch_body)
        self.assertIn("sq_vm_runtime_reset_vm_context(context->runtime)", start_body)
        self.assertIn("set_current || strcmp(context->runtime->current_app, app_id) != 0", start_body)

    def test_zephyr_wifi_station_uses_real_connect_disconnect_backend(self):
        runtime_c = self.read("firmware/zephyr/src/vm_runtime.c")

        connect_start = runtime_c.index("static int32_t runtime_wifi_connect")
        disconnect_start = runtime_c.index("static int32_t runtime_wifi_disconnect")
        ap_ip_start = runtime_c.index("static int32_t runtime_wifi_get_ap_ip")
        connect_body = runtime_c[connect_start:disconnect_start]
        disconnect_body = runtime_c[disconnect_start:ap_ip_start]

        self.assertIn("NET_REQUEST_WIFI_CONNECT", connect_body)
        self.assertIn("NET_EVENT_WIFI_CONNECT_RESULT", runtime_c)
        self.assertIn("NET_REQUEST_WIFI_DISCONNECT", disconnect_body)
        self.assertIn("NET_EVENT_WIFI_DISCONNECT_RESULT", runtime_c)
        self.assertNotIn("runtime_wifi_unsupported_action(out)", connect_body.split("#else", 1)[0])
        self.assertNotIn("runtime_wifi_unsupported_action(out)", disconnect_body.split("#else", 1)[0])

    def test_zephyr_wifi_status_reports_station_dhcp_ip_without_fixture_leak(self):
        runtime_c = self.read("firmware/zephyr/src/vm_runtime.c")
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        station_fixture = self.read("tests/hardware/c3-supermini/wifi-station-summary/main.squid")

        self.assertIn("#include <zephyr/net/dhcpv4.h>", runtime_c)
        self.assertIn("net_dhcpv4_start(iface)", runtime_c)
        self.assertIn("net_if_ipv4_get_global_addr(iface, NET_ADDR_PREFERRED)", runtime_c)
        self.assertIn("net_addr_ntop(NET_AF_INET", runtime_c)
        self.assertIn("wifi_station_ip", runtime_h)
        self.assertNotIn("status.ipAddress", station_fixture)

    def test_wifi_ap_check_is_current_redacted_and_in_default_suite(self):
        ap = self.read("scripts/c3-supermini-test-wifi-ap-api.sh")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")
        runtime_c = self.read("firmware/zephyr/src/vm_runtime.c")

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


if __name__ == "__main__":
    unittest.main()
