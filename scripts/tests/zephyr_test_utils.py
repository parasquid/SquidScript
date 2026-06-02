from pathlib import Path
import stat
import unittest


ROOT = Path(__file__).resolve().parents[2]


class ZephyrScriptTestCase(unittest.TestCase):
    def read(self, relative_path):
        return (ROOT / relative_path).read_text(encoding="utf-8")

    def write_executable(self, path, contents):
        path.write_text(contents, encoding="utf-8")
        path.chmod(path.stat().st_mode | stat.S_IXUSR)
