from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]


class ZephyrToolingScriptTests(unittest.TestCase):
    def read(self, relative_path):
        return (ROOT / relative_path).read_text(encoding="utf-8")

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


if __name__ == "__main__":
    unittest.main()
