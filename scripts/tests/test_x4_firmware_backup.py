import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class X4FirmwareBackupTests(unittest.TestCase):
    def test_backup_script_passes_numeric_flash_size_to_espflash(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            port = tmp_path / "ttyACM0"
            port.touch()

            fake_bin = tmp_path / "bin"
            fake_bin.mkdir()
            args_log = tmp_path / "espflash-args.txt"
            fake_espflash = fake_bin / "espflash"
            fake_espflash.write_text(
                "\n".join(
                    [
                        "#!/usr/bin/env python3",
                        "import pathlib, sys",
                        f"pathlib.Path({str(args_log)!r}).write_text('\\n'.join(sys.argv[1:]))",
                        "pathlib.Path(sys.argv[-1]).write_bytes(b'backup')",
                    ]
                )
                + "\n"
            )
            fake_espflash.chmod(0o755)

            env = os.environ.copy()
            env["ESPFLASH_PORT"] = str(port)
            env["PATH"] = f"{fake_bin}:{env['PATH']}"

            subprocess.run(
                [str(ROOT / "scripts/x4-firmware-backup.sh")],
                cwd=ROOT,
                env=env,
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )

            args = args_log.read_text().splitlines()
            self.assertIn("read-flash", args)
            self.assertIn("0x1000000", args)
            self.assertNotIn("16MB", args)


if __name__ == "__main__":
    unittest.main()
