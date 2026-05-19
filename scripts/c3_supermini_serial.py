#!/usr/bin/env python3
import argparse
import os
import select
import sys
import termios
import time
import tty


DEFAULT_PORT = "/dev/ttyACM0"
DEFAULT_TIMEOUT = 5.0
DEFAULT_CHUNK_SIZE = 64


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


def install_sqbc(
    serial,
    data,
    *,
    chunk_size=DEFAULT_CHUNK_SIZE,
    output=sys.stdout.buffer,
    timeout=DEFAULT_TIMEOUT,
):
    hash_value = compute_fnv1a(data)
    _drain(serial, output)
    serial.write_all(f"install {len(data)} {hash_value:08x}\n".encode("ascii"))
    _wait_for(serial, b"READY install", output, timeout, InstallError)
    for offset in range(0, len(data), chunk_size):
        serial.write_all(data[offset : offset + chunk_size])
        time.sleep(0.002)
    _wait_for(serial, b"OK install", output, timeout, InstallError)


def smoke_sequence(serial, *, output=sys.stdout.buffer, timeout=DEFAULT_TIMEOUT):
    _send_line(serial, "run", output, b"OK run", timeout)
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
        b"trace=onStart",
        b"trace=state.load",
        b"trace=state.save",
        b"trace=onKey.SELECT",
        b"trace=onKey.BACK",
        b"trace=app.exit",
    ):
        if expected not in trace:
            raise SmokeError(f"missing trace entry: {expected.decode('ascii')}")


def parse_state(data):
    values = {}
    for raw_line in data.decode("utf-8", "replace").splitlines():
        if "=" not in raw_line:
            continue
        name, value = raw_line.split("=", 1)
        if name in {"started", "count", "exited"}:
            values[name] = value
    return values


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
    parser.add_argument("--port", default=os.environ.get("ESPFLASH_PORT", DEFAULT_PORT))
    parser.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT)
    subcommands = parser.add_subparsers(dest="command", required=True)

    install = subcommands.add_parser("install", help="install SQBC bytes into RAM")
    install.add_argument("sqbc")

    subcommands.add_parser("smoke", help="run the headless counter smoke sequence")

    args = parser.parse_args(argv)
    with SerialPort(args.port) as serial:
        if args.command == "install":
            with open(args.sqbc, "rb") as handle:
                install_sqbc(serial, handle.read(), timeout=args.timeout)
        elif args.command == "smoke":
            smoke_sequence(serial, timeout=args.timeout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
