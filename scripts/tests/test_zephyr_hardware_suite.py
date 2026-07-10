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
    def test_xiao_sd_card_smoke_is_retained_read_only_wiring_check(self):
        script = self.read("scripts/xiao-esp32c3-test-sd-card-smoke.sh")
        source = self.read("tests/hardware/xiao-esp32c3/sd-card-smoke/src/main.c")
        overlay = self.read("tests/hardware/xiao-esp32c3/sd-card-smoke/boards/xiao_esp32c3.overlay")
        target = self.read_json("targets/xiao-esp32c3-gdeq0426t82-sd.target.json")

        self.assertIn("SD_CARD_SMOKE_READY", script)
        self.assertIn("SD_CARD_SMOKE_READY", source)
        self.assertIn("sd_init", source)
        self.assertIn("sdmmc_read_blocks", source)
        self.assertNotIn("sdmmc_write_blocks", source)
        self.assertNotIn("DISK_IOCTL_CTRL_SYNC", source)
        self.assertIn("<SPIM2_SCLK_GPIO8>", overlay)
        self.assertIn("<SPIM2_MOSI_GPIO10>", overlay)
        self.assertIn("<SPIM2_MISO_GPIO7>", overlay)
        self.assertIn("cs-gpios = <&gpio0 6 GPIO_ACTIVE_LOW>", overlay)
        self.assertIn("storage.sd.cs.unverified", target["pins"]["GPIO6"]["usedBy"])
        self.assertIn("storage.sd.miso.unverified", target["pins"]["GPIO7"]["usedBy"])
        self.assertEqual(target["buses"]["spi"]["shared"]["sck"], "GPIO8")
        self.assertEqual(target["buses"]["spi"]["shared"]["mosi"], "GPIO10")
        self.assertEqual(target["devices"]["storage.sd"]["status"], "planned-unverified")

    def test_x4_sd_card_smoke_is_standalone_read_only_wiring_check(self):
        script = self.read("scripts/xteink-x4-test-sd-card-smoke.sh")
        source = self.read("tests/hardware/xteink-x4/sd-card-smoke/src/main.c")
        overlay = self.read("tests/hardware/xteink-x4/sd-card-smoke/boards/esp32c3_devkitm.overlay")
        target = self.read_json("targets/xteink-x4.target.json")

        self.assertIn("XTEINK X4", script)
        self.assertIn("SD_CARD_SMOKE_READY", script)
        self.assertLess(
            script.index('export ZEPHYR_BOARD="${ZEPHYR_BOARD:-esp32c3_devkitm}"'),
            script.index('source "${ROOT}/scripts/zephyr-env.sh"'),
        )
        self.assertIn("SD_CARD_SMOKE_READY", source)
        self.assertIn("diagnostic-only XTEINK X4", source)
        self.assertIn("sd_init", source)
        self.assertIn("sdmmc_read_blocks", source)
        self.assertIn("SD_CARD_SMOKE_FAT", source)
        self.assertNotIn("sdmmc_write_blocks", source)
        self.assertNotIn("DISK_IOCTL_CTRL_SYNC", source)
        self.assertIn('code=%d (%s)', source)
        self.assertIn("<SPIM2_SCLK_GPIO8>", overlay)
        self.assertIn("<SPIM2_MOSI_GPIO10>", overlay)
        self.assertIn("<SPIM2_MISO_GPIO7>", overlay)
        self.assertIn("cs-gpios = <&gpio0 12 GPIO_ACTIVE_LOW>", overlay)
        self.assertNotIn("CONFIG_FAT_FILESYSTEM_ELM", self.read("tests/hardware/xteink-x4/sd-card-smoke/prj.conf"))
        self.assertEqual(target["buses"]["spi"]["shared"]["sck"], "GPIO8")
        self.assertEqual(target["buses"]["spi"]["shared"]["mosi"], "GPIO10")
        self.assertEqual(target["buses"]["spi"]["shared"]["miso"], "GPIO7")
        self.assertEqual(target["devices"]["storage.sd"]["pins"]["cs"], "GPIO12")

    def test_x4_http_binbook_upload_is_retained_hardware_flow(self):
        script = self.read("scripts/xteink-x4-test-http-binbook-upload.sh")
        docs = self.read("docs/hardware_target_tests.md")
        app = self.read("tests/hardware/xteink-x4/http-binbook-upload/main.squid")

        self.assertIn('APP_ID="http-binbook-upload-smoke"', script)
        self.assertIn('DEVICE_AP_SSID="${DEVICE_AP_SSID:-SquidScript-X4}"', script)
        self.assertIn("nmcli device wifi connect", script)
        self.assertIn("query_upload_offset", script)
        self.assertIn('INTERRUPT_UPLOAD="${INTERRUPT_UPLOAD:-0}"', script)
        self.assertIn("interrupt_upload_probe", script)
        self.assertIn("interrupted_upload_offset=", script)
        self.assertIn('local curl_args=(--max-time "${CURL_MAX_TIME_SECONDS}" -fS --http1.1', script)
        self.assertIn('curl "${curl_args[@]}" --upload-file "${BINBOOK}"', script)
        self.assertIn("http://192.168.4.1/upload/${UPLOAD_NAME}", script)
        self.assertIn('cargo run --quiet -p squidc -- app package "${APP_DIR}"', script)
        self.assertIn('cargo run --quiet -p squidc -- app install "${PACKAGE}"', script)
        self.assertIn('cargo run --quiet -p squidc -- app launch "${APP_ID}"', script)
        self.assertIn("http upload ready true null", script)
        self.assertIn('device upload "${BINBOOK}"', script)
        self.assertIn("--transport http --host 192.168.4.1", script)
        self.assertIn("upload copy true null", script)
        self.assertIn("uploaded book page", script)
        self.assertIn("device drawlog", script)
        self.assertIn("draw=binbook", script)
        self.assertIn("--host-wifi-iface", script)
        self.assertIn("service.upload.start({", app)
        self.assertIn('transports: ["http"]', app)
        self.assertIn('complete: "upload.complete"', app)
        self.assertIn('accept: [".binbook"]', app)
        self.assertIn("file.copy(ev.upload", app)
        self.assertIn("binbook.open(copied.ref)", app)
        self.assertIn("uploaded book page", app)
        self.assertIn('content.binbook.list("books"', app)
        self.assertIn("service.display.draw", app)
        self.assertIn("XTEINK X4 HTTP BinBook", docs)
        self.assertIn("xteink-x4-test-http-binbook-upload.sh", docs)
        self.assertIn("curl", docs)
        self.assertIn("INTERRUPT_UPLOAD=1", docs)
        self.assertIn("content.binbook.list", docs)

    def test_xteink_binbook_reader_script_drives_selection_and_resume(self):
        script = self.read("scripts/xteink-x4-test-binbook-reader.sh")

        self.assertIn("app package", script)
        self.assertIn("device content-put", script)
        self.assertIn("generate-test-binbook.py", script)
        self.assertIn("run_key() {", script)
        self.assertIn('timeout "${COMMAND_TIMEOUT_SECONDS}s" cargo run --quiet -p squidc -- device key', script)
        self.assertIn('grep -Fq "busy (-16)"', script)
        self.assertIn("return 0", script)
        self.assertIn("run_key library-down DOWN", script)
        self.assertIn("run_key open-selected SELECT", script)
        self.assertIn("run_key open-menu BACK", script)
        self.assertIn("device reset", script)
        self.assertIn("drawlog-reader", script)
        self.assertIn("device drawlog", script)
        self.assertIn("device errors", script)
        self.assertIn("draw=binbook", script)
        self.assertIn("mode=full", script)
        self.assertIn("drawlog-selection", script)
        self.assertIn("mode=fast1bpp", script)
        self.assertIn("proto_stack_unused_bytes", script)
        self.assertIn("vm_stack_unused_bytes", script)

    def test_xteink_grid_cursor_script_drives_cursor_and_lifecycle_reset(self):
        script = self.read("scripts/xteink-x4-test-grid-cursor.sh")
        app = self.read("examples/grid-cursor/main.squid")

        self.assertIn("app package", script)
        self.assertIn("run_key() {", script)
        self.assertIn("run_key cursor-down DOWN", script)
        self.assertIn("run_key cursor-right RIGHT", script)
        self.assertIn("run_key cursor-up UP", script)
        self.assertIn("run_key cursor-left LEFT", script)
        self.assertIn("device reset", script)
        self.assertIn("device drawlog", script)
        self.assertIn("device errors", script)
        self.assertIn("mode=fast1bpp", script)
        self.assertIn("drawlog-after-reset", script)
        self.assertIn("proto_stack_unused_bytes", script)
        self.assertIn("vm_stack_unused_bytes", script)
        self.assertNotIn("&&", app)
        self.assertIn('service.display.text("X:"', app)
        self.assertIn('service.display.text(state.cursorCol', app)
        self.assertIn('service.display.text("Y:"', app)
        self.assertIn('service.display.text(state.cursorRow', app)

    def test_generated_binbook_fixture_uses_firmware_section_layout(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "sample.binbook"
            subprocess.run(
                [sys.executable, str(ROOT / "scripts/generate-test-binbook.py"), str(out)],
                cwd=ROOT,
                check=True,
            )

            data = out.read_bytes()

        self.assertEqual(data[:8], b"BINBOOK\0")
        self.assertEqual(int.from_bytes(data[12:14], "little"), 256)
        self.assertEqual(int.from_bytes(data[36:38], "little"), 40)
        self.assertEqual(int.from_bytes(data[38:40], "little"), 16)

        section = data[256:296]
        self.assertEqual(int.from_bytes(section[0:2], "little"), 1)
        self.assertEqual(int.from_bytes(section[4:12], "little"), 896)
        self.assertGreater(int.from_bytes(section[12:20], "little"), len(b"Chapter OneChapter Two"))
        self.assertEqual(int.from_bytes(section[20:24], "little"), 0)
        self.assertEqual(int.from_bytes(section[24:28], "little"), 0)

        page_section = 256 + 12 * 40
        self.assertEqual(int.from_bytes(data[page_section:page_section + 2], "little"), 40)
        self.assertEqual(int.from_bytes(data[page_section + 20:page_section + 24], "little"), 76)
        self.assertEqual(int.from_bytes(data[page_section + 24:page_section + 28], "little"), 4)
        page_index_offset = int.from_bytes(data[page_section + 4:page_section + 12], "little")
        page_one = data[page_index_offset:page_index_offset + 76]
        page_two = data[page_index_offset + 76:page_index_offset + 152]
        self.assertEqual(int.from_bytes(page_one[24:28], "little"), 1500)
        self.assertEqual(int.from_bytes(page_one[28:32], "little"), 96000)
        self.assertEqual(int.from_bytes(page_two[16:24], "little"), 1500)
        self.assertGreater(int.from_bytes(page_two[24:28], "little"), 0)
        self.assertEqual(int.from_bytes(page_two[28:32], "little"), 96000)


    def test_x4_transfer_regression_scripts_cover_all_transports(self):
        suite = self.read("scripts/xteink-x4-test-transfer-regression.sh")
        serial = self.read("scripts/xteink-x4-test-serial-transfer.sh")
        http = self.read("scripts/xteink-x4-test-http-transfer.sh")
        ble = self.read("scripts/xteink-x4-test-ble-transfer.sh")
        app = self.read("tests/hardware/xteink-x4/file-transfer-regression/main.squid")
        cli = self.read("compiler/rust/crates/squidc-cli/src/main.rs")
        docs = self.read("docs/hardware_target_tests.md")

        self.assertIn("xteink-x4-test-serial-transfer.sh", suite)
        self.assertIn("xteink-x4-test-http-transfer.sh", suite)
        self.assertIn("xteink-x4-test-ble-transfer.sh", suite)
        self.assertLess(suite.index("xteink-x4-test-serial-transfer.sh"), suite.index("xteink-x4-test-http-transfer.sh"))
        self.assertLess(suite.index("xteink-x4-test-http-transfer.sh"), suite.index("xteink-x4-test-ble-transfer.sh"))
        self.assertIn('source "${ROOT}/scripts/lib/hardware-command.sh"', serial)
        self.assertIn("device content-put", serial)
        self.assertIn("device content-check", serial)
        self.assertIn("curl_args=(--max-time", http)
        self.assertIn("device content-check", http)
        self.assertIn("device upload", ble)
        self.assertIn("--transport ble", ble)
        self.assertIn("device content-check", ble)
        ble_verification = ble[ble.index("device upload"):ble.index("device errors")]
        self.assertLess(ble_verification.index("device upload"), ble_verification.index("device content-check"))
        self.assertNotIn('if [[ "${TARGET_BACKEND}"', ble_verification)
        self.assertIn("service.upload.start({", app)
        self.assertIn('transports: ["http", "ble"]', app)
        self.assertIn('name: ev.name', app)
        self.assertIn('name: "transfer-regression"', cli)
        self.assertIn("xteink-x4-test-transfer-regression.sh", cli)
        self.assertIn("XTEINK X4 transfer regression", docs)

    def test_xiao_epaper_gray2_smoke_is_retained_hardware_display_check(self):
        script = self.read("scripts/xiao-esp32c3-test-epaper-gray2-smoke.sh")
        docs = self.read("docs/hardware_target_tests.md")
        app = self.read("tests/hardware/xiao-esp32c3/epaper-gray2-smoke/main.squid")
        fixture = ROOT / "tests/hardware/xiao-esp32c3/epaper-gray2-smoke/books/sample.binbook"

        self.assertTrue(fixture.exists())
        self.assertGreater(fixture.stat().st_size, 1024)
        self.assertLess(fixture.stat().st_size, 16384)
        self.assertIn('binbook.open("books/sample.binbook")', app)
        self.assertIn("binbook.readPage", app)
        self.assertIn("service.display.draw", app)
        self.assertIn("gray2 pages", app)
        self.assertIn('APP_ID="epaper-gray2-smoke"', script)
        self.assertIn('APP_DIR="${ROOT}/tests/hardware/xiao-esp32c3/epaper-gray2-smoke"', script)
        self.assertIn('cargo run --quiet -p squidc -- app package "${APP_DIR}"', script)
        self.assertIn('cargo run --quiet -p squidc -- app install "${PACKAGE}"', script)
        self.assertIn("cargo run --quiet -p squidc -- app launch ${APP_ID}", script)
        self.assertIn("device drawlog", script)
        self.assertIn("device errors", script)
        self.assertIn("device resources", script)
        self.assertIn('COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-120}"', script)
        self.assertIn("lifecycle-after-launch", script)
        self.assertIn("lifecycle=active=${APP_ID}", script)
        self.assertIn("draw=binbook", script)
        self.assertIn("gray2 pages 1", script)
        self.assertIn("--skip-flash", script)
        self.assertIn("--require-camera", script)
        self.assertIn("ffmpeg", script)
        self.assertIn("XIAO e-paper GRAY2 smoke", docs)
        self.assertIn("xiao-esp32c3-test-epaper-gray2-smoke.sh", docs)
        self.assertIn("BinBook fixture", docs)
        self.assertIn("draw=binbook", docs)
        self.assertIn("black, dark gray, light gray, white", docs)
        self.assertIn("USB webcam", docs)

    def test_xiao_epaper_fast_redraw_smoke_is_retained_visual_check(self):
        script = self.read("scripts/xiao-esp32c3-test-epaper-fast-redraw-smoke.sh")
        docs = self.read("docs/hardware_target_tests.md")
        app = self.read("tests/hardware/xiao-esp32c3/epaper-fast-redraw-smoke/main.squid")
        fixture = ROOT / "tests/hardware/xiao-esp32c3/epaper-fast-redraw-smoke/books/sample.binbook"

        self.assertTrue(fixture.exists())
        self.assertGreater(fixture.stat().st_size, 16384)
        self.assertLess(fixture.stat().st_size, 131072)
        self.assertIn('binbook.open("books/sample.binbook")', app)
        self.assertIn("binbook.readPage", app)
        self.assertIn("service.display.draw", app)
        self.assertIn('event.on("key.RIGHT")', app)
        self.assertIn('event.on("key.SELECT")', app)
        self.assertIn("fast redraw page", app)
        self.assertIn('APP_ID="epaper-fast-redraw-smoke"', script)
        self.assertIn('APP_DIR="${ROOT}/tests/hardware/xiao-esp32c3/epaper-fast-redraw-smoke"', script)
        self.assertIn('cargo run --quiet -p squidc -- app package "${APP_DIR}"', script)
        self.assertIn('cargo run --quiet -p squidc -- app install "${PACKAGE}"', script)
        self.assertIn("cargo run --quiet -p squidc -- app launch ${APP_ID}", script)
        self.assertGreaterEqual(script.count("cargo run --quiet -p squidc -- device key RIGHT"), 3)
        self.assertIn("device drawlog", script)
        self.assertIn("device errors", script)
        self.assertIn("device resources", script)
        self.assertIn("draw=binbook", script)
        self.assertIn("fast redraw page 1", script)
        self.assertIn("fast redraw page 2", script)
        self.assertIn("fast redraw page 0", script)
        self.assertIn("gray bands -> chimp/image -> sharp geometry", script)
        self.assertIn("no full flash-style refresh", script)
        self.assertIn("--skip-flash", script)
        self.assertIn("--require-camera", script)
        self.assertIn("ffmpeg", script)
        self.assertIn("XIAO e-paper fast redraw smoke", docs)
        self.assertIn("xiao-esp32c3-test-epaper-fast-redraw-smoke.sh", docs)
        self.assertIn("gray bands -> chimp/image -> sharp geometry", docs)
        self.assertIn("no full flash-style refresh", docs)

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

    def test_ble_file_transfer_script_retries_post_flash_serial_setup(self):
        script = self.read("scripts/zephyr-test-ble-file-transfer.sh")

        self.assertIn("run_serial_setup", script)
        self.assertIn("wait_for_serial_output", script)
        self.assertIn("BLE_SERIAL_SETUP_ATTEMPTS", script)
        self.assertIn("BLE_SERIAL_SETUP_DELAY_SECONDS", script)
        self.assertIn("run_serial_setup storage-format", script)
        self.assertIn("run_serial_setup launch-fallback-main", script)
        self.assertIn("output=ble installer ready", script)
        self.assertIn(r"busy \(-16\)", script)
        self.assertIn("firmware did not become ready", script)

    def test_ble_installed_receiver_script_exercises_registry_slot_and_clear(self):
        script = self.read("scripts/zephyr-test-ble-installed-receiver.sh")

        self.assertIn("tests/hardware/zephyr/ble-installed-receiver/main.squid", script)
        self.assertIn("app install", script)
        self.assertIn("app launch ble-installed-receiver", script)
        self.assertIn("output=ble-installed-receiver ready", script)
        self.assertIn("app push", script)
        self.assertIn("output=ble-installed-receiver complete sqbc-install", script)
        self.assertIn("tests/hardware/zephyr/ble-route-return/main.squid", script)
        self.assertIn("Pushing return payload via BLE to installed receiver", script)
        self.assertIn("Pushing return payload again after foreground return", script)
        self.assertIn("wait_for_serial_output_count receiver-ready-returned", script)
        self.assertIn("wait_for_serial_output_count receiver-complete-again", script)
        self.assertIn("OK ble-installed-receiver", script)

    def test_target_hardware_suite_runs_installed_ble_receiver_after_fallback_install(self):
        cli = self.read("compiler/rust/crates/squidc-cli/src/main.rs")

        self.assertIn('name: "ble-installed-receiver"', cli)
        self.assertLess(
            cli.index('name: "ble-file-transfer-install"'),
            cli.index('name: "ble-installed-receiver"'),
        )
        self.assertLess(
            cli.index('name: "ble-installed-receiver"'),
            cli.index('name: "ble-reconnect"'),
        )

    def test_hardware_command_failures_capture_raw_serial_when_protocol_diagnostics_are_empty(self):
        helper = self.read("scripts/lib/hardware-command.sh")

        self.assertIn("capture_raw_serial_diagnostics", helper)
        self.assertIn("raw-serial.out", helper)
        self.assertIn("target monitor", helper)
        self.assertIn("command -v script", helper)
        self.assertIn("script -q -e -c", helper)
        self.assertIn("SQUID_CAPTURE_RAW_SERIAL_DIAGNOSTICS", helper)
        self.assertIn("protocol diagnostics were empty", helper)

    def test_ap_after_station_initial_reset_uses_bounded_recovery_diagnostics(self):
        script = self.read("scripts/zephyr-test-ap-after-station.sh")

        self.assertIn("run_reset_with_recovery reset-before-ap-after-station", script)
        self.assertIn("${label}-hello-before.out", script)
        self.assertIn("${label}-raw-serial.out", script)
        self.assertIn("firmware did not become ready", script)
        self.assertIn('"${status}" == "124"', script)
        self.assertNotIn("run_capture reset-before-ap-after-station cargo run --quiet -p squidc -- device reset", script)

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
        self.assertIn('cargo run --quiet -p squidc -- app package "${DEVICE_CONFIG_APP}"', script)
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
        self.assertRegex(app, r'indicator\s*\{\s*use "device/indicator\.sqdevice"\s*\}')
        self.assertIn('indicator.ready("device binding ready")', app)
        self.assertIn("service.indicator.write(true)", helper)
        self.assertIn("indicator.default", resource)
        self.assertIn("pinName string 5:GPIO8", resource)
        self.assertIn('cargo run --quiet -p squidc -- app package "${DEVICE_BINDING_APP}"', script)
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
        self.assertRegex(app, r'indicator\s*\{\s*use "gpio:GPIO8"\s*\}')
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

        self.assertRegex(app, r'indicator\s*\{\s*use "gpio:GPIO10"\s*\}')
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

        self.assertRegex(app, r'input\s*\{\s*use "gpio-button:GPIO9:key\.SELECT:activeLow"\s*\}')
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

        self.assertRegex(app, r'indicator\s*\{\s*use "gpio:GPIO18"\s*\}')
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
        self.assertIn("draw=clear color=0", script)
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

        self.assertIn("service.ble.file-transfer", target["features"])
        self.assertEqual(target["radios"]["ble"]["status"], "runtime-supported-reference")
        self.assertIn("BLE advertising started: ${DEVICE_NAME}", script)
        self.assertIn("bluetoothctl", script)
        self.assertIn("host scan skipped", script)
        self.assertIn("--require-host-scan", script)
        self.assertIn("host scan did not discover", script)
        self.assertNotIn("ble-smoke.conf", script)
        self.assertIn("target metadata", docs)

    def test_ble_smoke_restarts_advertising_after_disconnect(self):
        ble = self.read("firmware/zephyr/src/ble_smoke.c")

        self.assertIn("#include <zephyr/bluetooth/conn.h>", ble)
        self.assertIn("BT_CONN_CB_DEFINE", ble)
        self.assertIn("sq_ble_smoke_disconnected", ble)
        self.assertIn("K_WORK_DELAYABLE_DEFINE", ble)
        self.assertIn("k_work_schedule", ble)
        self.assertIn("sq_ble_smoke_sm_handle_disconnect", ble)
        self.assertIn("BLE advertising restarted after disconnect", ble)

    def test_ble_smoke_restart_stops_before_starting_again(self):
        ble = self.read("firmware/zephyr/src/ble_smoke.c")
        sm = self.read("firmware/zephyr/src/ble_smoke_sm.h")
        ztest_main = self.read("firmware/zephyr/tests/ble-smoke/src/main.c")
        ztest_kconfig = self.read("firmware/zephyr/tests/ble-smoke/Kconfig")
        ztest_conf = self.read("firmware/zephyr/tests/ble-smoke/prj.conf")
        script = self.read("scripts/zephyr-test-ble-smoke.sh")

        self.assertIn("BLE advertising stopped before restart", ble)
        stop_index = ble.index("BLE advertising stopped before restart")
        start_index = ble.index("BLE advertising restarted after disconnect")
        self.assertLess(stop_index, start_index)
        self.assertIn("bt_le_adv_stop", ble)
        self.assertIn("bt_le_adv_start(BT_LE_ADV_CONN_FAST_1", ble)
        self.assertIn("struct sq_ble_smoke_adv_api", sm)
        self.assertIn("SQUIDSCRIPT_BLE_SMOKE_TEST", ztest_kconfig)
        self.assertIn("CONFIG_SQUIDSCRIPT_BLE_SMOKE_TEST=y", ztest_conf)
        self.assertIn("restart_work_calls_stop_before_start", ztest_main)
        self.assertIn("native_sim/native/64", script)

    def test_ble_reconnect_script_uses_host_rescan_proof(self):
        script = self.read("scripts/zephyr-test-ble-reconnect.sh")
        docs = self.read("docs/hardware_target_tests.md")

        self.assertIn("bluetoothctl", script)
        self.assertIn("rediscover", script)
        self.assertIn("BLE_RESCAN_TIMEOUT_SECONDS", script)
        self.assertIn("BLE_RESTART_GRACE_SECONDS", script)
        self.assertIn("ESPFLASH_PORT", script)
        self.assertIn("--target", script)
        self.assertIn("--skip-flash", script)
        self.assertIn("connect \"${BLE_ADDR}\"", script)
        self.assertIn("disconnect \"${BLE_ADDR}\"", script)
        self.assertIn("rediscovered fresh advertisement", script)
        self.assertIn("scripts/zephyr-test-ble-reconnect.sh", docs)

    def test_ap_after_station_script_covers_station_teardown_path(self):
        script = self.read("scripts/zephyr-test-ap-after-station.sh")
        app = self.read("tests/hardware/zephyr/ap-after-station/main.squid")

        self.assertIn("ap1 start true", script)
        self.assertIn("ap1 stop true", script)
        self.assertIn("connect1 true null", script)
        self.assertIn("ap2 start true", script)
        self.assertIn("ap ip failed", script)
        self.assertIn("ESPFLASH_PORT", script)
        self.assertIn("--target", script)
        self.assertIn("--skip-flash", script)
        self.assertIn('service.wifi.startAP("SquidScript-AP-1")', app)
        self.assertIn('service.wifi.startAP("SquidScript-AP-2")', app)
        self.assertIn('service.wifi.connect("dev")', app)
        self.assertIn('service.wifi.disconnect()', app)
        self.assertIn("service.wifi.getAPIP()", app)

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
        station = self.read("scripts/zephyr-test-wifi-station-api.sh")
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
        self.assertIn('assert_no_unexpected_device_errors', station)
        self.assertIn(r'^error=display=unavailable code=-19( \(ENODEV\))?$', station)
        self.assertNotIn("obsolete", station.lower())
        self.assertNotIn("wifi ap", station)
        self.assertNotIn("SQUID_WIFI_STATION_PASSWORD}", station)
        self.assertNotIn("zephyr-test-wifi-station-api.sh", suite)
        self.assertNotIn("app.exit()", self.read("tests/hardware/c3-supermini/wifi-station-summary/main.squid"))

    def test_radio_concurrency_check_is_opt_in_target_aware_and_redacted(self):
        script = self.read("scripts/zephyr-test-radio-concurrency.sh")
        suite = self.read("scripts/c3-supermini-test-hardware.sh")
        cli = self.read("compiler/rust/crates/squidc-cli/src/main.rs")
        docs = self.read("docs/hardware_target_tests.md")

        self.assertIn('TARGET_ID="${TARGET_ID:-xiao-esp32c3-gdeq0426t82-sd}"', script)
        self.assertIn("target inspect --target", script)
        self.assertIn("tests/hardware/zephyr/radio-concurrency/wifi-list/main.squid", script)
        self.assertIn("tests/hardware/zephyr/radio-concurrency/wifi-station/main.squid", script)
        self.assertIn("tests/hardware/zephyr/radio-concurrency/wifi-ap/main.squid", script)
        self.assertIn("tests/hardware/zephyr/radio-concurrency/wifi-status/main.squid", script)
        self.assertIn("cleanup_radio_concurrency", script)
        self.assertIn("trap cleanup_radio_concurrency EXIT", script)
        self.assertIn("nmcli", script)
        self.assertIn("device wifi rescan", script)
        self.assertIn("connect_host_to_device_ap", script)
        self.assertIn("assert_device_ap_dhcp_lease", script)
        self.assertIn("OK host Wi-Fi received target AP DHCP lease", script)
        self.assertIn("bluetoothctl", script)
        self.assertIn("--device", script)
        self.assertIn("DEVICE_SELECTOR", script)
        self.assertIn("ble-connect-attempt", script)
        self.assertIn("ble_is_connected", script)
        self.assertIn("launch_fallback_ble_installer", script)
        self.assertIn("output=ble installer ready", script)
        self.assertIn("assert_no_raw_network_identifiers", script)
        self.assertIn("SQUID_WIFI_STATION_SSID", script)
        self.assertIn("SQUID_WIFI_STATION_PASSWORD", script)
        self.assertIn("output=radio wifi list true", script)
        self.assertIn("output=radio wifi station dev true", script)
        self.assertIn("output=radio wifi ap start true", script)
        self.assertIn("output=radio wifi ap stop true", script)
        self.assertIn("output=radio wifi status", script)
        self.assertIn("reset-before-wifi-ap", script)
        self.assertIn("reset-before-wifi-status", script)
        self.assertIn("ensure_ble_connected", script)
        self.assertIn(
            "assert_ble_connected\n"
            "check_device_errors recovery\n"
            "disconnect_ble_device\n"
            'if [[ "${REQUIRE_BLE_RECONNECT}" == "1" ]]; then',
            script,
        )
        self.assertIn(r'^error=display=unavailable code=-19( \(ENODEV\))?$', script)
        self.assertNotIn("host-scan.log", script)
        self.assertNotIn(" BSS", script)
        self.assertNotIn("zephyr-test-radio-concurrency.sh", suite)
        self.assertIn("radio concurrency", docs)
        self.assertIn("scripts/zephyr-test-radio-concurrency.sh", docs)
        self.assertIn("reset-boundary recovery", docs)
        self.assertIn("raw serial diagnostics", docs)
        self.assertIn("`radio-concurrency`\nbefore `ap-after-station`", docs)
        self.assertIn("command.arg(\"--device\").arg(device)", cli)
        self.assertNotIn("wait_for_device_ap_client_count", script)

    def test_xiao_epaper_hello_smoke_is_diagnostic_with_optional_visual_check(self):
        script = self.read("scripts/xiao-esp32c3-test-epaper-hello.sh")
        app = self.read("tests/hardware/xiao-esp32c3/epaper-hello/src/main.c")
        docs = self.read("docs/hardware_target_tests.md")

        self.assertIn("xiao-esp32c3-gdeq0426t82-sd", script)
        self.assertIn("tests/hardware/xiao-esp32c3/epaper-hello", script)
        self.assertIn("west build", script)
        self.assertIn("west flash", script)
        self.assertIn("EPAPER_HELLO_READY", script)
        self.assertIn("serial marker reached", script)
        self.assertIn("visual confirmation optional", script)
        self.assertIn("diagnostic-only", script)
        self.assertIn("HELLO WORLD", app)
        self.assertIn("SSD1677_CMD_WRITE_RAM", app)
        self.assertIn("bitbang_write_byte", app)
        self.assertIn("ROW_BYTES", app)
        self.assertNotIn("uint8_t framebuffer", app)
        self.assertIn("scripts/xiao-esp32c3-test-epaper-hello.sh", docs)
        self.assertIn("EPAPER_HELLO_READY", docs)
        self.assertIn("unattended\nsmoke-test pass criterion", docs)
        self.assertIn("Visual confirmation is optional for\nsmoke runs", docs)
        self.assertIn("serial evidence can prove controller activity", docs)
        self.assertIn("required only when the task explicitly asks", docs)
        self.assertIn("Zephyr SPI backend", docs)
        self.assertIn("service.display.clear", docs)
        self.assertIn("busy_observed=1", docs)

    def test_radio_concurrency_fixtures_are_portable_and_redacted(self):
        wifi_list = self.read("tests/hardware/zephyr/radio-concurrency/wifi-list/main.squid")
        wifi_station = self.read("tests/hardware/zephyr/radio-concurrency/wifi-station/main.squid")
        wifi_ap = self.read("tests/hardware/zephyr/radio-concurrency/wifi-ap/main.squid")
        wifi_status = self.read("tests/hardware/zephyr/radio-concurrency/wifi-status/main.squid")

        self.assertIn('app "radio-wifi-list"', wifi_list)
        self.assertIn("service.wifi.scan()", wifi_list)
        self.assertIn("service.wifi.scanNetwork(0)", wifi_list)
        self.assertIn("first.ssidLength", wifi_list)
        self.assertNotIn("first.ssid,", wifi_list)
        self.assertNotIn("app.exit()", wifi_list)

        self.assertIn('app "radio-wifi-station"', wifi_station)
        self.assertIn('service.wifi.connect("dev")', wifi_station)
        self.assertIn('service.timer.after("timer.radio.station", 250)', wifi_station)
        self.assertNotIn("timer.radio.wifi.station", wifi_station)
        self.assertIn("service.wifi.disconnect()", wifi_station)
        self.assertIn("radio wifi station", wifi_station)
        self.assertIn("radio wifi disconnect", wifi_station)
        self.assertIn("radio wifi disconnected", wifi_station)
        self.assertIn('service.timer.after("timer.radio.disc", 250)', wifi_station)
        self.assertIn("output=radio wifi disconnected dev false", self.read("scripts/zephyr-test-radio-concurrency.sh"))
        self.assertNotIn("status.ssid", wifi_station)
        self.assertNotIn("app.exit()", wifi_station)

        self.assertIn('app "radio-wifi-ap"', wifi_ap)
        self.assertIn('service.wifi.startAP("SquidScript")', wifi_ap)
        self.assertIn("service.wifi.getAPIP()", wifi_ap)
        self.assertIn("service.wifi.stopAP()", wifi_ap)
        self.assertIn("radio wifi ap start", wifi_ap)
        self.assertIn("radio wifi ap clients", wifi_ap)
        self.assertIn("radio wifi ap stop", wifi_ap)
        self.assertNotIn("ip.ip", wifi_ap)
        self.assertNotIn("app.exit()", wifi_ap)

        self.assertIn('app "radio-wifi-status"', wifi_status)
        self.assertIn("service.wifi.status()", wifi_status)
        self.assertIn("radio wifi status", wifi_status)
        self.assertNotIn("status.ssid", wifi_status)
        self.assertNotIn("app.exit()", wifi_status)

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
