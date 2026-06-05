from pathlib import Path
import json
import os
import re
import subprocess
import sys
import tempfile

from scripts.tests.zephyr_test_utils import ROOT, ZephyrScriptTestCase


class ZephyrRuntimeContractTests(ZephyrScriptTestCase):
    def test_default_runtime_keeps_wifi_event_state_gated(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")

        guard = "#if IS_ENABLED(CONFIG_NET_L2_WIFI_MGMT) && IS_ENABLED(CONFIG_NET_MGMT_EVENT) &&"
        self.assertIn(guard, runtime_h)
        guard_index = runtime_h.index(guard, runtime_h.index("struct sq_vm_runtime"))
        op_index = runtime_h.index("wifi_op_kind", guard_index)
        end_index = runtime_h.index("#endif", op_index)
        self.assertLess(guard_index, op_index)
        self.assertLess(runtime_h.index("wifi_scan_done"), end_index)
        self.assertNotIn("wifi_scan_sem_initialized", runtime_h)
        self.assertNotIn("struct k_sem wifi_scan_done", runtime_h)

    def test_diagnostic_banner_is_reemitted_after_optional_ble_startup(self):
        main_c = self.read("firmware/zephyr/src/main.c")

        banner = 'LOG_INF("SquidScript Zephyr firmware diagnostic boot");'
        self.assertEqual(main_c.count(banner), 2)
        first_banner = main_c.index(banner)
        ble_start = main_c.index("(void)sq_ble_smoke_start();")
        second_banner = main_c.index(banner, first_banner + 1)
        self.assertLess(first_banner, ble_start)
        self.assertGreater(second_banner, ble_start)

    def test_wifi_scan_results_use_resident_cursor_snapshot_not_transfer_scratch(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        runtime_c = self.read("firmware/zephyr/src/vm_runtime_wifi.c")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")
        runtime_body = runtime_h[
            runtime_h.index("struct sq_vm_runtime {") : runtime_h.index(
                "void sq_vm_runtime_init", runtime_h.index("struct sq_vm_runtime {")
            )
        ]

        self.assertIn("struct sq_vm_runtime_wifi_scan_scratch", runtime_h)
        self.assertIn("struct sq_vm_runtime_wifi_scan_scratch wifi_scan", runtime_h)
        self.assertNotIn("wifi_scan", runtime_h[runtime_h.index("union sq_vm_runtime_transfer") : runtime_h.index("};", runtime_h.index("union sq_vm_runtime_transfer"))])
        self.assertNotIn("SqvmWifiAccessPoint wifi_scan_networks", runtime_body)
        self.assertNotIn("char wifi_scan_ssids", runtime_body)
        self.assertNotIn("char wifi_scan_bssids", runtime_body)
        self.assertNotIn("char wifi_scan_auth", runtime_body)
        self.assertIn("struct sq_vm_runtime_wifi_scan_scratch *scan =", runtime_c)
        self.assertIn("out->network = runtime->wifi_scan.networks[index]", runtime_c)
        self.assertNotIn("runtime->transfer.wifi_scan", runtime_c)
        self.assertIn("runtime_static <= 12160", ztest)
        self.assertNotIn("runtime_static <= 13984", ztest)
        self.assertNotIn("runtime_static <= 14176", ztest)
        self.assertNotIn("runtime_static <= 14304", ztest)
        self.assertNotIn("runtime_static <= 14720", ztest)

    def test_runtime_transfer_scratch_has_diagnostic_owner_checks(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        runtime_impl = "\n".join(
            [
                self.read("firmware/zephyr/src/vm_runtime.c"),
                self.read("firmware/zephyr/src/vm_runtime_device_config.c"),
            ]
        )
        protocol_c = self.read("firmware/zephyr/src/device_protocol.c")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")
        ztest_kconfig = self.read("firmware/zephyr/tests/protocol/Kconfig")
        ztest_conf = self.read("firmware/zephyr/tests/protocol/prj.conf")

        self.assertIn("enum sq_vm_runtime_transfer_owner", runtime_h)
        self.assertIn("SQ_VM_RUNTIME_TRANSFER_SCRATCH", runtime_h)
        self.assertIn("SQ_VM_RUNTIME_TRANSFER_COMPLETION", runtime_h)
        self.assertNotIn("SQ_VM_RUNTIME_TRANSFER_WIFI_SCAN", runtime_h)
        self.assertIn("enum sq_vm_runtime_transfer_owner transfer_owner", runtime_h)
        self.assertIn("sq_vm_runtime_transfer_acquire", runtime_h)
        self.assertIn("sq_vm_runtime_transfer_release", runtime_h)
        self.assertIn("CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC", runtime_h)
        self.assertIn("return -EBUSY", runtime_h)

        self.assertNotIn("SQ_VM_RUNTIME_TRANSFER_WIFI_SCAN", runtime_impl)
        self.assertIn(
            "sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION)",
            runtime_impl,
        )
        self.assertIn(
            "sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION)",
            runtime_impl,
        )
        self.assertIn(
            "sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH)",
            runtime_impl,
        )
        self.assertIn(
            "sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH)",
            runtime_impl,
        )
        self.assertIn(
            "sq_vm_runtime_transfer_acquire(context->runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH)",
            protocol_c,
        )
        self.assertIn(
            "sq_vm_runtime_transfer_acquire(context->runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION)",
            protocol_c,
        )
        self.assertIn("test_runtime_transfer_owner_rejects_overlap", ztest)
        self.assertIn("config SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC", ztest_kconfig)
        self.assertIn("CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC=y", ztest_conf)

    def test_firmware_state_machines_are_documented_and_linked(self):
        doc = self.read("docs/firmware_state_machines.md")
        roadmap = self.read("ROADMAP.md")
        protocol_doc = self.read("docs/developer_repl_protocol.md")

        for heading in [
            "# Firmware State Machines",
            "## Protocol Transfer Sessions",
            "## Protocol Scratch Ownership",
            "## Device Input Buttons",
            "## Indicator Patterns",
            "## Bounded Queues",
        ]:
            self.assertIn(heading, doc)
        self.assertIn("```mermaid", doc)
        self.assertIn("docs/app_lifecycle_state_machine.md", doc)
        self.assertIn("docs/firmware_state_machines.md", protocol_doc)
        self.assertNotIn("## Explicit State Machines", roadmap)

    def test_protocol_transfer_sessions_use_explicit_phases(self):
        protocol_h = self.read("firmware/zephyr/src/device_protocol.h")
        protocol_c = self.read("firmware/zephyr/src/device_protocol.c")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")

        self.assertIn("enum sq_device_transfer_phase", protocol_h)
        for phase in [
            "SQ_DEVICE_TRANSFER_IDLE",
            "SQ_DEVICE_TRANSFER_RECEIVING",
            "SQ_DEVICE_TRANSFER_COMMITTING",
        ]:
            self.assertIn(phase, protocol_h)
        for struct_name in [
            "struct sq_device_install_session",
            "struct sq_device_temp_session",
            "struct sq_device_resource_session",
        ]:
            body = protocol_h[
                protocol_h.index(struct_name) : protocol_h.index("};", protocol_h.index(struct_name))
            ]
            self.assertIn("enum sq_device_transfer_phase phase;", body)
        self.assertIn("transfer_session_begin_receiving", protocol_c)
        self.assertIn("transfer_session_begin_committing", protocol_c)
        self.assertIn("transfer_session_finish_idle", protocol_c)
        self.assertIn("test_protocol_transfer_session_phases_track_begin_chunk_commit", ztest)

    def test_transfer_owner_exercise_fixture_uses_state_config_and_wifi_scan(self):
        app = self.read("tests/hardware/c3-supermini/transfer-owner-summary/main.squid")
        device = self.read(
            "tests/hardware/c3-supermini/transfer-owner-summary/device/indicator.sqdevice"
        )

        self.assertIn("state {", app)
        self.assertIn("@runs = @runs + 1", app)
        self.assertIn('device.config.load("package:device/indicator.sqdevice")', app)
        self.assertIn("service.wifi.scan()", app)
        self.assertIn("debug.print", app)
        self.assertIn("mode string 4:gpio", device)

    def test_wifi_station_hardware_script_waits_for_connected_status(self):
        script = self.read("scripts/zephyr-test-wifi-station-api.sh")

        self.assertIn(
            'wait_for_contains output "output=wifi station dev true"',
            script,
        )
        self.assertIn(
            'assert_file_contains "${output_out}" "output=wifi connect true null"',
            script,
        )
        self.assertNotIn(
            'wait_for_contains output "output=wifi connect true null"',
            script,
        )

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

        self.assertIn("#define SQVM_STORAGE_TRANSFER_CAPACITY 640", ffi_h)
        self.assertIn("pub const MAX_CODE_CHUNK_BYTES: usize = 640;", limits_rs)
        self.assertNotIn("#define SQVM_STORAGE_TRANSFER_CAPACITY 768", ffi_h)
        self.assertNotIn("pub const MAX_CODE_CHUNK_BYTES: usize = 768;", limits_rs)
        self.assertNotIn("#define SQVM_STORAGE_TRANSFER_CAPACITY 1024", ffi_h)
        self.assertNotIn("pub const MAX_CODE_CHUNK_BYTES: usize = 1024;", limits_rs)
        self.assertIn("union sq_vm_runtime_transfer", runtime_h)
        self.assertIn("uint8_t init_scratch[SQ_VM_RUNTIME_SCRATCH_BYTES]", runtime_h)
        self.assertIn("SqvmStorageCompletion completion", runtime_h)
        self.assertNotIn("uint8_t scratch[SQ_VM_RUNTIME_SCRATCH_BYTES];", runtime_body)
        self.assertNotIn("SqvmStorageCompletion completion;", runtime_body)
        self.assertIn("sizeof(runtime.transfer.init_scratch)", ztest)
        self.assertIn("SQVM_STORAGE_TRANSFER_CAPACITY <= 640", ztest)
        self.assertIn("runtime_static <= 12160", ztest)
        self.assertNotIn("runtime_static <= 13984", ztest)
        self.assertNotIn("runtime_static <= 14176", ztest)
        self.assertNotIn("runtime_static <= 14720", ztest)
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
        runtime_c = self.read("firmware/zephyr/src/vm_runtime_device_config.c")
        apply_start = runtime_c.index("static int __noinline sq_vm_runtime_apply_device_bindings")
        apply_body = runtime_c[
            apply_start : runtime_c.index("int32_t runtime_device_config_load", apply_start)
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

    def test_app_lifecycle_uses_explicit_phase_enum(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        protocol_c = self.read("firmware/zephyr/src/device_protocol.c")
        lifecycle_c = self.read("firmware/zephyr/src/app_lifecycle.c")
        runtime_body = runtime_h[
            runtime_h.index("struct sq_vm_runtime {") : runtime_h.index(
                "void sq_vm_runtime_init", runtime_h.index("struct sq_vm_runtime {")
            )
        ]
        poll_body = protocol_c[
            protocol_c.index("int sq_device_protocol_poll")
            : protocol_c.index("static int repeated_runtime_lines_response")
        ]

        self.assertIn("enum sq_vm_runtime_lifecycle_phase", runtime_h)
        for phase in [
            "SQ_VM_RUNTIME_LIFECYCLE_IDLE",
            "SQ_VM_RUNTIME_LIFECYCLE_LAUNCH_REQUESTED",
            "SQ_VM_RUNTIME_LIFECYCLE_EXIT_FOR_LAUNCH",
            "SQ_VM_RUNTIME_LIFECYCLE_RETURN_REQUESTED",
            "SQ_VM_RUNTIME_LIFECYCLE_SLEEP_REQUESTED",
            "SQ_VM_RUNTIME_LIFECYCLE_SLEEP_CHECKPOINT",
        ]:
            self.assertIn(phase, runtime_h)
        self.assertIn("enum sq_vm_runtime_lifecycle_phase lifecycle_phase;", runtime_body)
        self.assertIn("enum sq_vm_runtime_arm_phase", runtime_h)
        self.assertIn("enum sq_vm_runtime_arm_phase arm_phase;", runtime_body)
        self.assertIn("char arm_target_app[SQ_APP_STORE_APP_ID_MAX];", runtime_body)
        self.assertIn("switch (runtime->lifecycle_phase)", lifecycle_c)
        self.assertIn("sq_app_lifecycle_next_step", poll_body)
        self.assertIn("switch (step.kind)", poll_body)
        for obsolete_flag in [
            "bool pending_launch_active;",
            "bool pending_arm_active;",
            "bool arm_registration_active;",
            "bool lifecycle_launch_after_exit;",
            "bool planned_sleep_requested;",
            "bool planned_sleep_preparing;",
        ]:
            self.assertNotIn(obsolete_flag, runtime_body)

    def test_app_lifecycle_state_machine_docs_are_linked(self):
        lifecycle_doc = self.read("docs/app_lifecycle_state_machine.md")
        expected_terms = [
            "host `app launch`",
            "`app.launch`",
            "`app.exit`",
            "fallback `main`",
            "armed timer",
            "planned sleep",
            "start reason",
            "`device reset`",
            "`device storage-format`",
            "`-ENOSPC`",
            "```mermaid",
        ]
        for term in expected_terms:
            with self.subTest(term=term):
                self.assertIn(term, lifecycle_doc)

        for path in [
            "docs/language_spec.md",
            "docs/developer_repl_protocol.md",
            "docs/firmware_build_architecture.md",
            "docs/firmware_app_storage.md",
            "docs/hardware_target_tests.md",
        ]:
            with self.subTest(path=path):
                self.assertIn("docs/app_lifecycle_state_machine.md", self.read(path))

    def test_planned_resume_uses_protocol_scratch_not_stack_arrays(self):
        protocol_h = self.read("firmware/zephyr/src/device_protocol.h")
        protocol_c = self.read("firmware/zephyr/src/device_protocol.c")
        main_c = self.read("firmware/zephyr/src/main.c")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")

        write_start = protocol_c.index("static int write_planned_resume_file")
        write_end = protocol_c.index("static int sqdp_status_to_protocol_result")
        write_body = protocol_c[write_start:write_end]
        restore_start = protocol_c.index("int sq_device_protocol_restore_planned_resume")
        restore_end = protocol_c.index("int sq_device_protocol_poll")
        restore_body = protocol_c[restore_start:restore_end]

        self.assertIn("enum sq_device_protocol_scratch_owner", protocol_h)
        self.assertIn("SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME", protocol_h)
        self.assertIn("struct sq_device_protocol_scratch", protocol_h)
        self.assertIn("struct sq_device_protocol_scratch *scratch;", protocol_h)
        self.assertIn("static struct sq_device_protocol_scratch protocol_scratch;", main_c)
        self.assertIn(".scratch = &protocol_scratch", main_c)
        self.assertIn("protocol_scratch_acquire(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME)", write_body)
        self.assertIn("protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME)", write_body)
        self.assertIn("protocol_scratch_acquire(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME)", restore_body)
        self.assertIn("protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME)", restore_body)
        for body in [write_body, restore_body]:
            self.assertNotIn("struct sq_device_planned_resume_record record = {0};", body)
            self.assertNotIn("uint8_t bytes[SQ_DEVICE_PLANNED_RESUME_LEN];", body)
            self.assertNotIn("char path[SQ_APP_STORE_PLANNED_RESUME_PATH_MAX];", body)
            self.assertNotIn("char temp_path[SQ_APP_STORE_PLANNED_RESUME_PATH_MAX];", body)
            self.assertNotIn("char final_path[SQ_APP_STORE_PLANNED_RESUME_PATH_MAX];", body)
            self.assertNotIn("struct fs_file_t file;", body)
        self.assertIn("test_planned_resume_scratch_rejects_overlap", ztest)

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
        device_config_c = self.read("firmware/zephyr/src/vm_runtime_device_config.c")
        start_body = runtime_c[
            runtime_c.index("int sq_vm_runtime_start")
            : runtime_c.index(
                "int sq_vm_runtime_record_output",
                runtime_c.index("int sq_vm_runtime_start"),
            )
        ]
        work_body = runtime_c[
            runtime_c.index("static void runtime_run_job")
            : runtime_c.index("void sq_vm_runtime_init", runtime_c.index("static void runtime_run_job"))
        ]

        self.assertIn("sq_vm_runtime_prepare_app_start", runtime_c)
        self.assertIn("int __noinline sq_vm_runtime_prepare_app_start", device_config_c)
        self.assertIn("static int __noinline sq_vm_runtime_apply_saved_device_config", device_config_c)
        self.assertIn("static int __noinline sq_vm_runtime_apply_device_bindings", device_config_c)
        self.assertIn("sq_vm_runtime_prepare_app_start(runtime)", work_body)
        self.assertIn("runtime->start_apply_bindings", start_body)
        self.assertNotIn("sq_vm_runtime_apply_device_bindings(runtime)", start_body)
        self.assertNotIn("sq_vm_runtime_apply_saved_device_config(runtime)", start_body)
        self.assertNotIn("sq_vm_runtime_apply_target_default_indicator_binding(runtime)", start_body)

    def test_runtime_keeps_bounded_diagnostic_history(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")
        limits = self.read("compiler/rust/crates/squidvm-core/src/limits.rs")

        self.assertIn("pub const MAX_STACK: usize = 16;", limits)
        self.assertNotIn("pub const MAX_STACK: usize = 32;", limits)

    def test_event_name_slot_fits_timer_breathe_marker(self):
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")

        self.assertIn('strlen("timer.breathe.marker") < SQ_VM_RUNTIME_EVENT_LEN', ztest)

    def test_repeated_line_responses_use_rust_encoder_without_c_payload_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int repeated_runtime_lines_response")
        end = protocol.index("static int __noinline lifecycle_response")
        body = protocol[start:end]

        self.assertIn("sqdp_encode_line_response", body)
        self.assertNotIn("uint8_t payload[512]", body)
        self.assertNotIn("append_string_field(payload", body)

    def test_lifecycle_response_encodes_without_resident_timer_staging(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int __noinline lifecycle_response")
        end = protocol.index("static int __noinline state_get_response")
        body = protocol[start:end]

        self.assertIn("encode_lifecycle_header", body)
        self.assertIn("append_line_payload", body)
        self.assertIn("runtime->armed_timers[i].active", body)
        self.assertIn("runtime->armed_timers[i].app_id", body)
        self.assertIn("runtime->armed_timers[i].event", body)
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
        end = protocol.index("static int clear_runtime_context")
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

        self.assertIn("#define SQ_DEVICE_RESPONSE_BYTES 1088u", header)
        self.assertNotIn("#define SQ_DEVICE_RESPONSE_BYTES 1120u", header)
        self.assertNotIn("#define SQ_DEVICE_RESPONSE_BYTES 824u", header)
        self.assertNotIn("#define SQ_DEVICE_RESPONSE_BYTES 820u", header)
        self.assertNotIn("#define SQ_DEVICE_RESPONSE_BYTES 916u", header)
        self.assertNotIn("#define SQ_DEVICE_RESPONSE_BYTES 826u", header)
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

    def test_protocol_dispatch_uses_narrow_request_header(self):
        protocol = self.read("firmware/zephyr/src/protocol.h")
        protocol_c = self.read("firmware/zephyr/src/protocol.c")
        device_protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = device_protocol.index("int sq_device_protocol_handle_frame")
        body = device_protocol[start:]

        self.assertIn("struct sq_protocol_request", protocol)
        self.assertIn("int sq_protocol_decode_request", protocol)
        self.assertIn("sq_protocol_decode_request", protocol_c)
        self.assertIn("struct sq_protocol_request frame;", body)
        self.assertIn("sq_protocol_decode_request(request, request_len, &frame)", body)
        self.assertNotIn("struct sq_protocol_frame frame;", body)
        self.assertNotIn("sq_protocol_decode_frame(request, request_len, &frame)", body)

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

    def test_app_launch_avoids_local_app_id_buffer(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        start = protocol.index("static int __noinline launch_app")
        end = protocol.index("static int start_installed_app", start)
        body = protocol[start:end]

        self.assertIn("sq_app_store_vm_storage_for_app_bytes", app_store_h)
        self.assertNotIn("char app_id_buffer[SQ_APP_STORE_APP_ID_MAX];", body)
        self.assertNotIn("memcpy(app_id_buffer, launch.app_id, launch.app_id_len);", body)
        self.assertIn("sq_app_lifecycle_request_launch(context->runtime, launch.app_id", body)
        self.assertIn("ok_response(request, response, response_cap, response_len)", body)

    def test_app_launch_uses_pending_lifecycle_chain_without_direct_start(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        launch_start = protocol.index("static int __noinline launch_app")
        launch_end = protocol.index("static int start_installed_app", launch_start)
        launch_body = protocol[launch_start:launch_end]

        self.assertIn("sq_app_lifecycle_request_launch", launch_body)
        self.assertNotIn("sq_device_protocol_poll(context)", launch_body)
        self.assertNotIn("sq_vm_runtime_wait_idle", launch_body)
        self.assertNotIn("start_foreground_app_bytes", protocol)
        self.assertNotIn("start_installed_app_bytes(context, launch.app_id", launch_body)

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
        self.assertNotIn("dispatch_event_from_parts", body)
        self.assertIn("sq_app_store_vm_storage_for_app_bytes", body)
        self.assertIn("sq_vm_runtime_start_event(context->runtime, &backend,", body)

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

    def test_indicator_patterns_use_single_state_machine(self):
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        runtime_c = self.read("firmware/zephyr/src/vm_runtime_indicator_gpio.c")
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")

        self.assertIn("enum sq_vm_runtime_indicator_pattern", runtime_h)
        for pattern in [
            "SQ_VM_RUNTIME_INDICATOR_STEADY",
            "SQ_VM_RUNTIME_INDICATOR_BREATHE",
            "SQ_VM_RUNTIME_INDICATOR_BLINK",
        ]:
            self.assertIn(pattern, runtime_h)
        self.assertIn("enum sq_vm_runtime_indicator_pattern indicator_pattern;", runtime_h)
        self.assertNotIn("indicator_breathe_active", runtime_h)
        self.assertNotIn("indicator_blink_active", runtime_h)
        self.assertIn("switch (runtime->indicator_pattern)", runtime_c)
        self.assertIn("test_indicator_pattern_state_machine_transitions", ztest)

    def test_serial_protocol_reader_allows_long_wifi_scan_launch_response(self):
        runtime = self.read("firmware/zephyr/src/vm_runtime_wifi.c")
        serial = self.read("compiler/rust/crates/squidc-cli/src/serial.rs")

        scan_timeout = re.search(
            r"#define SQ_VM_RUNTIME_WIFI_SCAN_TIMEOUT_MS ([0-9]+)", runtime
        )
        cli_timeout = re.search(
            r"const DEFAULT_TIMEOUT: Duration = Duration::from_secs\(([0-9]+)\);", serial
        )

        self.assertIsNotNone(scan_timeout)
        self.assertIsNotNone(cli_timeout)
        self.assertGreaterEqual(int(cli_timeout.group(1)) * 1000, int(scan_timeout.group(1)) + 1000)
        self.assertIn("read_protocol_frame(DEFAULT_TIMEOUT)", serial)
        self.assertIn("complete_frame_end_from_stream", serial)

    def test_app_registry_scan_uses_narrow_path_scratch_after_opening_directory(self):
        app_store = self.read("firmware/zephyr/src/app_store.c")
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        start = app_store.index("int sq_app_store_scan_registry_with_path")
        end = app_store.index("static int delete_files_under")
        body = app_store[start:end]
        public_start = app_store.index("int sq_app_store_scan_registry(")
        public_end = app_store.index("int sq_app_store_scan_registry_with_path")
        public_body = app_store[public_start:public_end]

        self.assertIn("#define SQ_APP_STORE_APP_FILE_PATH_MAX 64", app_store_h)
        self.assertNotIn("#define SQ_APP_STORE_APP_FILE_PATH_MAX 72", app_store_h)
        self.assertIn("int sq_app_store_scan_registry_with_path", app_store_h)
        self.assertIn("char *path, size_t path_cap", body)
        self.assertNotIn("char path[SQ_APP_STORE_APP_FILE_PATH_MAX];", body)
        self.assertNotIn("char path[SQ_APP_STORE_PATH_MAX];", body)
        self.assertNotIn("char apps_path[SQ_APP_STORE_PATH_MAX];", body)
        self.assertNotIn("char sqbc_path[SQ_APP_STORE_PATH_MAX];", body)
        self.assertNotIn("struct fs_dirent sqbc_entry", body)
        self.assertIn('join_path2(path, path_cap, mount_point, "apps")', body)
        self.assertIn("fs_opendir(&dir, path)", body)
        self.assertIn('format_app_path(path, path_cap, mount_point, entry.name,', body)
        self.assertIn("fs_stat(path, &entry)", body)
        self.assertIn("struct sq_app_registry_entry *record = NULL;", body)
        self.assertIn("record = &registry->apps[registry->count];", body)
        self.assertIn("registry->count++", body)
        self.assertIn("char path[SQ_APP_STORE_APP_FILE_PATH_MAX];", public_body)
        self.assertIn("sq_app_store_scan_registry_with_path(mount_point, registry, path,",
                      public_body)

    def test_commit_install_updates_registry_without_full_scan(self):
        app_store = self.read("firmware/zephyr/src/app_store.c")
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        commit_start = protocol.index("static int __noinline commit_install")
        commit_end = protocol.index("static int start_installed_app", commit_start)
        commit_body = protocol[commit_start:commit_end]
        commit_body_flat = " ".join(commit_body.split())

        self.assertIn("int sq_app_store_update_registry_entry_with_path", app_store_h)
        self.assertIn("int sq_app_store_update_registry_entry_with_path", app_store)
        self.assertIn(
            "sq_app_store_update_registry_entry_with_path( context->store_mount_point, "
            "context->mutable_registry, session->app_id,",
            commit_body_flat,
        )
        self.assertIn("session->staging_path", commit_body)
        self.assertIn("sizeof(session->staging_path)", commit_body)
        self.assertNotIn("sq_app_store_scan_registry_with_path(context->store_mount_point,",
                         commit_body)

    def test_registry_entry_update_avoids_dirent_stat_buffer(self):
        app_store = self.read("firmware/zephyr/src/app_store.c")
        start = app_store.index("int sq_app_store_update_registry_entry_with_path")
        end = app_store.index("static int delete_one_under", start)
        body = app_store[start:end]

        self.assertNotIn("struct fs_dirent entry;", body)
        self.assertNotIn("fs_stat(path, &entry)", body)
        self.assertIn("struct fs_file_t main_sqbc;", body)
        self.assertIn("fs_open(&main_sqbc, path, FS_O_READ)", body)
        self.assertIn("fs_seek(&main_sqbc, 0, FS_SEEK_END)", body)
        self.assertIn("sqbc_size = fs_tell(&main_sqbc)", body)
        self.assertIn("record->sqbc_len = (uint32_t)sqbc_size;", body)

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

    def test_trigger_registration_uses_dedicated_storage_without_local_sqbc_path_scratch(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        start = protocol.index("static int __noinline register_app_triggers")
        end = protocol.index("int sq_device_protocol_poll")
        body = protocol[start:end]

        self.assertIn("#define SQ_APP_STORE_APP_FILE_PATH_MAX 64", app_store_h)
        self.assertNotIn("#define SQ_APP_STORE_APP_FILE_PATH_MAX 72", app_store_h)
        self.assertNotIn("char sqbc_path[SQ_APP_STORE_APP_FILE_PATH_MAX];", body)
        self.assertNotIn("char sqbc_path[SQ_APP_STORE_PATH_MAX];", body)
        self.assertIn("sq_app_store_vm_storage_for_app(context->store_mount_point, app_id,", body)
        self.assertIn("context->trigger_storage == NULL", body)
        self.assertIn("trigger_storage);", body)
        self.assertNotIn("context->launch_storage);", body)

    def test_protocol_begin_and_commit_validation_avoid_unused_action_stack(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        ffi_rs = self.read("compiler/rust/crates/squidvm-ffi/src/lib.rs")

        for start_marker, end_marker in [
            ("static int __noinline begin_install", "static int __noinline append_install_chunk"),
            ("static int __noinline begin_resource_install", "static int __noinline append_resource_chunk"),
            ("static int __noinline commit_resource_install", "struct temp_storage_backend"),
            ("static int __noinline commit_temp_run", "static int __noinline commit_install"),
            ("static int __noinline commit_install", "static int start_installed_app"),
        ]:
            with self.subTest(handler=start_marker):
                start = protocol.index(start_marker)
                end = protocol.index(end_marker)
                body = protocol[start:end]
                self.assertNotIn("SqdpAction action", body)
                self.assertIn("NULL", body)

        self.assertIn("if !out_action.is_null() {", ffi_rs)

    def test_temp_run_commit_requests_lifecycle_temp_launch(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int __noinline commit_temp_run")
        end = protocol.index("static int __noinline commit_install")
        body = protocol[start:end]

        self.assertIn("sq_vm_fs_storage_backend", body)
        self.assertIn("context->runtime->job_backend.reset_state", body)
        self.assertIn("sq_app_lifecycle_clear_temp_routes(context->runtime)", body)
        self.assertIn("sq_app_lifecycle_request_temp_launch", body)
        self.assertNotIn("sq_vm_runtime_start_event", body)
        self.assertNotIn("struct sq_vm_storage_backend backend;", body)
        self.assertNotIn("sq_vm_runtime_start(context->runtime, &backend,", body)

    def test_resource_install_paths_reuse_single_path_scratch(self):
        app_store = self.read("firmware/zephyr/src/app_store.c")
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        protocol = self.read("firmware/zephyr/src/device_protocol.c")

        parent_start = app_store.index("static int ensure_resource_parent_dirs")
        parent_end = app_store.index("int sq_app_store_prepare_filesystem")
        parent_body = app_store[parent_start:parent_end]
        self.assertIn("char *dir, size_t dir_cap", parent_body)
        self.assertNotIn("char dir[SQ_APP_STORE_PATH_MAX];", parent_body)
        self.assertIn("snprintf(dir, dir_cap,", parent_body)
        self.assertIn("dir_len + 1 + segment_len >= dir_cap", parent_body)

        commit_start = app_store.index("int sq_app_store_commit_staged_resource")
        commit_end = app_store.index("int sq_app_store_resource_path")
        commit_body = app_store[commit_start:commit_end]
        self.assertIn("char path[SQ_APP_STORE_PATH_MAX];", commit_body)
        self.assertIn("validate_app_main_sqbc_with_path(path, path_len, mount_point, app_id)",
                      commit_body)
        self.assertNotIn("struct fs_file_t main_sqbc;", commit_body)
        self.assertNotIn("struct fs_dirent entry;", commit_body)
        self.assertNotIn("char sqbc_path[SQ_APP_STORE_PATH_MAX];", commit_body)
        self.assertNotIn("char final_path[SQ_APP_STORE_PATH_MAX];", commit_body)
        self.assertIn("ensure_resource_parent_dirs(path, path_len, mount_point, app_id,", commit_body)
        self.assertIn("sq_app_store_resource_path(mount_point, app_id, resource_path, path, path_len)",
                      commit_body)
        self.assertIn("fs_rename(staging_path, path)", commit_body)
        self.assertIn("path, sizeof(path)", commit_body)
        self.assertIn("sq_app_store_commit_staged_resource_with_path", app_store_h)

        protocol_commit_start = protocol.index("static int __noinline commit_resource_install")
        protocol_commit_end = protocol.index("struct temp_storage_backend", protocol_commit_start)
        protocol_commit_body = protocol[protocol_commit_start:protocol_commit_end]
        self.assertIn("sq_app_store_commit_staged_resource_with_path(", protocol_commit_body)
        self.assertIn("(char *)response, response_cap", protocol_commit_body)
        self.assertNotIn("sq_app_store_commit_staged_resource(context->store_mount_point",
                         protocol_commit_body)

        install_start = app_store.index("int sq_app_store_install_resource")
        install_end = app_store.index("int sq_app_store_scan_registry")
        install_body = app_store[install_start:install_end]
        self.assertIn("char path[SQ_APP_STORE_PATH_MAX];", install_body)
        self.assertIn("validate_app_main_sqbc_with_path(path, sizeof(path), mount_point, app_id)",
                      install_body)
        self.assertNotIn("struct fs_file_t main_sqbc;", install_body)
        self.assertNotIn("struct fs_dirent entry;", install_body)
        self.assertNotIn("char sqbc_path[SQ_APP_STORE_PATH_MAX];", install_body)
        self.assertIn("ensure_resource_parent_dirs(path, sizeof(path), mount_point, app_id,", install_body)
        self.assertIn("sq_app_store_resource_path(mount_point, app_id, resource_path, path,", install_body)
        self.assertIn("write_file(path, bytes, len)", install_body)

    def test_resource_install_validates_main_sqbc_with_caller_path_scratch(self):
        app_store = self.read("firmware/zephyr/src/app_store.c")

        self.assertIn("static int validate_app_main_sqbc_with_path", app_store)
        helper_start = app_store.index("static int validate_app_main_sqbc_with_path")
        helper_end = app_store.index("static int ensure_resource_parent_dir")
        helper_body = app_store[helper_start:helper_end]
        self.assertIn("char *path, size_t path_cap", helper_body)
        self.assertNotIn("char path[SQ_APP_STORE_APP_FILE_PATH_MAX];", helper_body)
        self.assertIn('format_app_path(path, path_cap, mount_point, app_id, "main.sqbc")', helper_body)
        self.assertIn("fs_open(&main_sqbc, path, FS_O_READ)", helper_body)
        self.assertNotIn("static int validate_app_main_sqbc(", app_store)

        commit_start = app_store.index("int sq_app_store_commit_staged_resource")
        commit_end = app_store.index("int sq_app_store_resource_path")
        commit_body = app_store[commit_start:commit_end]
        self.assertIn("char path[SQ_APP_STORE_PATH_MAX];", commit_body)
        self.assertIn("validate_app_main_sqbc_with_path(path, path_len, mount_point, app_id)",
                      commit_body)
        self.assertIn("path, sizeof(path)", commit_body)
        self.assertNotIn("validate_app_main_sqbc(mount_point, app_id)", commit_body)
        self.assertNotIn("struct fs_file_t main_sqbc;", commit_body)

        install_start = app_store.index("int sq_app_store_install_resource")
        install_end = app_store.index("int sq_app_store_scan_registry")
        install_body = app_store[install_start:install_end]
        self.assertIn("char path[SQ_APP_STORE_PATH_MAX];", install_body)
        self.assertIn("validate_app_main_sqbc_with_path(path, sizeof(path), mount_point, app_id)",
                      install_body)
        self.assertNotIn("validate_app_main_sqbc(mount_point, app_id)", install_body)
        self.assertNotIn("struct fs_file_t main_sqbc;", install_body)

    def test_resource_parent_dirs_avoid_dirent_stat_probe(self):
        app_store = self.read("firmware/zephyr/src/app_store.c")

        self.assertIn("static int ensure_resource_parent_dir(", app_store)
        helper_start = app_store.index("static int ensure_resource_parent_dir(")
        helper_end = app_store.index("static int ensure_resource_parent_dirs")
        helper_body = app_store[helper_start:helper_end]
        self.assertIn("fs_mkdir(path)", helper_body)
        self.assertIn("result == 0 || result == -EEXIST", helper_body)
        self.assertNotIn("struct fs_dirent", helper_body)
        self.assertNotIn("fs_stat", helper_body)

        parent_start = app_store.index("static int ensure_resource_parent_dirs")
        parent_end = app_store.index("static inline int prepare_filesystem_with_path")
        parent_body = app_store[parent_start:parent_end]
        self.assertIn("ensure_resource_parent_dir(dir)", parent_body)
        self.assertNotIn("ensure_directory(dir)", parent_body)

    def test_file_backed_state_and_config_reads_avoid_dirent_size_probe(self):
        storage = self.read("firmware/zephyr/src/vm_fs_storage.c")
        runtime = self.read("firmware/zephyr/src/vm_runtime_device_config.c")

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
        self.assertIn("prepare_filesystem_with_path(path, sizeof(path), mount_point)", install_body)
        self.assertNotIn("sq_app_store_prepare_filesystem(mount_point)", install_body)
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

        fallback_start = app_store.index("static int format_filesystem_by_delete_walk")
        fallback_end = app_store.index("#if defined(CONFIG_FILE_SYSTEM_LITTLEFS)", fallback_start)
        fallback_body = app_store[fallback_start:fallback_end]
        self.assertIn("delete_files_under(path, sizeof(path), &deleted_any)", fallback_body)

        format_start = app_store.index("int sq_app_store_format_filesystem")
        format_body = app_store[format_start:]
        self.assertIn("format_filesystem_by_delete_walk(mount_point)", format_body)

    def test_target_storage_format_erases_partition_without_delete_walk(self):
        prj_conf = self.read("firmware/zephyr/prj.conf")
        app_store = self.read("firmware/zephyr/src/app_store.c")

        self.assertNotIn("CONFIG_FILE_SYSTEM_MKFS=y", prj_conf)
        self.assertIn("static int format_target_filesystem(", app_store)

        target_start = app_store.index("static int format_target_filesystem(")
        target_end = app_store.index("#endif", target_start)
        target_body = app_store[target_start:target_end]
        self.assertIn("fs_unmount(&sq_app_store_target_mount)", target_body)
        self.assertIn("flash_area_open(PARTITION_ID(storage_partition), &area)", target_body)
        self.assertIn("flash_area_erase(area, 0, area->fa_size)", target_body)
        self.assertIn("flash_area_close(area)", target_body)
        self.assertIn("fs_mount(&sq_app_store_target_mount)", target_body)
        self.assertNotIn("fs_mkfs", target_body)
        self.assertNotIn("delete_files_under", target_body)
        self.assertNotIn("char path[SQ_APP_STORE_PATH_MAX];", target_body)

        public_start = app_store.index("int sq_app_store_format_filesystem")
        public_end = app_store.index("const struct sq_app_registry_entry")
        public_body = app_store[public_start:public_end]
        self.assertIn("format_target_filesystem(mount_point)", public_body)
        self.assertNotIn("char path[SQ_APP_STORE_PATH_MAX];", public_body)
        self.assertNotIn("delete_files_under(path", public_body)

    def test_format_prepare_reuses_format_path_scratch(self):
        app_store = self.read("firmware/zephyr/src/app_store.c")
        self.assertIn("prepare_filesystem_with_path", app_store)
        prepare_start = app_store.index("prepare_filesystem_with_path")
        prepare_end = app_store.index("int sq_app_store_prepare_filesystem")
        prepare_body = app_store[prepare_start:prepare_end]
        self.assertIn("char *path, size_t path_cap", prepare_body)
        self.assertNotIn("char path[SQ_APP_STORE_PATH_MAX];", prepare_body)
        self.assertIn("join_path2(path, path_cap, mount_point,", prepare_body)

        public_start = app_store.index("int sq_app_store_prepare_filesystem")
        public_end = app_store.index("int sq_app_store_mount_target_filesystem")
        public_body = app_store[public_start:public_end]
        self.assertIn("char path[SQ_APP_STORE_PATH_MAX];", public_body)
        self.assertIn("join_path2(path, sizeof(path), mount_point,", public_body)
        self.assertNotIn("prepare_filesystem_with_path(path, sizeof(path), mount_point)", public_body)

        fallback_start = app_store.index("static int format_filesystem_by_delete_walk")
        fallback_end = app_store.index("#if defined(CONFIG_FILE_SYSTEM_LITTLEFS)", fallback_start)
        fallback_body = app_store[fallback_start:fallback_end]
        self.assertIn("prepare_filesystem_with_path(path, sizeof(path), mount_point)", fallback_body)

        format_start = app_store.index("int sq_app_store_format_filesystem")
        format_body = app_store[format_start:]
        self.assertIn("format_filesystem_by_delete_walk(mount_point)", format_body)
        self.assertNotIn("return sq_app_store_prepare_filesystem(mount_point);", format_body)

    def test_staged_install_begin_reuses_staging_path_for_prepare_and_app_dir(self):
        app_store = self.read("firmware/zephyr/src/app_store.c")
        start = app_store.index("int sq_app_store_begin_staged_install")
        end = app_store.index("int sq_app_store_begin_temp_run")
        body = app_store[start:end]

        self.assertNotIn("char app_dir[SQ_APP_STORE_APP_FILE_PATH_MAX];", body)
        self.assertIn("prepare_staged_app_path(staging_path, staging_path_len, mount_point, app_id,",
                      body)
        self.assertIn('"main.sqbc.tmp"', body)
        self.assertNotIn("prepare_filesystem_with_path", body)
        self.assertNotIn("sq_app_store_prepare_filesystem(mount_point)", body)

    def test_transfer_begin_reuses_staging_path_for_prepare(self):
        app_store = self.read("firmware/zephyr/src/app_store.c")

        temp_start = app_store.index("int sq_app_store_begin_temp_run")
        temp_end = app_store.index("int sq_app_store_begin_staged_resource")
        temp_body = app_store[temp_start:temp_end]
        self.assertIn("prepare_tmp_staging_path(staging_path, staging_path_len, mount_point,",
                      temp_body)
        self.assertIn('"temp-run.sqbc.tmp"', temp_body)
        self.assertNotIn("prepare_filesystem_with_path", temp_body)
        self.assertNotIn("sq_app_store_prepare_filesystem(mount_point)", temp_body)
        self.assertNotIn('join_path2(staging_path, staging_path_len, mount_point, "tmp/temp-run.sqbc.tmp")',
                         temp_body)

        resource_start = app_store.index("int sq_app_store_begin_staged_resource")
        resource_end = app_store.index("int sq_app_store_write_staged_chunk")
        resource_body = app_store[resource_start:resource_end]
        self.assertIn("prepare_tmp_staging_path(staging_path, staging_path_len, mount_point,",
                      resource_body)
        self.assertIn('"resource.tmp"', resource_body)
        self.assertNotIn("prepare_filesystem_with_path", resource_body)
        self.assertNotIn("sq_app_store_prepare_filesystem(mount_point)", resource_body)
        self.assertNotIn('join_path2(staging_path, staging_path_len, mount_point, "tmp/resource.tmp")',
                         resource_body)

    def test_protocol_poll_uses_runtime_scratch_instead_of_stack_arrays(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        runtime = self.read("firmware/zephyr/src/vm_runtime.c")
        lifecycle = self.read("firmware/zephyr/src/app_lifecycle.c")
        start = protocol.index("int sq_device_protocol_poll")
        end = protocol.index("static int repeated_runtime_lines_response")
        body = protocol[start:end]

        self.assertNotIn("char target[SQ_APP_STORE_APP_ID_MAX];", body)
        self.assertNotIn("char armed_event[SQ_VM_RUNTIME_EVENT_LEN];", body)
        self.assertIn(
            "sq_app_lifecycle_pop_return(runtime, runtime->lifecycle_target_app,",
            lifecycle,
        )
        self.assertIn("sq_vm_runtime_next_due_armed_timer", body)
        self.assertIn("due_app, sizeof(due_app), due_event", body)
        self.assertIn("(const uint8_t *)step.event", body)
        self.assertIn("strlen(step.event), step.set_current, step.temp_app);", body)
        self.assertIn("memmove(runtime->event, event, event_len);", runtime)
        self.assertIn("runtime->event[event_len] = '\\0';", runtime)

    def test_protocol_event_dispatch_uses_byte_slice_runtime_start(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        app_store_c = self.read("firmware/zephyr/src/app_store.c")
        runtime_c = self.read("firmware/zephyr/src/vm_runtime.c")
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        start = protocol.index("static int dispatch_event_from_parts")
        end = protocol.index("static int __noinline dispatch_event_request", start)
        body = protocol[start:end]

        self.assertIn("sq_vm_runtime_start_event", runtime_h)
        self.assertIn("int sq_vm_runtime_start_event(struct sq_vm_runtime *runtime,", runtime_c)
        self.assertIn("return sq_vm_runtime_start_event(runtime, backend,", runtime_c)
        self.assertNotIn("char event_buffer[SQ_VM_RUNTIME_EVENT_LEN];", body)
        self.assertNotIn("memcpy(event_buffer, event, event_len);", body)
        self.assertNotIn("char app_id_buffer[SQ_APP_STORE_APP_ID_MAX];", body)
        self.assertNotIn("memcpy(app_id_buffer, app_id, app_id_len);", body)
        self.assertIn("sq_app_store_vm_storage_for_app_bytes", app_store_h)
        self.assertIn("sq_app_store_vm_storage_for_app_bytes", app_store_c)
        self.assertIn("sq_app_store_vm_storage_for_app_bytes", body)
        self.assertIn("use_temp_backend = context->runtime->current_app_temp", body)
        self.assertIn("&context->runtime->job_backend", body)
        self.assertIn("sq_vm_runtime_start_event(context->runtime, &backend, event, event_len)", body)

    def test_key_dispatch_uses_runtime_event_scratch(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        start = protocol.index("static int __noinline dispatch_key")
        end = protocol.index("static int __noinline wifi_profile_set", start)
        body = protocol[start:end]

        self.assertNotIn("uint8_t event[SQ_VM_RUNTIME_EVENT_LEN];", body)
        self.assertIn("context->runtime == NULL", body)
        self.assertIn("uint8_t *event = (uint8_t *)context->runtime->event;", body)
        self.assertIn("sqdp_prepare_key_event(request_bytes, request_len, event,",
                      body)
        self.assertIn("sizeof(context->runtime->event)", body)

    def test_trigger_registration_uses_dedicated_runtime_storage_path_buffers(self):
        protocol = self.read("firmware/zephyr/src/device_protocol.c")
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        app_store_c = self.read("firmware/zephyr/src/app_store.c")
        start = protocol.index("static int __noinline register_app_triggers")
        end = protocol.index("int sq_device_protocol_poll")
        body = protocol[start:end]

        self.assertIn("sq_app_store_sqbc_path", app_store_h)
        self.assertIn("int sq_app_store_sqbc_path", app_store_c)
        self.assertNotIn("char sqbc_path[SQ_APP_STORE_APP_FILE_PATH_MAX];", body)
        self.assertNotIn("struct sq_vm_fs_storage trigger_storage", body)
        self.assertIn("struct sq_app_store_vm_storage *trigger_storage;", body)
        self.assertIn("context->trigger_storage == NULL", body)
        self.assertIn("trigger_storage = context->trigger_storage;", body)
        self.assertIn("sq_app_store_vm_storage_for_app(context->store_mount_point, app_id,", body)
        self.assertIn("trigger_storage);", body)
        self.assertIn("sq_app_store_vm_storage_backend(trigger_storage)", body)
        self.assertNotIn("context->launch_storage);", body)
        self.assertNotIn("sq_app_store_vm_storage_backend(context->launch_storage)", body)
        self.assertIn("static int __noinline register_app_triggers", protocol)
        self.assertIn("static int __noinline register_app_trigger_timer", protocol)
        self.assertNotIn("SqvmTriggerTimer timer = {0};", body)

    def test_device_config_package_load_formats_resource_path_from_bytes(self):
        runtime = self.read("firmware/zephyr/src/vm_runtime_device_config.c")
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
        runtime = self.read("firmware/zephyr/src/vm_runtime_device_config.c")
        app_store_h = self.read("firmware/zephyr/src/app_store.h")
        app_store_c = self.read("firmware/zephyr/src/app_store.c")
        path_start = app_store_c.index("int sq_app_store_device_config_path")
        path_end = app_store_c.index("int sq_app_store_install_resource")
        path_body = app_store_c[path_start:path_end]
        load_start = runtime.index("static int __noinline sq_vm_runtime_apply_saved_device_config")
        load_end = runtime.index("static int __noinline sq_vm_runtime_apply_device_bindings")
        load_body = runtime[load_start:load_end]
        save_start = runtime.index("int sq_vm_runtime_device_config_save")
        save_end = runtime.index("int32_t runtime_device_config_save", save_start)
        save_body = runtime[save_start:save_end]

        self.assertIn("#define SQ_APP_STORE_DEVICE_CONFIG_PATH_MAX 40", app_store_h)
        self.assertNotIn("char system_dir[SQ_APP_STORE_PATH_MAX];", path_body)
        self.assertIn('"%s/system/device-config.sqdc"', path_body)
        self.assertIn("char path[SQ_APP_STORE_DEVICE_CONFIG_PATH_MAX];", load_body)
        self.assertIn("char path[SQ_APP_STORE_DEVICE_CONFIG_PATH_MAX];", save_body)
        self.assertNotIn("char path[SQ_APP_STORE_PATH_MAX];", load_body)
        self.assertNotIn("char path[SQ_APP_STORE_PATH_MAX];", save_body)

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
            "static int start_installed_app_bytes(const struct sq_device_protocol_context *context,",
            protocol_c.index("static int __noinline launch_app"),
        )
        start_body = protocol_c[
            start_definition : protocol_c.index("static int start_fallback_app", start_definition)
        ]

        self.assertIn("bool context_ready", runtime_h)
        self.assertIn("void sq_vm_runtime_reset_vm_context", runtime_h)
        self.assertIn("if (!runtime->context_ready)", dispatch_body)
        self.assertIn("sqvm_context_init_in_place", dispatch_body)
        self.assertNotIn("clear_dispatch_state(runtime);", dispatch_body)
        self.assertIn("sq_vm_runtime_reset_vm_context(context->runtime)", start_body)
        self.assertIn("set_current || context->runtime->current_app_temp ||", start_body)
        self.assertIn("strlen(context->runtime->current_app) != app_id_len", start_body)
        self.assertIn("memcmp(context->runtime->current_app, app_id, app_id_len) != 0", start_body)
        self.assertNotIn("char previous_app[SQ_APP_STORE_APP_ID_MAX];", start_body)
        self.assertIn("context->runtime->lifecycle_previous_app", start_body)
        self.assertIn("context->runtime->lifecycle_previous_app_temp", start_body)
        self.assertIn("memset(context->runtime->lifecycle_previous_app, 0,", start_body)

    def test_installed_app_start_uses_byte_slice_runtime_start(self):
        protocol_c = self.read("firmware/zephyr/src/device_protocol.c")
        lifecycle_c = self.read("firmware/zephyr/src/app_lifecycle.c")
        start_definition = protocol_c.index(
            "static int start_installed_app_bytes(const struct sq_device_protocol_context *context,",
            protocol_c.index("static int __noinline launch_app"),
        )
        start_body = protocol_c[
            start_definition : protocol_c.index("static bool is_main_app_id", start_definition)
        ]

        self.assertIn("const uint8_t *event, size_t event_len", start_body)
        self.assertNotIn("struct sq_vm_storage_backend backend;", start_body)
        self.assertIn(
            "context->runtime->job_backend = sq_app_store_vm_storage_backend(context->launch_storage)",
            start_body,
        )
        start_body_flat = " ".join(start_body.split())
        self.assertIn(
            "sq_vm_runtime_start_event(context->runtime, &context->runtime->job_backend, "
            "event, event_len)",
            start_body_flat,
        )
        self.assertNotIn("sq_vm_runtime_start(context->runtime, &backend, event)", start_body)
        self.assertNotIn("&backend", start_body)
        self.assertIn('(const uint8_t *)"app.start"', protocol_c)
        self.assertIn('sizeof("app.start") - 1, true', protocol_c)
        self.assertIn('"app.exit", false', lifecycle_c)

    def test_zephyr_wifi_station_uses_real_connect_disconnect_backend(self):
        runtime_c = self.read("firmware/zephyr/src/vm_runtime_wifi.c")

        connect_start = runtime_c.index("int32_t runtime_wifi_connect")
        disconnect_start = runtime_c.index("int32_t runtime_wifi_disconnect")
        ap_ip_start = runtime_c.index("int32_t runtime_wifi_get_ap_ip")
        connect_body = runtime_c[connect_start:disconnect_start]
        disconnect_body = runtime_c[disconnect_start:ap_ip_start]

        self.assertIn("NET_REQUEST_WIFI_CONNECT", connect_body)
        self.assertIn("NET_EVENT_WIFI_CONNECT_RESULT", runtime_c)
        self.assertIn("NET_REQUEST_WIFI_DISCONNECT", disconnect_body)
        self.assertIn("NET_EVENT_WIFI_DISCONNECT_RESULT", runtime_c)
        self.assertNotIn("runtime_wifi_unsupported_action(out)", connect_body.split("#else", 1)[0])
        self.assertNotIn("runtime_wifi_unsupported_action(out)", disconnect_body.split("#else", 1)[0])

    def test_zephyr_wifi_status_reports_station_dhcp_ip_without_fixture_leak(self):
        runtime_c = self.read("firmware/zephyr/src/vm_runtime_wifi.c")
        runtime_h = self.read("firmware/zephyr/src/vm_runtime.h")
        station_fixture = self.read("tests/hardware/c3-supermini/wifi-station-summary/main.squid")

        self.assertIn("#include <zephyr/net/dhcpv4.h>", runtime_c)
        self.assertIn("net_dhcpv4_start(iface)", runtime_c)
        self.assertIn("net_if_ipv4_get_global_addr(iface, NET_ADDR_PREFERRED)", runtime_c)
        self.assertIn("net_addr_ntop(NET_AF_INET", runtime_c)
        self.assertIn("wifi_station_ip", runtime_h)
        self.assertNotIn("status.ipAddress", station_fixture)
