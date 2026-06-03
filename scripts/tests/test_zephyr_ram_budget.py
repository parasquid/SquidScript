from pathlib import Path
import json
import os
import re
import subprocess
import sys
import tempfile

from scripts.tests.zephyr_test_utils import ROOT, ZephyrScriptTestCase


class ZephyrRamBudgetTests(ZephyrScriptTestCase):
    def test_default_config_uses_bounded_ble_peripheral_pools(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        for option in [
            "CONFIG_BT_BUF_ACL_TX_COUNT=3",
            "CONFIG_BT_L2CAP_TX_BUF_COUNT=3",
            "CONFIG_BT_ATT_TX_COUNT=3",
            "CONFIG_BT_CONN_TX_MAX=3",
            "CONFIG_BT_BUF_EVT_RX_COUNT=4",
            "CONFIG_BT_BUF_EVT_DISCARDABLE_COUNT=1",
            "CONFIG_ESP32_BT_CTLR_LE_MAX_CONN=1",
            "CONFIG_ESP32_BT_CTLR_LE_MAX_ACT=2",
        ]:
            self.assertIn(option, prj_conf)

    def test_default_config_uses_measured_low_throughput_network_pools(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        for option in [
            "CONFIG_NET_PKT_RX_COUNT=6",
            "CONFIG_NET_PKT_TX_COUNT=6",
            "CONFIG_NET_BUF_RX_COUNT=16",
            "CONFIG_NET_BUF_TX_COUNT=16",
            "CONFIG_NET_MGMT_EVENT_QUEUE_SIZE=8",
            "CONFIG_NET_MGMT_EVENT_QUEUE_TIMEOUT=5000",
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

        self.assertIn("CONFIG_LOG_PROCESS_THREAD_STACK_SIZE=768", prj_conf)
        self.assertNotIn("CONFIG_LOG_PROCESS_THREAD_STACK_SIZE=512", prj_conf)

    def test_default_config_uses_bounded_littlefs_file_pool(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        self.assertIn("CONFIG_FS_LITTLEFS_NUM_FILES=2", prj_conf)
        self.assertNotIn("CONFIG_FS_LITTLEFS_NUM_FILES=4", prj_conf)
        self.assertNotIn("CONFIG_FS_LITTLEFS_NUM_DIRS=2", prj_conf)

    def test_default_config_uses_bounded_filesystem_name_buffer(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")
        protocol_h = self.read("firmware/zephyr/src/device_protocol.h")
        build_doc = self.read("docs/firmware_build_architecture.md")

        self.assertIn("CONFIG_FILE_SYSTEM_MAX_FILE_NAME=80", prj_conf)
        self.assertIn("#define SQ_DEVICE_RESOURCE_PATH_BYTES 80u", protocol_h)
        self.assertIn("Zephyr filesystem filename buffer is capped at 80 bytes", build_doc)
        self.assertNotIn("CONFIG_FILE_SYSTEM_MAX_FILE_NAME=128", prj_conf)

    def test_default_config_enables_live_heap_resource_telemetry(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int __noinline resources_response")
        end = protocol.index("static int clear_runtime_context")
        body = protocol[start:end]

        self.assertIn("CONFIG_SYS_HEAP_RUNTIME_STATS=y", prj_conf)
        self.assertIn("sys_heap_runtime_stats_get", body)
        self.assertIn("heap_count", body)
        self.assertIn("heap_free_bytes", body)
        self.assertIn("heap_alloc_bytes", body)
        self.assertIn("heap_max_alloc_bytes", body)
        self.assertIn("sys_heap_runtime_stats_reset_max", body)
        self.assertIn("heap_largest_free_supported", body)
        self.assertIn("heap_largest_free_bytes", body)
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

    def test_resources_include_runtime_lockup_triage_metrics(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int __noinline resources_response")
        end = protocol.index("static int clear_runtime_context")
        body = protocol[start:end]

        for metric in [
            "runtime_status",
            "runtime_dispatch_started",
            "runtime_dispatch_age_us",
            "runtime_work_submitted",
            "runtime_current_app_present",
            "runtime_lifecycle_phase",
            "runtime_arm_phase",
        ]:
            self.assertIn(metric, body)

        self.assertIn("k_cycle_get_64", body)
        self.assertIn("k_cyc_to_us_floor64", body)

    def test_default_config_uses_measured_system_heap_budget(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")
        ram_workloads = self.read("scripts/c3-supermini-measure-ram-workloads.sh")

        self.assertIn("CONFIG_HEAP_MEM_POOL_SIZE=65536", prj_conf)
        self.assertNotIn("CONFIG_HEAP_MEM_POOL_SIZE=51200", prj_conf)
        self.assertIn("CONFIG_HEAP_MEM_POOL_IGNORE_MIN=y", prj_conf)
        self.assertNotIn("CONFIG_HEAP_MEM_POOL_ADD_SIZE_ESP_WIFI=", prj_conf)
        self.assertIn('SYSTEM_HEAP_BYTES="${SYSTEM_HEAP_BYTES:-65536}"', ram_workloads)
        self.assertNotIn('SYSTEM_HEAP_BYTES="${SYSTEM_HEAP_BYTES:-51200}"', ram_workloads)
        self.assertIn("heap_max_headroom_bytes", ram_workloads)
        self.assertIn("SYSTEM_HEAP_BYTES - heap_max_alloc", ram_workloads)

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

    def test_static_buffer_report_classifies_known_platform_symbols(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            elf = tmp_path / "zephyr.elf"
            elf.write_bytes(b"fake")
            nm_tool = tmp_path / "fake-nm"
            self.write_executable(
                nm_tool,
                "#!/usr/bin/env bash\n"
                "cat <<'EOF'\n"
                "3fc90000 00005600 B sq_vm_runtime_work_stack\n"
                "3fc95600 00002fe8 b runtime.4\n"
                "3fc99000 00002000 B z_main_stack\n"
                "3fc9b000 00000400 b sys_work_q_stack\n"
                "3fc9b400 000003c0 D TxRxCxt\n"
                "3fc9b7c0 000002a0 b _net_buf_rx_bufs\n"
                "3fc9ba60 000002a0 b _net_buf_tx_bufs\n"
                "3fc9bd00 00000350 D phy_param\n"
                "3fc9c050 00000330 B gWpaSm\n"
                "3fc9c380 00000284 b global_data\n"
                "3fc9c604 00000220 b route_ipv4_entries\n"
                "3fc9c824 00000250 B gChmCxt\n"
                "3fc9ca74 0000011c B gScanStruct\n"
                "3fc9cb90 00000394 b response.0\n"
                "3fc9cf24 00001000 b bt_stack\n"
                "3fc9df24 00000400 b bt_tx_processor_stack\n"
                "3fc9e324 00000520 B bt_lw_stack_area\n"
                "EOF\n",
            )

            env = os.environ.copy()
            env["NM"] = str(nm_tool)

            result = subprocess.run(
                [str(ROOT / "scripts/zephyr-static-buffer-report.sh"), str(elf)],
                cwd=ROOT,
                env=env,
                text=True,
                capture_output=True,
                check=True,
            )

        self.assertNotIn("group=unknown", result.stdout)
        for name in [
            "sys_work_q_stack",
            "TxRxCxt",
            "_net_buf_rx_bufs",
            "_net_buf_tx_bufs",
            "phy_param",
            "gWpaSm",
            "global_data",
            "route_ipv4_entries",
            "gChmCxt",
            "gScanStruct",
            "bt_stack",
            "bt_tx_processor_stack",
            "bt_lw_stack_area",
        ]:
            with self.subTest(name=name):
                self.assertRegex(result.stdout, rf"group=platform .*name={name}")
        self.assertRegex(result.stdout, r"group=squidscript .*name=sq_vm_runtime_work_stack")
        self.assertRegex(result.stdout, r"group=squidscript .*name=runtime\.4")
        self.assertRegex(result.stdout, r"group=squidscript .*name=response\.0")

    def test_zephyr_main_stack_tracks_measured_protocol_work(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")

        self.assertIn("CONFIG_MAIN_STACK_SIZE=4864", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=4608", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=5120", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=6144", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=8192", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=3264", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=3328", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=3584", prj_conf)
        self.assertNotIn("CONFIG_MAIN_STACK_SIZE=4096", prj_conf)

    def test_stack_usage_harness_tracks_current_vm_worker_budget(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        stack_script = self.read("scripts/c3-supermini-measure-stack-usage.sh")

        self.assertIn("#define SQ_VM_RUNTIME_WORK_STACK_SIZE 16640", runtime_h)
        self.assertIn('Expected vm_stack_size_bytes=16640', stack_script)
        self.assertNotIn("#define SQ_VM_RUNTIME_WORK_STACK_SIZE 17408", runtime_h)
        self.assertNotIn('Expected vm_stack_size_bytes=17408', stack_script)
        self.assertNotIn("#define SQ_VM_RUNTIME_WORK_STACK_SIZE 18432", runtime_h)
        self.assertNotIn('Expected vm_stack_size_bytes=18432', stack_script)
        self.assertNotIn("#define SQ_VM_RUNTIME_WORK_STACK_SIZE 20480", runtime_h)
        self.assertNotIn('Expected vm_stack_size_bytes=20480', stack_script)
        self.assertNotIn("#define SQ_VM_RUNTIME_WORK_STACK_SIZE 22016", runtime_h)
        self.assertNotIn('Expected vm_stack_size_bytes=22016', stack_script)
        self.assertNotIn("#define SQ_VM_RUNTIME_WORK_STACK_SIZE 18048", runtime_h)
        self.assertNotIn('Expected vm_worker_stack_size_bytes=18048', stack_script)
        self.assertNotIn("#define SQ_VM_RUNTIME_WORK_STACK_SIZE 19456", runtime_h)
        self.assertNotIn('Expected vm_worker_stack_size_bytes=18432', stack_script)
        self.assertNotIn('Expected vm_worker_stack_size_bytes=19456', stack_script)
        self.assertNotIn('Expected vm_worker_stack_size_bytes=20480', stack_script)
        self.assertNotIn('Expected vm_stack_size_bytes=18016', stack_script)
        self.assertNotIn('Expected vm_worker_stack_size_bytes=16384', stack_script)
        self.assertIn("proto_stack_size_bytes", stack_script)
        self.assertIn('Expected proto_stack_size_bytes=4864', stack_script)
        self.assertNotIn('Expected proto_stack_size_bytes=4608', stack_script)
        self.assertNotIn('Expected proto_stack_size_bytes=5120', stack_script)
        self.assertNotIn('Expected proto_stack_size_bytes=6144', stack_script)
        self.assertNotIn('Expected proto_stack_size_bytes=8192', stack_script)
        self.assertNotIn('Expected proto_stack_size_bytes=3264', stack_script)
        self.assertNotIn('Expected protocol_thread_stack_size_bytes=3328', stack_script)
        self.assertNotIn('Expected protocol_thread_stack_size_bytes=3584', stack_script)
        self.assertNotIn('Expected protocol_thread_stack_size_bytes=4096', stack_script)
        self.assertNotIn('Expected protocol_thread_stack_size_bytes=5120', stack_script)
        self.assertNotIn('Expected protocol_thread_stack_size_bytes=6144', stack_script)
        self.assertIn("proto_stack_pre_used_bytes", stack_script)
        self.assertNotIn("proto_stack_pre_res_used_bytes", stack_script)
        self.assertIn('PROTOCOL_STACK_MIN_UNUSED_BYTES="${PROTOCOL_STACK_MIN_UNUSED_BYTES:-768}"', stack_script)
        self.assertIn('WORKER_STACK_MIN_UNUSED_BYTES="${WORKER_STACK_MIN_UNUSED_BYTES:-384}"', stack_script)
        self.assertIn("Protocol stack headroom below", stack_script)
        self.assertIn("VM worker stack headroom below", stack_script)

    def test_ram_reference_docs_track_current_esp32c3_baseline(self):
        stack_script = self.read("scripts/c3-supermini-measure-stack-usage.sh")
        docs = {
            "ROADMAP.md": self.read("ROADMAP.md"),
            "docs/firmware_build_architecture.md": self.read(
                "docs/firmware_build_architecture.md"
            ),
            "docs/hardware_target_tests.md": self.read(
                "docs/hardware_target_tests.md"
            ),
            "docs/firmware_app_storage.md": self.read("docs/firmware_app_storage.md"),
        }

        for path, contents in docs.items():
            with self.subTest(path=path):
                self.assertNotIn("22,016 bytes", contents)
                self.assertNotIn("22,016-byte", contents)
                self.assertNotIn("22016 bytes", contents)
                self.assertNotIn("8,192 bytes", contents)
                self.assertNotIn("8,192-byte", contents)
                self.assertNotIn("8192 bytes", contents)
                self.assertNotIn("10,880 bytes", contents)
                self.assertNotIn("10,880-byte", contents)
                self.assertNotIn("10,624 bytes", contents)
                self.assertNotIn("10,624-byte", contents)
                self.assertNotIn("215,188 bytes", contents)
                self.assertNotIn("14,888-byte", contents)

        build_doc = docs["docs/firmware_build_architecture.md"]
        self.assertIn("7,872 bytes", build_doc)
        self.assertNotIn("10,304 bytes", build_doc)
        self.assertNotIn("10,496 bytes", build_doc)
        self.assertIn("protocol/main thread stack is currently 4,864 bytes", build_doc)
        self.assertIn("VM worker stack\nis 16,640 bytes", build_doc)
        self.assertIn("239,232 bytes of linker DRAM", build_doc)
        self.assertIn("239,216 bytes through", build_doc)
        self.assertIn("11,920-byte `runtime.4`", build_doc)
        self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', stack_script)
        self.assertIn(
            'resources_out="$(run_capture resources-after-workloads cargo run --quiet -p squidc -- device resources)"',
            stack_script,
        )
        self.assertNotIn(
            'timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" \\\n  cargo run --quiet -p squidc -- device resources',
            stack_script,
        )

    def test_hardware_workload_ram_measurement_captures_attributed_snapshots(self):
        stack = self.read("scripts/c3-supermini-measure-ram-workloads.sh")

        self.assertIn('COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"', stack)
        self.assertIn('WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"', stack)
        self.assertIn("target/hardware-tests/ram-workloads", stack)
        self.assertIn('snapshot_resources after-format', stack)
        self.assertIn('snapshot_resources input-after-install', stack)
        self.assertIn('snapshot_resources input-after-launch', stack)
        self.assertIn('snapshot_resources input-after-select', stack)
        self.assertNotIn('wait_for_contains input-output-start "output=count 0"', stack)
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
        self.assertIn("reset_runtime_between_workloads display", stack)
        self.assertIn("reset_runtime_between_workloads system", stack)
        self.assertIn("reset_runtime_between_workloads wifi-ap", stack)
        self.assertLess(
            stack.index('run_capture key-select cargo run --quiet -p squidc -- device key SELECT'),
            stack.index("reset_runtime_between_workloads display"),
        )
        self.assertLess(
            stack.index("reset_runtime_between_workloads display"),
            stack.index('run_capture launch-display-drawlog cargo run --quiet -p squidc -- app launch display-drawlog'),
        )
        self.assertLess(
            stack.index("reset_runtime_between_workloads system"),
            stack.index('run_capture launch-system-resources cargo run --quiet -p squidc -- app launch system-resources'),
        )
        self.assertLess(
            stack.index("reset_runtime_between_workloads wifi-ap"),
            stack.index('run_capture launch-wifi-ap cargo run --quiet -p squidc -- app launch wifi-ap-summary'),
        )
        self.assertIn('heap_alloc_bytes', stack)
        self.assertIn('heap_max_alloc_bytes', stack)
        self.assertIn('device resources --reset-heap-max', stack)
        self.assertIn('heap_largest_free_supported', stack)
        self.assertIn('heap_largest_free_bytes', stack)
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
        self.assertIn('heap_largest_free_supported', stack)
        self.assertIn('heap_largest_free_bytes', stack)
        self.assertIn('summary.tsv', stack)
        self.assertNotIn("wifi", stack.lower())

    def test_c3_stack_usage_attribution_is_opt_in_and_reportable(self):
        cli = self.read("compiler/rust/crates/squidc-cli/src/target.rs")
        cmake = self.read("firmware/zephyr/CMakeLists.txt")
        docs = self.read("docs/firmware_build_architecture.md")
        report = ROOT / "scripts/c3-supermini-stack-usage-report.sh"

        self.assertTrue(report.exists())
        self.assertIn("SQUID_ZEPHYR_STACK_USAGE", cli)
        self.assertIn("-DSQUID_ZEPHYR_STACK_USAGE=ON", cli)
        self.assertIn("SQUID_ZEPHYR_STACK_USAGE", cmake)
        self.assertIn("-fstack-usage", cmake)
        self.assertIn("scripts/c3-supermini-stack-usage-report.sh", docs)
        self.assertIn(
            "cargo run -p squidc -- target build --target esp32c3-super-mini --stack-usage",
            docs,
        )

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
        self.assertIn("note: .su rows are per-function static estimates, not cumulative call-chain peaks.", result.stderr)

    def test_c3_stack_usage_report_summarizes_top_rows_by_source_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            (tmp_path / "stack.su").write_text(
                "\n".join(
                    [
                        "src/device_protocol.c:20:1:large\t160\tstatic",
                        "src/device_protocol.c:30:1:medium\t96\tstatic",
                        "src/app_store.c:40:1:storage\t112\tstatic",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            result = subprocess.run(
                [
                    str(ROOT / "scripts/c3-supermini-stack-usage-report.sh"),
                    str(tmp_path),
                    "3",
                ],
                cwd=ROOT,
                text=True,
                capture_output=True,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("top_rows\tmax_bytes\tsum_bytes\tsource_file", result.stdout)
        self.assertIn("2\t160\t256\tsrc/device_protocol.c", result.stdout)
        self.assertIn("1\t112\t112\tsrc/app_store.c", result.stdout)

    def test_c3_stack_usage_report_shows_cumulative_source_known_call_paths(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            source = tmp_path / "device_protocol.c"
            source.write_text(
                "\n".join(
                    [
                        "static int caller(void)",
                        "{",
                        "\treturn helper();",
                        "}",
                        "",
                        "static int helper(void)",
                        "{",
                        "\treturn leaf();",
                        "}",
                        "",
                        "static int leaf(void)",
                        "{",
                        "\treturn 0;",
                        "}",
                        "",
                        "static int independent(void)",
                        "{",
                        "\treturn 0;",
                        "}",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            (tmp_path / "stack.su").write_text(
                "\n".join(
                    [
                        f"{source}:1:12:caller\t80\tstatic",
                        f"{source}:6:12:helper\t120\tstatic",
                        f"{source}:11:12:leaf\t64\tstatic",
                        f"{source}:16:12:independent\t160\tstatic",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            result = subprocess.run(
                [
                    str(ROOT / "scripts/c3-supermini-stack-usage-report.sh"),
                    str(tmp_path),
                    "4",
                ],
                cwd=ROOT,
                text=True,
                capture_output=True,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(
            "cumulative_bytes\tself_bytes\tcallee_path\tfunction\tlocation",
            result.stdout,
        )
        self.assertIn("264\t80\tcaller -> helper -> leaf\tcaller\t", result.stdout)
        self.assertIn("184\t120\thelper -> leaf\thelper\t", result.stdout)
        self.assertIn("160\t160\tindependent\tindependent\t", result.stdout)

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
        self.assertEqual(lines[0], "bytes\tfunction\tlocation\tmode")
        self.assertTrue(lines[1].startswith("10000\tfunction_9999\t"))
        self.assertTrue(lines[5].startswith("9996\tfunction_9995\t"))
        self.assertIn("top_rows\tmax_bytes\tsum_bytes\tsource_file", lines)
