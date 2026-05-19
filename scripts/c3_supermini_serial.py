#!/usr/bin/env python3
import argparse
import glob
import os
import select
import sys
import termios
import time
import tty


DEFAULT_TIMEOUT = 5.0
DEFAULT_CHUNK_SIZE = 64


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


def install_app_sqbc(
    serial,
    app_id,
    data,
    *,
    chunk_size=DEFAULT_CHUNK_SIZE,
    output=sys.stdout.buffer,
    timeout=DEFAULT_TIMEOUT,
):
    hash_value = compute_fnv1a(data)
    _drain(serial, output)
    serial.write_all(f"INSTALL.APP {app_id} {len(data)} {hash_value:08x}\n".encode("ascii"))
    _wait_for(serial, b"READY install.app", output, timeout, InstallError)
    for offset in range(0, len(data), chunk_size):
        serial.write_all(data[offset : offset + chunk_size])
        time.sleep(0.002)
    _wait_for(serial, b"OK install.app", output, timeout, InstallError)


def run_temp_app_sqbc(
    serial,
    app_id,
    data,
    *,
    chunk_size=DEFAULT_CHUNK_SIZE,
    output=sys.stdout.buffer,
    timeout=DEFAULT_TIMEOUT,
):
    hash_value = compute_fnv1a(data)
    _drain(serial, output)
    serial.write_all(f"RUN.TEMP {app_id} {len(data)} {hash_value:08x}\n".encode("ascii"))
    _wait_for(serial, b"READY RUN.TEMP", output, timeout, InstallError)
    for offset in range(0, len(data), chunk_size):
        serial.write_all(data[offset : offset + chunk_size])
        time.sleep(0.002)
    _wait_for(serial, b"OK RUN.TEMP", output, timeout, InstallError)


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
    _send_line(serial, f"RUN.APP {app_id}", output, b"OK RUN.APP", timeout)


def get_state(serial, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    serial.write_all(b"STATE.GET\n")
    return _wait_block(serial, b"BEGIN STATE", b"END STATE", output, timeout, SmokeError)


def get_output(serial, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    serial.write_all(b"OUTPUT.GET\n")
    return _wait_block(serial, b"BEGIN OUTPUT", b"END OUTPUT", output, timeout, SmokeError)


def get_drawlog(serial, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    serial.write_all(b"DRAWLOG.GET\n")
    return _wait_block(serial, b"BEGIN DRAWLOG", b"END DRAWLOG", output, timeout, SmokeError)


def list_apps(serial, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    serial.write_all(b"APP.LIST\n")
    return _wait_block(serial, b"BEGIN APPS", b"END APPS", output, timeout, SmokeError)


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

    run_temp = subcommands.add_parser("run-temp", help="run named SQBC app bytes from RAM")
    run_temp.add_argument("app_id")
    run_temp.add_argument("sqbc")

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
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
