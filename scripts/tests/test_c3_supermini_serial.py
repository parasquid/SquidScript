import io
import contextlib
import unittest

from c3_supermini_serial import (
    InstallError,
    compute_fnv1a,
    decode_protocol_app_list,
    decode_protocol_frame,
    decode_protocol_hello_identity,
    encode_protocol_app_list_request,
    encode_protocol_frame,
    encode_protocol_hello_request,
    format_storage,
    get_protocol_app_list,
    get_protocol_hello_identity,
    install_app_sqbc,
    list_apps,
    parse_state,
    provision_wifi_profile,
    reference_firmware_test_sequence,
    run_app,
    run_temp_app_sqbc,
    main,
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
    def test_protocol_codec_matches_rust_golden_hello_frame(self):
        frame = encode_protocol_frame(
            kind=1,
            opcode=1,
            status=0,
            sequence=7,
            fields=[
                ("string", 1, "esp32c3-supermini"),
                ("bool", 2, True),
                ("u64", 3, 4096),
            ],
        )

        self.assertEqual(
            frame,
            bytes(
                [
                    0x53,
                    0x51,
                    0x44,
                    0x50,
                    0x01,
                    0x01,
                    0x00,
                    0x00,
                    0x07,
                    0x00,
                    0x00,
                    0x00,
                    0x26,
                    0x00,
                    0x00,
                    0x00,
                    0x43,
                    0xA5,
                    0x05,
                    0x5C,
                    0x01,
                    0x01,
                    0x11,
                    0x00,
                    0x65,
                    0x73,
                    0x70,
                    0x33,
                    0x32,
                    0x63,
                    0x33,
                    0x2D,
                    0x73,
                    0x75,
                    0x70,
                    0x65,
                    0x72,
                    0x6D,
                    0x69,
                    0x6E,
                    0x69,
                    0x02,
                    0x03,
                    0x01,
                    0x00,
                    0x01,
                    0x03,
                    0x05,
                    0x08,
                    0x00,
                    0x00,
                    0x10,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                ]
            ),
        )

    def test_protocol_decoder_rejects_payload_crc_mismatch(self):
        frame = bytearray(
            encode_protocol_frame(
                kind=2,
                opcode=69,
                status=0,
                sequence=12,
                fields=[("u64", 1, 409600), ("u64", 2, 86016)],
            )
        )
        decoded = decode_protocol_frame(bytes(frame))
        self.assertEqual(decoded["kind"], 2)
        self.assertEqual(decoded["opcode"], 69)

        frame[-1] ^= 0xFF
        with self.assertRaises(ValueError):
            decode_protocol_frame(bytes(frame))

    def test_protocol_hello_request_and_identity_response_helpers(self):
        request = encode_protocol_hello_request(sequence=9)
        decoded_request = decode_protocol_frame(request)
        self.assertEqual(decoded_request["kind"], 1)
        self.assertEqual(decoded_request["opcode"], 1)
        self.assertEqual(decoded_request["sequence"], 9)

        response = encode_protocol_frame(
            kind=2,
            opcode=1,
            status=0,
            sequence=9,
            fields=[
                ("string", 1, "esp32c3-supermini"),
                ("string", 2, "squidscript-zephyr"),
                ("bool", 3, True),
            ],
        )

        self.assertEqual(
            decode_protocol_hello_identity(response),
            {
                "target": "esp32c3-supermini",
                "firmware": "squidscript-zephyr",
                "diagnostic": True,
            },
        )

    def test_get_protocol_hello_identity_sends_framed_request(self):
        response = encode_protocol_frame(
            kind=2,
            opcode=1,
            status=0,
            sequence=1,
            fields=[
                ("string", 1, "esp32c3-supermini"),
                ("string", 2, "squidscript-zephyr"),
                ("bool", 3, True),
            ],
        )
        serial = FakeSerial([response])

        identity = get_protocol_hello_identity(serial, output=io.BytesIO(), timeout=0.01)

        self.assertEqual(identity["target"], "esp32c3-supermini")
        self.assertEqual(serial.writes, [encode_protocol_hello_request(sequence=1)])

    def test_get_protocol_hello_identity_does_not_require_output_sink(self):
        response = encode_protocol_frame(
            kind=2,
            opcode=1,
            status=0,
            sequence=1,
            fields=[
                ("string", 1, "esp32c3-supermini"),
                ("string", 2, "squidscript-zephyr"),
            ],
        )
        serial = FakeSerial([response])

        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            identity = get_protocol_hello_identity(serial, timeout=0.01)

        self.assertEqual(identity["firmware"], "squidscript-zephyr")
        self.assertEqual(output.getvalue(), "")

    def test_get_protocol_app_list_sends_framed_request_and_decodes_entries(self):
        response = encode_protocol_frame(
            kind=2,
            opcode=33,
            status=0,
            sequence=2,
            fields=[
                ("record", 1, [("string", 1, "alpha"), ("u64", 2, 5)]),
                ("record", 1, [("string", 1, "beta"), ("u64", 2, 6)]),
            ],
        )
        serial = FakeSerial([response])

        entries = get_protocol_app_list(serial, output=io.BytesIO(), timeout=0.01)

        self.assertEqual(
            entries,
            [
                {"app_id": "alpha", "sqbc_len": 5},
                {"app_id": "beta", "sqbc_len": 6},
            ],
        )
        self.assertEqual(serial.writes, [encode_protocol_app_list_request(sequence=2)])
        self.assertEqual(decode_protocol_app_list(response), entries)

    def test_cli_exposes_hello_command(self):
        output = io.StringIO()
        with self.assertRaises(SystemExit):
            with contextlib.redirect_stdout(output):
                main(["--help"])
        self.assertIn("hello", output.getvalue())

    def test_compute_fnv1a_matches_known_value(self):
        self.assertEqual(compute_fnv1a(b"hello"), 0x4F9F2CAB)

    def test_install_app_sqbc_sends_named_command(self):
        serial = FakeSerial(
            [
                encode_protocol_frame(kind=2, opcode=16, status=0, sequence=10, fields=[]),
                encode_protocol_frame(kind=2, opcode=17, status=0, sequence=11, fields=[]),
                encode_protocol_frame(kind=2, opcode=17, status=0, sequence=12, fields=[]),
                encode_protocol_frame(kind=2, opcode=18, status=0, sequence=13, fields=[]),
            ]
        )
        output = io.BytesIO()

        install_app_sqbc(serial, "main", b"hello", chunk_size=3, output=output)

        self.assertEqual(
            [decode_protocol_frame(write) for write in serial.writes],
            [
                {
                    "kind": 1,
                    "opcode": 16,
                    "status": 0,
                    "sequence": 10,
                    "fields": [
                        (1, 1, "main"),
                        (2, 5, 5),
                        (3, 5, 0x3610A686),
                    ],
                },
                {
                    "kind": 1,
                    "opcode": 17,
                    "status": 0,
                    "sequence": 11,
                    "fields": [(1, 5, 0), (2, 0, b"hel")],
                },
                {
                    "kind": 1,
                    "opcode": 17,
                    "status": 0,
                    "sequence": 12,
                    "fields": [(1, 5, 3), (2, 0, b"lo")],
                },
                {
                    "kind": 1,
                    "opcode": 18,
                    "status": 0,
                    "sequence": 13,
                    "fields": [],
                },
            ],
        )

    def test_run_app_sends_named_run_command(self):
        serial = FakeSerial(
            [encode_protocol_frame(kind=2, opcode=32, status=0, sequence=20, fields=[])]
        )

        run_app(serial, "main", output=io.BytesIO(), timeout=0.01)

        self.assertEqual(
            [decode_protocol_frame(write) for write in serial.writes],
            [
                {
                    "kind": 1,
                    "opcode": 32,
                    "status": 0,
                    "sequence": 20,
                    "fields": [(1, 1, "main")],
                }
            ],
        )

    def test_run_temp_app_sqbc_sends_named_temp_command(self):
        serial = FakeSerial(
            [
                encode_protocol_frame(kind=2, opcode=24, status=0, sequence=30, fields=[]),
                encode_protocol_frame(kind=2, opcode=25, status=0, sequence=31, fields=[]),
                encode_protocol_frame(kind=2, opcode=25, status=0, sequence=32, fields=[]),
                encode_protocol_frame(kind=2, opcode=26, status=0, sequence=33, fields=[]),
            ]
        )
        output = io.BytesIO()

        run_temp_app_sqbc(serial, "quick", b"hello", chunk_size=3, output=output)

        self.assertEqual([decode_protocol_frame(write)["opcode"] for write in serial.writes], [24, 25, 25, 26])

    def test_list_apps_requests_framed_app_registry(self):
        serial = FakeSerial(
            [
                encode_protocol_frame(
                    kind=2,
                    opcode=33,
                    status=0,
                    sequence=2,
                    fields=[("record", 1, [("string", 1, "main"), ("u64", 2, 5)])],
                )
            ]
        )
        output = io.BytesIO()

        response = list_apps(serial, output=output, timeout=0.01)

        self.assertEqual(response, [{"app_id": "main", "sqbc_len": 5}])
        self.assertEqual(output.getvalue(), b"app=main sqbc_len=5\n")
        self.assertEqual(serial.writes, [encode_protocol_app_list_request(sequence=2)])

    def test_format_storage_sends_storage_format_command(self):
        serial = FakeSerial([b"OK STORAGE.FORMAT\r\n"])

        format_storage(serial, output=io.BytesIO(), timeout=0.01)

        self.assertEqual(serial.writes, [b"STORAGE.FORMAT\n"])

    def test_provision_wifi_profile_streams_credentials_after_header(self):
        serial = FakeSerial([b"READY WIFI.PROFILE.SET profile=dev\r\n", b"OK WIFI.PROFILE.SET profile=dev\r\n"])

        provision_wifi_profile(
            serial,
            "dev",
            "ExampleSSID",
            "secret-pass",
            output=io.BytesIO(),
            timeout=0.01,
        )

        payload = b"ExampleSSIDsecret-pass"
        self.assertEqual(
            serial.writes,
            [
                f"WIFI.PROFILE.SET dev 11 11 {compute_fnv1a(payload):08x}\n".encode("ascii"),
                payload,
            ],
        )

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
