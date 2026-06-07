from pathlib import Path
import json
import re
import stat
import unittest


ROOT = Path(__file__).resolve().parents[2]


class ZephyrScriptTestCase(unittest.TestCase):
    def read(self, relative_path):
        return (ROOT / relative_path).read_text(encoding="utf-8")

    def read_json(self, relative_path):
        return json.loads(self.read(relative_path))

    def esp32c3_runtime_limits(self):
        return self.read_json("targets/runtime-limits/esp32c3-zephyr.json")

    def parse_config_assignments(self, contents):
        values = {}
        for line in contents.splitlines():
            if not line.startswith("CONFIG_") or "=" not in line:
                continue
            key, value = line.split("=", 1)
            values[key] = value.strip().strip('"')
        return values

    def parse_c_defines(self, contents):
        values = {}
        for match in re.finditer(r"^#define\s+([A-Z0-9_]+)\s+([0-9]+)u?\b", contents, re.MULTILINE):
            values[match.group(1)] = int(match.group(2))
        return values

    def assert_config_value(self, contents, key, expected):
        values = self.parse_config_assignments(contents)
        self.assertIn(key, values)
        self.assertEqual(values[key], str(expected))

    def assert_define_value(self, contents, key, expected):
        values = self.parse_c_defines(contents)
        self.assertIn(key, values)
        self.assertEqual(values[key], expected)

    def write_executable(self, path, contents):
        path.write_text(contents, encoding="utf-8")
        path.chmod(path.stat().st_mode | stat.S_IXUSR)
