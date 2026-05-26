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

    def test_default_config_enables_live_heap_resource_telemetry(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int resources_response")
        end = protocol.index("static void clear_runtime_context")
        body = protocol[start:end]

        self.assertIn("CONFIG_SYS_HEAP_RUNTIME_STATS=y", prj_conf)
        self.assertIn("sys_heap_runtime_stats_get", body)
        self.assertIn("ram_heap_count", body)
        self.assertIn("ram_heap_free_bytes", body)
        self.assertIn("ram_heap_allocated_bytes", body)
        self.assertIn("ram_heap_max_allocated_bytes", body)
        self.assertIn("protocol_thread_stack_pre_resources_unused_bytes", body)
        self.assertIn("protocol_thread_stack_pre_resources_used_bytes", body)
        self.assertIn("context->resource_metrics", body)
        self.assertIn("context->resource_metric_cap", body)
        self.assertNotIn("SqdpResourceMetric metrics[]", body)
        self.assertNotIn("install_session_bytes", body)
        self.assertNotIn("temp_session_bytes", body)
        self.assertNotIn("resource_session_bytes", body)

    def test_resources_include_last_dispatch_lazy_load_metrics(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        runtime_c = self.read("firmware/zephyr/src/vm_runtime.c")
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")

        for metric in [
            "last_dispatch_sequence",
            "last_dispatch_elapsed_us",
            "last_dispatch_sqbc_read_count",
            "last_dispatch_sqbc_read_bytes",
        ]:
            self.assertIn(metric, protocol)
            self.assertIn(metric, runtime_h)
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
        self.assertIn("last_dispatch_sequence", script)
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

        self.assertIn("CONFIG_MAIN_STACK_SIZE=8192", prj_conf)

    def test_stack_usage_harness_tracks_current_vm_worker_budget(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        stack_script = self.read("scripts/c3-supermini-measure-stack-usage.sh")

        self.assertIn("#define SQ_VM_RUNTIME_WORK_STACK_SIZE 19456", runtime_h)
        self.assertIn('Expected vm_worker_stack_size_bytes=19456', stack_script)
        self.assertNotIn('Expected vm_worker_stack_size_bytes=20480', stack_script)
        self.assertNotIn('Expected vm_worker_stack_size_bytes=16384', stack_script)
        self.assertIn("protocol_thread_stack_size_bytes", stack_script)
        self.assertIn('Expected protocol_thread_stack_size_bytes=8192', stack_script)
        self.assertIn("protocol_thread_stack_pre_resources_used_bytes", stack_script)
        self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', stack_script)
        self.assertIn('timeout "${COMMAND_TIMEOUT_SECONDS:-20}s"', stack_script)

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

        self.assertIn("#define SQ_VM_RUNTIME_CONTEXT_BYTES 12288", runtime_h)
        self.assertIn("SQ_VM_RUNTIME_CONTEXT_BYTES <= 12288", ztest)

    def test_runtime_reuses_transfer_storage_for_init_scratch_and_completion(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")
        runtime_body = runtime_h[
            runtime_h.index("struct sq_vm_runtime {") : runtime_h.index(
                "void sq_vm_runtime_init", runtime_h.index("struct sq_vm_runtime {")
            )
        ]

        self.assertIn("union sq_vm_runtime_transfer", runtime_h)
        self.assertIn("uint8_t init_scratch[SQ_VM_RUNTIME_SCRATCH_BYTES]", runtime_h)
        self.assertIn("SqvmStorageCompletion completion", runtime_h)
        self.assertNotIn("uint8_t scratch[SQ_VM_RUNTIME_SCRATCH_BYTES];", runtime_body)
        self.assertNotIn("SqvmStorageCompletion completion;", runtime_body)
        self.assertIn("sizeof(runtime.transfer.init_scratch)", ztest)
        self.assertIn("runtime_static <= 16640", ztest)

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
        apply_body = runtime_c[
            runtime_c.index("static int sq_vm_runtime_apply_device_bindings")
            : runtime_c.index(
                "static int32_t runtime_device_config_load",
                runtime_c.index("static int sq_vm_runtime_apply_device_bindings"),
            )
        ]
        self.assertIn("struct sq_vm_runtime_binding_scratch", runtime_c)
        self.assertIn("sizeof(*scratch) <= sizeof(runtime->transfer.init_scratch)", apply_body)
        self.assertIn(
            "struct sq_vm_runtime_binding_scratch *scratch =",
            apply_body,
        )
        self.assertNotIn("SqvmDeviceBinding binding_storage", apply_body)
        self.assertNotIn("SqdcDeviceBindingPlan plan_storage", apply_body)

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
        self.assertIn("sq_vm_runtime_prepare_app_start(runtime)", work_body)
        self.assertIn("runtime->start_setup_done", start_body)
        self.assertNotIn("sq_vm_runtime_apply_device_bindings(runtime)", start_body)
        self.assertNotIn("sq_vm_runtime_apply_saved_device_config(runtime)", start_body)
        self.assertNotIn("sq_vm_runtime_apply_target_default_indicator_binding(runtime)", start_body)

    def test_runtime_keeps_bounded_diagnostic_history(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")

        self.assertIn("#define SQ_VM_RUNTIME_TRACE_MAX 8", runtime_h)
        self.assertIn("#define SQ_VM_RUNTIME_OUTPUT_MAX 12", runtime_h)
        self.assertIn("#define SQ_VM_RUNTIME_DRAWLOG_MAX 4", runtime_h)

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

    def test_state_import_uses_rust_parser_without_c_tlv_loop(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        start = protocol.index("static int state_import")
        end = protocol.index("static int resources_response")
        body = protocol[start:end]

        self.assertIn("SqdpStateImport", ffi_h)
        self.assertIn("sqdp_parse_state_import_request", ffi_h)
        self.assertIn("sqdp_parse_state_import_request", body)
        self.assertNotIn("sq_protocol_next_field", body)
        self.assertNotIn("struct sq_protocol_field", body)

    def test_state_get_response_uses_rust_encoder_without_c_payload_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        start = protocol.index("static int state_get_response")
        end = protocol.index("static int state_import")
        body = protocol[start:end]

        self.assertIn("sqdp_encode_state_response", ffi_h)
        self.assertIn("sqdp_encode_state_response", body)
        self.assertIn("runtime->transfer.completion.bytes", body)
        self.assertNotIn("payload[0]", body)
        self.assertNotIn("SQ_DEVICE_STATE_FIELD_BYTES", body)
        self.assertNotIn("sq_protocol_encode_frame_header", body)

    def test_resources_response_uses_rust_encoder_without_c_record_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int resources_response")
        end = protocol.index("static void clear_runtime_context")
        body = protocol[start:end]

        self.assertIn("sqdp_encode_resources_response", body)
        self.assertNotIn("uint8_t record[96]", body)
        self.assertNotIn("append_record_field(payload", body)

    def test_resources_response_reports_input_binding_state(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        header = self.read("firmware/zephyr/src/device_protocol.h")
        stack = self.read("scripts/c3-supermini-measure-input-stack-isolation.sh")

        self.assertIn("#define SQ_DEVICE_RESOURCE_METRIC_MAX 21", header)
        self.assertNotIn('SQ_RESOURCE_METRIC("active_binding_count"', protocol)
        self.assertIn('SQ_RESOURCE_METRIC("input_button_count"', protocol)
        self.assertNotIn('SQ_RESOURCE_METRIC("input_button_pressed_count"', protocol)
        self.assertIn("input_button_count", stack)
        self.assertNotIn("input_button_pressed_count", stack)

    def test_key_dispatch_uses_rust_parser_without_c_payload_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int dispatch_key")
        end = protocol.index("int sq_device_protocol_handle_frame")
        body = protocol[start:end]

        self.assertIn("sqdp_prepare_key_event", body)
        self.assertNotIn("uint8_t event_payload[64]", body)
        self.assertNotIn("append_string_field(event_payload", body)

    def test_app_launch_uses_rust_parser_without_c_tlv_loop(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        ffi_h = self.read("firmware/zephyr/src/squidvm_ffi.h")
        start = protocol.index("static int launch_app")
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
        start = protocol.index("static int dispatch_event_request")
        end = protocol.index("static int dispatch_key")
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
        self.assertIn('vm_worker_stack_size_bytes', stack)
        self.assertIn('vm_worker_stack_used_bytes', stack)
        self.assertIn('vm_worker_stack_unused_bytes', stack)
        self.assertIn('stack_used + stack_unused != stack_size', stack)

        lifecycle_check = suite.index('c3-supermini-test-app-lifecycle.sh')
        stack_check = suite.index('c3-supermini-measure-stack-usage.sh')
        system_check = suite.index('c3-supermini-test-system-resources.sh')
        self.assertLess(lifecycle_check, system_check)
        self.assertLess(system_check, stack_check)

    def test_hardware_workload_ram_measurement_captures_attributed_snapshots(self):
        stack = self.read("scripts/c3-supermini-measure-ram-workloads.sh")

        self.assertIn('COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"', stack)
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
        self.assertIn('protocol_thread_stack_used_bytes', stack)
        self.assertIn('protocol_thread_stack_pre_resources_used_bytes', stack)
        self.assertIn('vm_worker_stack_used_bytes', stack)
        self.assertIn('ram_heap_allocated_bytes', stack)
        self.assertIn('ram_heap_max_allocated_bytes', stack)
        self.assertIn('summary.tsv', stack)
        self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', stack)

    def test_input_stack_isolation_measurement_is_bounded_and_input_only(self):
        stack = self.read("scripts/c3-supermini-measure-input-stack-isolation.sh")

        self.assertIn('COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"', stack)
        self.assertIn('WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"', stack)
        self.assertIn("target/hardware-tests/input-stack-isolation", stack)
        self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', stack)
        self.assertIn('source "${ROOT}/scripts/lib/serial-port.sh"', stack)
        self.assertIn('c3-supermini-zephyr-test-diagnostic.sh', stack)
        self.assertIn('snapshot_resources after-boot', stack)
        self.assertIn('snapshot_resources after-format', stack)
        self.assertIn('snapshot_resources after-install', stack)
        self.assertIn('snapshot_resources after-launch', stack)
        self.assertIn('snapshot_resources after-press', stack)
        self.assertIn('tests/hardware/c3-supermini/input-button-summary/main.squid', stack)
        self.assertIn('Press and release the ESP32-C3 Super Mini BOOT/GPIO9 button now.', stack)
        self.assertNotIn('cargo run --quiet -p squidc -- device key SELECT', stack)
        self.assertIn('protocol_thread_stack_pre_resources_used_bytes', stack)
        self.assertIn('vm_worker_stack_used_bytes', stack)
        self.assertIn('ram_heap_allocated_bytes', stack)
        self.assertIn('summary.tsv', stack)
        self.assertNotIn("wifi", stack.lower())

    def test_hardware_scripts_use_shared_bounded_command_helper(self):
        scripts_dir = ROOT / "scripts"
        helper = self.read("scripts/lib/hardware-command.sh")
        self.assertIn('timeout "${timeout_seconds}s"', helper)
        self.assertIn('COMMAND_TIMEOUT_SECONDS:-20', helper)

        for script_path in sorted(scripts_dir.glob("c3-supermini-*.sh")):
            contents = script_path.read_text(encoding="utf-8")
            if "cargo run --quiet -p squidc -- device" not in contents and (
                "cargo run --quiet -p squidc -- app" not in contents
            ):
                continue

            with self.subTest(script=script_path.name):
                self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', contents)
                self.assertNotIn("\nrun_capture() {\n", contents)

    def test_hardware_output_helper_bounds_device_output_command(self):
        helper = self.read("scripts/lib/hardware-output.sh")

        self.assertIn('timeout "${timeout_seconds}s"', helper)
        self.assertIn('COMMAND_TIMEOUT_SECONDS:-20', helper)
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
            protocol_c.index("static int wifi_profile_set") : protocol_c.index(
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
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")

        self.assertIn("#define SQDC_CONFIG_MAX_RECORDS 8", ffi_h)
        self.assertIn("#define SQDC_CONFIG_KEY_CAP 32", ffi_h)
        self.assertIn("#define SQDC_CONFIG_STRING_CAP 64", ffi_h)
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
            protocol_c.index("static int launch_app"),
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
