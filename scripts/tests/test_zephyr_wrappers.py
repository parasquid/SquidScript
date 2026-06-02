from pathlib import Path
import json
import os
import re
import subprocess
import sys
import tempfile

from scripts.tests.zephyr_test_utils import ROOT, ZephyrScriptTestCase


class ZephyrWrapperTests(ZephyrScriptTestCase):
    def test_zephyr_runtime_service_family_sources_are_listed_for_firmware_and_tests(self):
        firmware_cmake = self.read("firmware/zephyr/CMakeLists.txt")
        protocol_cmake = self.read("firmware/zephyr/tests/protocol/CMakeLists.txt")
        runtime_sources = [
            "vm_runtime_app_lifecycle.c",
            "vm_runtime_device_config.c",
            "vm_runtime_display.c",
            "vm_runtime_file.c",
            "vm_runtime_indicator_gpio.c",
            "vm_runtime_system.c",
            "vm_runtime_timers.c",
            "vm_runtime_wifi.c",
        ]

        for source in runtime_sources:
            self.assertIn(f"src/{source}", firmware_cmake, source)
            self.assertIn(f"../../src/{source}", protocol_cmake, source)

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

    def test_protocol_ztests_use_generated_squid_fixtures_not_static_sqbc_blobs(self):
        ztest = self.read("firmware/zephyr/tests/protocol/src/main.c")
        cmake = self.read("firmware/zephyr/tests/protocol/CMakeLists.txt")
        docs = self.read("docs/firmware_build_architecture.md")

        self.assertNotIn("_sqbc[] = {", ztest)
        self.assertIn("squidscript_protocol_fixtures.h", ztest)
        self.assertIn("generate-zephyr-protocol-fixtures.py", cmake)
        self.assertIn("fixtures/*.squid", cmake)
        self.assertIn("Generated protocol test fixtures", docs)

    def test_build_wrapper_applies_supermini_overlay(self):
        build = self.read("scripts/c3-supermini-zephyr-build.sh")

        self.assertIn("DTC_OVERLAY_FILE", build)
        self.assertIn("esp32c3_supermini.overlay", build)
        self.assertIn("generate-zephyr-target-kconfig.py", build)
        self.assertIn("SQUID_ZEPHYR_TARGET_JSON", build)
        self.assertIn("EXTRA_CONF_FILE", build)
        self.assertIn("ZEPHYR_PRISTINE", build)
        self.assertNotIn("unverified default", build)

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
