import io
import unittest

from c3_supermini_serial import (
    InstallError,
    compute_fnv1a,
    install_sqbc,
    parse_state,
    smoke_sequence,
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

    def test_install_sqbc_sends_command_and_payload_in_chunks(self):
        serial = FakeSerial([b"banner\r\nREADY install len=5\r\n", b"OK install hash=4f9f2cab\r\n"])
        output = io.BytesIO()

        install_sqbc(serial, b"hello", chunk_size=2, output=output)

        self.assertEqual(
            serial.writes,
            [
                b"install 5 4f9f2cab\n",
                b"he",
                b"ll",
                b"o",
            ],
        )
        self.assertIn(b"READY install", output.getvalue())
        self.assertIn(b"OK install", output.getvalue())

    def test_install_sqbc_fails_when_ok_is_missing(self):
        serial = FakeSerial([b"READY install len=5\r\n", b"ERR install timeout read=3 expected=5\r\n"])

        with self.assertRaisesRegex(InstallError, "ERR install"):
            install_sqbc(serial, b"hello", output=io.BytesIO(), timeout=0.01)

    def test_parse_state_extracts_values(self):
        state = parse_state(b"started=1\r\ncount=2\r\nexited=true\r\n")

        self.assertEqual(state, {"started": "1", "count": "2", "exited": "true"})

    def test_smoke_sequence_verifies_counter_behavior(self):
        serial = FakeSerial(
            [
                b"OK run\r\n",
                b"started=1\r\ncount=0\r\nexited=false\r\n",
                b"OK key SELECT\r\n",
                b"OK key SELECT\r\n",
                b"started=1\r\ncount=2\r\nexited=false\r\n",
                b"OK key BACK\r\n",
                b"started=1\r\ncount=2\r\nexited=true\r\n",
                b"trace=onStart\r\ntrace=state.load\r\ntrace=state.save\r\n"
                b"trace=onKey.SELECT\r\ntrace=state.save\r\n"
                b"trace=onKey.SELECT\r\ntrace=state.save\r\n"
                b"trace=onKey.BACK\r\ntrace=app.exit\r\n",
            ]
        )

        smoke_sequence(serial, output=io.BytesIO(), timeout=0.01)

        self.assertEqual(
            serial.writes,
            [
                b"run\n",
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
