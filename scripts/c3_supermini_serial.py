#!/usr/bin/env python3
import argparse
import binascii
import glob
import os
import select
import sys
import termios
import time
import tty


DEFAULT_TIMEOUT = 5.0
DEFAULT_CHUNK_SIZE = 64
PROTOCOL_MAGIC = b"SQDP"
PROTOCOL_HEADER_LEN = 20
PROTOCOL_FIELD_TYPES = {
    "bytes": 0,
    "string": 1,
    "bool": 3,
    "i64": 4,
    "u64": 5,
    "record": 32,
}
PROTOCOL_KIND_REQUEST = 1
PROTOCOL_KIND_RESPONSE = 2
PROTOCOL_STATUS_OK = 0
PROTOCOL_OPCODE_HELLO = 1
PROTOCOL_OPCODE_APP_LIST = 33
PROTOCOL_OPCODE_OUTPUT_GET = 64
PROTOCOL_HELLO_FIELD_TARGET = 1
PROTOCOL_HELLO_FIELD_FIRMWARE = 2
PROTOCOL_HELLO_FIELD_DIAGNOSTIC = 3
PROTOCOL_APP_LIST_FIELD_APP = 1
PROTOCOL_APP_FIELD_ID = 1
PROTOCOL_APP_FIELD_SQBC_LEN = 2
PROTOCOL_OUTPUT_FIELD_LINE = 1


def default_port():
    if os.environ.get("ESPFLASH_PORT"):
        return os.environ["ESPFLASH_PORT"]
    patterns = (
        "/dev/serial/by-id/*Espressif*",
        "/dev/cu.usbmodem*",
        "/dev/cu.SLAB_USBtoUART*",
        "/dev/ttyACM*",
        "/dev/ttyUSB*",
    )
    candidates = []
    for pattern in patterns:
        for path in sorted(glob.glob(pattern)):
            if path not in candidates:
                candidates.append(path)
    if len(candidates) == 1:
        return candidates[0]
    return None


class InstallError(RuntimeError):
    pass


class SmokeError(RuntimeError):
    pass


