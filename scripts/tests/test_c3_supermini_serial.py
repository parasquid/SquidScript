import io
import unittest

from c3_supermini_serial import (
    InstallError,
    compute_fnv1a,
    install_app_sqbc,
    parse_state,
    reference_firmware_test_sequence,
    run_app,
)


class FakeSerial:
    def __init__(self, reads):
        self.reads = list(reads)
        self.writes = []

    def write_all(self, data):
        self.writes.append(bytes(data))

    def read_available(self, _timeout):
        if not self.writes:
            return b""
        if self.reads:
            return self.reads.pop(0)
        return b""


class C3SuperMiniSerialTests(unittest.TestCase):
    def test_compute_fnv1a_matches_known_value(self):
        self.assertEqual(compute_fnv1a(b"hello"), 0x4F9F2CAB)

    def test_install_app_sqbc_sends_named_command(self):
        serial = FakeSerial(
            [b"READY install.app app=main len=5\r\n", b"OK install.app app=main hash=4f9f2cab\r\n"]
        )
        output = io.BytesIO()

        install_app_sqbc(serial, "main", b"hello", chunk_size=3, output=output)

        self.assertEqual(
            serial.writes,
            [
                b"INSTALL.APP main 5 4f9f2cab\n",
                b"hel",
                b"lo",
            ],
        )

    def test_run_app_sends_named_run_command(self):
        serial = FakeSerial([b"OK RUN.APP main\r\n"])

        run_app(serial, "main", output=io.BytesIO(), timeout=0.01)

        self.assertEqual(serial.writes, [b"RUN.APP main\n"])

    def test_parse_state_extracts_values(self):
        state = parse_state(b"started=1\r\ncount=2\r\nexited=true\r\n")

        self.assertEqual(state, {"started": "1", "count": "2", "exited": "true"})

    def test_reference_firmware_sequence_verifies_counter_behavior(self):
        serial = FakeSerial(
            [
                b"OK RUN.EVENT main app.start\r\n",
                b"started=1\r\ncount=0\r\nexited=false\r\n",
                b"OK key SELECT\r\n",
                b"OK key SELECT\r\n",
                b"started=1\r\ncount=2\r\nexited=false\r\n",
                b"OK key BACK\r\n",
                b"started=1\r\ncount=2\r\nexited=true\r\n",
                b"trace=app.start\r\ntrace=state.load\r\ntrace=state.save\r\n"
                b"trace=key.SELECT\r\ntrace=state.save\r\n"
                b"trace=key.SELECT\r\ntrace=state.save\r\n"
                b"trace=key.BACK\r\ntrace=app.exit\r\n",
            ]
        )

        reference_firmware_test_sequence(serial, output=io.BytesIO(), timeout=0.01)

        self.assertEqual(
            serial.writes,
            [
                b"RUN.EVENT main app.start\n",
                b"state\n",
                b"key SELECT\n",
                b"key SELECT\n",
                b"state\n",
                b"key BACK\n",
                b"state\n",
                b"trace\n",
            ],
        )


if __name__ == "__main__":
    unittest.main()