class SerialPort:
    def __init__(self, path):
        self.path = path
        self.fd = None
        self.attrs = None

    def __enter__(self):
        self.fd = os.open(self.path, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
        self.attrs = termios.tcgetattr(self.fd)
        tty.setraw(self.fd)
        return self

    def __exit__(self, _exc_type, _exc, _tb):
        if self.fd is not None:
            try:
                if self.attrs is not None:
                    termios.tcsetattr(self.fd, termios.TCSANOW, self.attrs)
            finally:
                os.close(self.fd)
                self.fd = None

    def write_all(self, data):
        view = memoryview(data)
        while view:
            _, writable, _ = select.select([], [self.fd], [], DEFAULT_TIMEOUT)
            if not writable:
                raise TimeoutError("serial write timed out")
            written = os.write(self.fd, view)
            if written == 0:
                raise TimeoutError("serial write returned zero bytes")
            view = view[written:]

    def read_available(self, timeout):
        readable, _, _ = select.select([self.fd], [], [], timeout)
        if not readable:
            return b""
        try:
            return os.read(self.fd, 4096)
        except BlockingIOError:
            return b""


def compute_fnv1a(data):
    value = 0x811C9DC5
    for byte in data:
        value ^= byte
        value = (value * 0x01000193) & 0xFFFFFFFF
    return value


def encode_protocol_frame(*, kind, opcode, status, sequence, fields):
    payload = _encode_protocol_fields(fields)
    header = bytearray(PROTOCOL_MAGIC)
    header.extend(bytes([kind, opcode, status, 0]))
    header.extend(sequence.to_bytes(4, "little"))
    header.extend(len(payload).to_bytes(4, "little"))
    header.extend((binascii.crc32(payload) & 0xFFFFFFFF).to_bytes(4, "little"))
    return bytes(header) + payload


def decode_protocol_frame(frame):
    if len(frame) < PROTOCOL_HEADER_LEN:
        raise ValueError("truncated protocol frame header")
    if frame[:4] != PROTOCOL_MAGIC:
        raise ValueError("invalid protocol frame magic")
    payload_len = int.from_bytes(frame[12:16], "little")
    expected_len = PROTOCOL_HEADER_LEN + payload_len
    if len(frame) != expected_len:
        raise ValueError(
            f"protocol frame length mismatch: expected {expected_len}, got {len(frame)}"
        )
    payload = frame[PROTOCOL_HEADER_LEN:]
    payload_crc = int.from_bytes(frame[16:20], "little")
    actual_crc = binascii.crc32(payload) & 0xFFFFFFFF
    if payload_crc != actual_crc:
        raise ValueError("protocol frame payload CRC mismatch")
    return {
        "kind": frame[4],
        "opcode": frame[5],
        "status": frame[6],
        "sequence": int.from_bytes(frame[8:12], "little"),
        "fields": _decode_protocol_fields(payload),
    }


def encode_protocol_hello_request(*, sequence=1):
    return encode_protocol_frame(
        kind=PROTOCOL_KIND_REQUEST,
        opcode=PROTOCOL_OPCODE_HELLO,
        status=PROTOCOL_STATUS_OK,
        sequence=sequence,
        fields=[],
    )


def encode_protocol_app_list_request(*, sequence=2):
    return encode_protocol_frame(
        kind=PROTOCOL_KIND_REQUEST,
        opcode=PROTOCOL_OPCODE_APP_LIST,
        status=PROTOCOL_STATUS_OK,
        sequence=sequence,
        fields=[],
    )


def encode_protocol_output_get_request(*, sequence=3):
    return encode_protocol_frame(
        kind=PROTOCOL_KIND_REQUEST,
        opcode=PROTOCOL_OPCODE_OUTPUT_GET,
        status=PROTOCOL_STATUS_OK,
        sequence=sequence,
        fields=[],
    )


def decode_protocol_hello_identity(frame):
    decoded = decode_protocol_frame(frame)
    if (
        decoded["kind"] != PROTOCOL_KIND_RESPONSE
        or decoded["opcode"] != PROTOCOL_OPCODE_HELLO
        or decoded["status"] != PROTOCOL_STATUS_OK
    ):
        raise ValueError("not a successful hello response frame")

    values = {}
    for tag, type_id, value in decoded["fields"]:
        if tag == PROTOCOL_HELLO_FIELD_TARGET and type_id == PROTOCOL_FIELD_TYPES["string"]:
            values["target"] = value
        elif tag == PROTOCOL_HELLO_FIELD_FIRMWARE and type_id == PROTOCOL_FIELD_TYPES["string"]:
            values["firmware"] = value
        elif tag == PROTOCOL_HELLO_FIELD_DIAGNOSTIC and type_id == PROTOCOL_FIELD_TYPES["bool"]:
            values["diagnostic"] = value

    if "target" not in values or "firmware" not in values:
        raise ValueError("hello response is missing identity fields")
    values.setdefault("diagnostic", False)
    return values


def decode_protocol_app_list(frame):
    decoded = decode_protocol_frame(frame)
    if (
        decoded["kind"] != PROTOCOL_KIND_RESPONSE
        or decoded["opcode"] != PROTOCOL_OPCODE_APP_LIST
        or decoded["status"] != PROTOCOL_STATUS_OK
    ):
        raise ValueError("not a successful app list response frame")

    entries = []
    for tag, type_id, value in decoded["fields"]:
        if tag != PROTOCOL_APP_LIST_FIELD_APP or type_id != PROTOCOL_FIELD_TYPES["record"]:
            continue
        entry = {}
        for field_tag, field_type_id, field_value in value:
            if field_tag == PROTOCOL_APP_FIELD_ID and field_type_id == PROTOCOL_FIELD_TYPES["string"]:
                entry["app_id"] = field_value
            elif (
                field_tag == PROTOCOL_APP_FIELD_SQBC_LEN
                and field_type_id == PROTOCOL_FIELD_TYPES["u64"]
            ):
                entry["sqbc_len"] = field_value
        if "app_id" not in entry or "sqbc_len" not in entry:
            raise ValueError("app list entry missing required fields")
        entries.append(entry)
    return entries


def decode_protocol_output(frame):
    decoded = decode_protocol_frame(frame)
    if (
        decoded["kind"] != PROTOCOL_KIND_RESPONSE
        or decoded["opcode"] != PROTOCOL_OPCODE_OUTPUT_GET
        or decoded["status"] != PROTOCOL_STATUS_OK
    ):
        raise ValueError("not a successful output response frame")

    lines = []
    for tag, type_id, value in decoded["fields"]:
        if tag == PROTOCOL_OUTPUT_FIELD_LINE and type_id == PROTOCOL_FIELD_TYPES["string"]:
            lines.append(value)
    return lines


def get_protocol_hello_identity(serial, *, output=None, timeout=DEFAULT_TIMEOUT):
    serial.write_all(encode_protocol_hello_request(sequence=1))
    response = _read_until_quiet(serial, output, timeout)
    return decode_protocol_hello_identity(response)


def get_protocol_app_list(serial, *, output=None, timeout=DEFAULT_TIMEOUT):
    serial.write_all(encode_protocol_app_list_request(sequence=2))
    response = _read_until_quiet(serial, output, timeout)
    return decode_protocol_app_list(response)


def get_protocol_output(serial, *, output=None, timeout=DEFAULT_TIMEOUT):
    serial.write_all(encode_protocol_output_get_request(sequence=3))
    response = _read_protocol_frame(serial, timeout)
    lines = decode_protocol_output(response)
    if output is not None:
        for line in lines:
            output.write(f"output={line}\n".encode("utf-8"))
        output.flush()
    return lines


def _encode_protocol_fields(fields):
    payload = bytearray()
    for field_type, tag, value in fields:
        type_id = PROTOCOL_FIELD_TYPES[field_type]
        if field_type == "bytes":
            value_bytes = bytes(value)
        elif field_type == "string":
            value_bytes = value.encode("utf-8")
        elif field_type == "bool":
            value_bytes = b"\x01" if value else b"\x00"
        elif field_type == "i64":
            value_bytes = int(value).to_bytes(8, "little", signed=True)
        elif field_type == "u64":
            value_bytes = int(value).to_bytes(8, "little", signed=False)
        elif field_type == "record":
            value_bytes = _encode_protocol_fields(value)
        else:
            raise ValueError(f"unsupported protocol field type: {field_type}")
        payload.extend(bytes([tag, type_id]))
        payload.extend(len(value_bytes).to_bytes(2, "little"))
        payload.extend(value_bytes)
    return bytes(payload)


def _decode_protocol_fields(payload):
    fields = []
    offset = 0
    while offset < len(payload):
        if len(payload) - offset < 4:
            raise ValueError("truncated protocol field header")
        tag = payload[offset]
        type_id = payload[offset + 1]
        value_len = int.from_bytes(payload[offset + 2 : offset + 4], "little")
        offset += 4
        value = payload[offset : offset + value_len]
        if len(value) != value_len:
            raise ValueError("truncated protocol field value")
        offset += value_len
        fields.append((tag, type_id, _decode_protocol_field_value(type_id, value)))
    return fields


def _decode_protocol_field_value(type_id, value):
    if type_id == PROTOCOL_FIELD_TYPES["bytes"]:
        return value
    if type_id == PROTOCOL_FIELD_TYPES["string"]:
        return value.decode("utf-8")
    if type_id == PROTOCOL_FIELD_TYPES["bool"]:
        if value == b"\x00":
            return False
        if value == b"\x01":
            return True
        raise ValueError("invalid protocol bool field")
    if type_id == PROTOCOL_FIELD_TYPES["i64"]:
        if len(value) != 8:
            raise ValueError("invalid protocol i64 field length")
        return int.from_bytes(value, "little", signed=True)
    if type_id == PROTOCOL_FIELD_TYPES["u64"]:
        if len(value) != 8:
            raise ValueError("invalid protocol u64 field length")
        return int.from_bytes(value, "little", signed=False)
    if type_id == PROTOCOL_FIELD_TYPES["record"]:
        return _decode_protocol_fields(value)
    raise ValueError(f"unknown protocol field type: {type_id}")


def install_app_sqbc(
    serial,
    app_id,
    data,
    *,
    chunk_size=DEFAULT_CHUNK_SIZE,
    output=sys.stdout.buffer,
    timeout=DEFAULT_TIMEOUT,
):
    sequence = 10
    _send_protocol_request_expect_ok(
        serial,
        encode_protocol_frame(
            kind=PROTOCOL_KIND_REQUEST,
            opcode=16,
            status=PROTOCOL_STATUS_OK,
            sequence=sequence,
            fields=[
                ("string", 1, app_id),
                ("u64", 2, len(data)),
                ("u64", 3, binascii.crc32(data) & 0xFFFFFFFF),
            ],
        ),
        opcode=16,
        sequence=sequence,
        timeout=timeout,
    )
    sequence += 1
    for offset in range(0, len(data), chunk_size):
        _send_protocol_request_expect_ok(
            serial,
            encode_protocol_frame(
                kind=PROTOCOL_KIND_REQUEST,
                opcode=17,
                status=PROTOCOL_STATUS_OK,
                sequence=sequence,
                fields=[
                    ("u64", 1, offset),
                    ("bytes", 2, data[offset : offset + chunk_size]),
                ],
            ),
            opcode=17,
            sequence=sequence,
            timeout=timeout,
        )
        sequence += 1
    _send_protocol_request_expect_ok(
        serial,
        encode_protocol_frame(
            kind=PROTOCOL_KIND_REQUEST,
            opcode=18,
            status=PROTOCOL_STATUS_OK,
            sequence=sequence,
            fields=[],
        ),
        opcode=18,
        sequence=sequence,
        timeout=timeout,
    )


def run_temp_app_sqbc(
    serial,
    app_id,
    data,
    *,
    chunk_size=DEFAULT_CHUNK_SIZE,
    output=sys.stdout.buffer,
    timeout=DEFAULT_TIMEOUT,
):
    sequence = 30
    _send_protocol_request_expect_ok(
        serial,
        encode_protocol_frame(
            kind=PROTOCOL_KIND_REQUEST,
            opcode=24,
            status=PROTOCOL_STATUS_OK,
            sequence=sequence,
            fields=[
                ("string", 1, app_id),
                ("u64", 2, len(data)),
                ("u64", 3, binascii.crc32(data) & 0xFFFFFFFF),
            ],
        ),
        opcode=24,
        sequence=sequence,
        timeout=timeout,
    )
    sequence += 1
    for offset in range(0, len(data), chunk_size):
        _send_protocol_request_expect_ok(
            serial,
            encode_protocol_frame(
                kind=PROTOCOL_KIND_REQUEST,
                opcode=25,
                status=PROTOCOL_STATUS_OK,
                sequence=sequence,
                fields=[("u64", 1, offset), ("bytes", 2, data[offset : offset + chunk_size])],
            ),
            opcode=25,
            sequence=sequence,
            timeout=timeout,
        )
        sequence += 1
    _send_protocol_request_expect_ok(
        serial,
        encode_protocol_frame(
            kind=PROTOCOL_KIND_REQUEST,
            opcode=26,
            status=PROTOCOL_STATUS_OK,
            sequence=sequence,
            fields=[],
        ),
        opcode=26,
        sequence=sequence,
        timeout=timeout,
    )


def provision_wifi_profile(
    serial,
    profile,
    ssid,
    password,
    *,
    output=sys.stdout.buffer,
    timeout=DEFAULT_TIMEOUT,
):
    ssid_bytes = ssid.encode("utf-8")
    password_bytes = password.encode("utf-8")
    payload = ssid_bytes + password_bytes
    hash_value = compute_fnv1a(payload)
    _drain(serial, output)
    serial.write_all(
        f"WIFI.PROFILE.SET {profile} {len(ssid_bytes)} {len(password_bytes)} {hash_value:08x}\n".encode(
            "ascii"
        )
    )
    _wait_for(serial, b"READY WIFI.PROFILE.SET", output, timeout, InstallError)
    serial.write_all(payload)
    _wait_for(serial, b"OK WIFI.PROFILE.SET", output, timeout, InstallError)


def reference_firmware_test_sequence(serial, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    _send_line(serial, "RUN.EVENT main app.start", output, b"OK RUN.EVENT", timeout)
    state = _state(serial, output, timeout)
    _expect_state(state, {"started": "1", "count": "0", "exited": "false"})

    _send_line(serial, "key SELECT", output, b"OK key SELECT", timeout)
    _send_line(serial, "key SELECT", output, b"OK key SELECT", timeout)
    state = _state(serial, output, timeout)
    _expect_state(state, {"started": "1", "count": "2", "exited": "false"})

    _send_line(serial, "key BACK", output, b"OK key BACK", timeout)
    state = _state(serial, output, timeout)
    _expect_state(state, {"started": "1", "count": "2", "exited": "true"})

    serial.write_all(b"trace\n")
    trace = _read_until_quiet(serial, output, timeout)
    for expected in (
        b"trace=app.start",
        b"trace=state.load",
        b"trace=state.save",
        b"trace=key.SELECT",
        b"trace=key.BACK",
        b"trace=app.exit",
    ):
        if expected not in trace:
            raise SmokeError(f"missing trace entry: {expected.decode('ascii')}")


def send_line(serial, line, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    serial.write_all(f"{line}\n".encode("ascii"))
    return _read_until_quiet(serial, output, timeout)


def run_app_event(serial, app_id, event, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    _send_line(serial, f"RUN.EVENT {app_id} {event}", output, b"OK RUN.EVENT", timeout)


def run_app(serial, app_id, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    _send_protocol_request_expect_ok(
        serial,
        encode_protocol_frame(
            kind=PROTOCOL_KIND_REQUEST,
            opcode=32,
            status=PROTOCOL_STATUS_OK,
            sequence=20,
            fields=[("string", 1, app_id)],
        ),
        opcode=32,
        sequence=20,
        timeout=timeout,
    )
    if output is not None:
        output.write(f"launched app {app_id}\n".encode("utf-8"))
        output.flush()


def get_state(serial, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    serial.write_all(b"STATE.GET\n")
    return _wait_block(serial, b"BEGIN STATE", b"END STATE", output, timeout, SmokeError)


def get_output(serial, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    return get_protocol_output(serial, output=output, timeout=timeout)


def get_drawlog(serial, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    serial.write_all(b"DRAWLOG.GET\n")
    return _wait_block(serial, b"BEGIN DRAWLOG", b"END DRAWLOG", output, timeout, SmokeError)


def list_apps(serial, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    entries = get_protocol_app_list(serial, output=None, timeout=timeout)
    if output is not None:
        for entry in entries:
            output.write(f"app={entry['app_id']} sqbc_len={entry['sqbc_len']}\n".encode("utf-8"))
        output.flush()
    return entries


def format_storage(serial, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    _send_line(serial, "STORAGE.FORMAT", output, b"OK STORAGE.FORMAT", timeout)


def import_state(serial, state_bytes, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    hash_value = compute_fnv1a(state_bytes)
    serial.write_all(f"STATE.IMPORT {len(state_bytes)} {hash_value:08x}\n".encode("ascii"))
    _wait_for(serial, b"READY STATE.IMPORT", output, timeout, SmokeError)
    serial.write_all(state_bytes)
    _wait_for(serial, b"OK STATE.IMPORT", output, timeout, SmokeError)


def parse_state(data):
    values = {}
    for raw_line in data.decode("utf-8", "replace").splitlines():
        if "=" not in raw_line:
            continue
        name, value = raw_line.split("=", 1)
        if name in {"started", "count", "exited"}:
            values[name] = value
    return values


def state_payload(data):
    lines = []
    for raw_line in data.decode("utf-8", "replace").splitlines():
        if raw_line.startswith("BEGIN ") or raw_line.startswith("END ") or raw_line.startswith("OK "):
            continue
        if raw_line.startswith("exited="):
            continue
        if "=" in raw_line:
            lines.append(raw_line)
    if not lines:
        return b""
    return ("\n".join(lines) + "\n").encode("utf-8")


def _state(serial, output, timeout):
    serial.write_all(b"state\n")
    data = _wait_for_state(serial, output, timeout)
    return parse_state(data)


def _expect_state(actual, expected):
    for name, value in expected.items():
        if actual.get(name) != value:
            raise SmokeError(f"expected {name}={value}, got {actual.get(name)!r}")


def _send_line(serial, line, output, expected, timeout):
    serial.write_all(f"{line}\n".encode("ascii"))
    _wait_for(serial, expected, output, timeout, SmokeError)


def _drain(serial, output):
    while True:
        chunk = serial.read_available(0.05)
        if not chunk:
            return
        output.write(chunk)
        output.flush()


def _send_protocol_request_expect_ok(serial, frame, *, opcode, sequence, timeout):
    serial.write_all(frame)
    response = _read_protocol_frame(serial, timeout)
    decoded = decode_protocol_frame(response)
    if (
        decoded["kind"] != PROTOCOL_KIND_RESPONSE
        or decoded["opcode"] != opcode
        or decoded["status"] != PROTOCOL_STATUS_OK
        or decoded["sequence"] != sequence
    ):
        raise InstallError(f"unexpected protocol response: {decoded}")


def _read_protocol_frame(serial, timeout):
    deadline = time.monotonic() + timeout
    response = b""
    expected_len = None
    while time.monotonic() < deadline:
        chunk = serial.read_available(0.1)
        if not chunk:
            continue
        response += chunk
        if expected_len is None and len(response) >= PROTOCOL_HEADER_LEN:
            payload_len = int.from_bytes(response[12:16], "little")
            expected_len = PROTOCOL_HEADER_LEN + payload_len
        if expected_len is not None and len(response) >= expected_len:
            return response[:expected_len]
    raise TimeoutError("timed out waiting for protocol response frame")


def _wait_for(serial, expected, output, timeout, error_type):
    deadline = time.monotonic() + timeout
    response = b""
    while time.monotonic() < deadline:
        chunk = serial.read_available(0.1)
        if chunk:
            output.write(chunk)
            output.flush()
            response += chunk
            if expected in response and _line_complete(response, expected):
                return response
            if b"ERR " in response:
                raise error_type(response.decode("utf-8", "replace").strip())
    raise error_type(f"timed out waiting for {expected.decode('ascii')}")


def _wait_for_state(serial, output, timeout):
    deadline = time.monotonic() + timeout
    response = b""
    while time.monotonic() < deadline:
        chunk = serial.read_available(0.1)
        if chunk:
            output.write(chunk)
            output.flush()
            response += chunk
            state = parse_state(response)
            if state.get("exited") in {"true", "false"} and _line_complete(
                response, b"exited="
            ):
                return response
            if b"ERR " in response:
                raise SmokeError(response.decode("utf-8", "replace").strip())
    raise SmokeError("timed out waiting for complete state")


def _wait_block(serial, begin, end, output, timeout, error_type):
    deadline = time.monotonic() + timeout
    response = b""
    while time.monotonic() < deadline:
        chunk = serial.read_available(0.1)
        if chunk:
            output.write(chunk)
            output.flush()
            response += chunk
            if begin in response and end in response and _line_complete(response, end):
                return response
            if b"ERR " in response:
                raise error_type(response.decode("utf-8", "replace").strip())
    raise error_type(f"timed out waiting for block {begin.decode('ascii')}")


def _line_complete(response, token):
    start = response.find(token)
    if start < 0:
        return False
    line_end = response.find(b"\n", start)
    return line_end >= 0


def _read_until_quiet(serial, output, timeout):
    deadline = time.monotonic() + timeout
    quiet_deadline = None
    response = b""
    while time.monotonic() < deadline:
        chunk = serial.read_available(0.1)
        if chunk:
            if output is not None:
                output.write(chunk)
                output.flush()
            response += chunk
            quiet_deadline = time.monotonic() + 0.15
        elif quiet_deadline is not None and time.monotonic() >= quiet_deadline:
            return response
    return response


def main(argv=None):
    parser = argparse.ArgumentParser(description="ESP32-C3 Super Mini SQBC serial helper")
    parser.add_argument("--port", default=default_port())
    parser.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT)
    subcommands = parser.add_subparsers(dest="command", required=True)

    install_app = subcommands.add_parser("install-app", help="install named SQBC app bytes")
    install_app.add_argument("app_id")
    install_app.add_argument("sqbc")

    run_temp = subcommands.add_parser("run-temp", help="run named SQBC app bytes as a temporary app")
    run_temp.add_argument("app_id")
    run_temp.add_argument("sqbc")

    wifi_profile = subcommands.add_parser("wifi-profile", help="provision volatile Wi-Fi profile")
    wifi_profile.add_argument("profile")
    wifi_profile.add_argument("ssid")
    wifi_profile.add_argument("password")

    subcommands.add_parser("test-reference-firmware", help="verify headless counter reference firmware behavior")
    run = subcommands.add_parser("run-event", help="run a named app event")
    run.add_argument("app_id")
    run.add_argument("event", nargs="?", default="app.start")

    run_app_parser = subcommands.add_parser("run-app", help="run a named installed app")
    run_app_parser.add_argument("app_id")

    send = subcommands.add_parser("send", help="send a raw line and print the response")
    send.add_argument("line")

    key = subcommands.add_parser("key", help="send a logical key")
    key.add_argument("key")

    subcommands.add_parser("state", help="print structured state")
    state_import = subcommands.add_parser("state-import", help="import state payload bytes")
    state_import.add_argument("state_file")
    subcommands.add_parser("output", help="print debug console output")
    subcommands.add_parser("drawlog", help="print draw log")
    subcommands.add_parser("app-list", help="print installed app registry")
    subcommands.add_parser("storage-format", help="format firmware app storage")
    subcommands.add_parser("hello", help="read framed Zephyr protocol identity")

    args = parser.parse_args(argv)
    if args.port is None:
        parser.error("no serial port found; pass --port or set ESPFLASH_PORT")
    with SerialPort(args.port) as serial:
        if args.command == "install-app":
            with open(args.sqbc, "rb") as handle:
                install_app_sqbc(serial, args.app_id, handle.read(), timeout=args.timeout)
        elif args.command == "run-temp":
            with open(args.sqbc, "rb") as handle:
                run_temp_app_sqbc(serial, args.app_id, handle.read(), timeout=args.timeout)
        elif args.command == "wifi-profile":
            provision_wifi_profile(
                serial,
                args.profile,
                args.ssid,
                args.password,
                timeout=args.timeout,
            )
        elif args.command == "test-reference-firmware":
            reference_firmware_test_sequence(serial, timeout=args.timeout)
        elif args.command == "run-event":
            run_app_event(serial, args.app_id, args.event, timeout=args.timeout)
        elif args.command == "run-app":
            run_app(serial, args.app_id, timeout=args.timeout)
        elif args.command == "send":
            send_line(serial, args.line, timeout=args.timeout)
        elif args.command == "key":
            _send_line(serial, f"KEY {args.key}", sys.stdout.buffer, b"OK key", args.timeout)
        elif args.command == "state":
            get_state(serial, timeout=args.timeout)
        elif args.command == "state-import":
            with open(args.state_file, "rb") as handle:
                import_state(serial, handle.read(), timeout=args.timeout)
        elif args.command == "output":
            get_output(serial, timeout=args.timeout)
        elif args.command == "drawlog":
            get_drawlog(serial, timeout=args.timeout)
        elif args.command == "app-list":
            list_apps(serial, timeout=args.timeout)
        elif args.command == "storage-format":
            format_storage(serial, timeout=args.timeout)
        elif args.command == "hello":
            identity = get_protocol_hello_identity(serial, timeout=args.timeout)
            print(
                "target={target} firmware={firmware} diagnostic={diagnostic}".format(
                    **identity
                )
            )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
