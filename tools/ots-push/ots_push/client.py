"""BLE app-upload client for SquidScript devices (custom GATT transport).

Drives the device's custom GATT app-transfer service over plain GATT writes,
which work cross-platform through bleak (Linux/macOS/Windows) and mirror what a
Web Bluetooth page does. The object name carries only the accepted file
extension; firmware delivers the upload to the foreground app's active profile.

Protocol (matches firmware/zephyr/src/ble_app_transfer.c):
  control char  write: [0x01][size: LE u32][object-name UTF-8]  (BEGIN)
                       [0x03]                                    (ABORT)
  data char     write-without-response: raw content chunks
  status char   notify: 0x00 = complete, 0x01 = error
"""

from __future__ import annotations

import asyncio
import os
from dataclasses import dataclass
from typing import Optional

# Vendor 128-bit UUIDs, base 7e57c0de-000N-4a5b-8c6d-0123456789ab.
SVC_UUID = "7e57c0de-0001-4a5b-8c6d-0123456789ab"
CTRL_UUID = "7e57c0de-0002-4a5b-8c6d-0123456789ab"
DATA_UUID = "7e57c0de-0003-4a5b-8c6d-0123456789ab"
STAT_UUID = "7e57c0de-0004-4a5b-8c6d-0123456789ab"

OP_BEGIN = 0x01
OP_NAME = 0x02
OP_ABORT = 0x03
STATUS_COMPLETE = 0x00
STATUS_ERROR = 0x01

# Conservative GATT write chunk; raised to (MTU - 3) after connect when known.
DEFAULT_CHUNK = 180
# NAME control writes must fit the default 23-byte ATT MTU (minus ATT + opcode).
NAME_CHUNK = 18


@dataclass
class OtsPushResult:
    pushed: bool
    extension: str
    bytes_sent: int
    skipped_reason: Optional[str] = None


def parse_object_name(name: str) -> str:
    """Parse an extension-only object name; raise ValueError if malformed."""
    extension = name
    if "/" in extension:
        raise ValueError(f"Object Name must not contain route segments: {name!r}")
    if not extension:
        raise ValueError("Object Name must be non-empty")
    if not extension.startswith("."):
        raise ValueError(f"Extension must start with '.': {extension!r}")
    return extension


def build_object_name(extension: str = ".sqbc") -> str:
    if not extension.startswith("."):
        raise ValueError(f"Extension must start with '.': {extension!r}")
    return extension


def build_begin_command(size: int, name_len: int) -> bytes:
    """Frame the control BEGIN write: opcode + content size + object-name length.

    Stays at 7 bytes so it fits the default 23-byte ATT MTU (no long write).
    """
    return bytes([OP_BEGIN]) + int(size).to_bytes(4, "little") + int(name_len).to_bytes(2, "little")


async def push_file(
    device_address: str,
    source_path: str,
    *,
    bleak_module=None,
) -> OtsPushResult:
    """Upload a file to the device's custom GATT app-transfer service.

    Imports bleak lazily so the package is importable without it; returns a
    skipped result (not an error) when bleak or an adapter is unavailable.
    """
    if not os.path.isfile(source_path):
        return OtsPushResult(False, ".sqbc", 0, f"source file not found: {source_path}")

    bleak = bleak_module
    if bleak is None:
        try:
            import bleak as _bleak

            bleak = _bleak
        except ImportError:
            return OtsPushResult(False, ".sqbc", 0, "bleak is unavailable")
    if not hasattr(bleak, "BleakScanner") or not hasattr(bleak, "BleakClient"):
        return OtsPushResult(False, ".sqbc", 0, "bleak API unavailable on this host")

    object_name = build_object_name()
    file_size = os.path.getsize(source_path)
    with open(source_path, "rb") as handle:
        payload = handle.read()

    return await _push_via_gatt(bleak, device_address, object_name, payload, file_size)


async def _push_via_gatt(
    bleak, device_address: str, object_name: str, payload: bytes, file_size: int
) -> OtsPushResult:
    extension = parse_object_name(object_name)

    try:
        device = await bleak.BleakScanner.find_device_by_filter(
            lambda d, _adv: d.address == device_address or d.name == device_address
        )
    except Exception:
        device = None
    if device is None:
        return OtsPushResult(False, extension, 0, "device not found / no Bluetooth adapter")

    status: dict[str, Optional[int]] = {"code": None}
    done = asyncio.Event()

    def on_status(_handle, data: bytearray) -> None:
        status["code"] = data[0] if data else None
        done.set()

    async with bleak.BleakClient(device) as client:
        # Best-effort service presence check; bleak resolves characteristics by
        # UUID on write anyway, and .services can raise if discovery is mid-flight.
        try:
            uuids = {str(s.uuid).lower() for s in client.services}
            if uuids and SVC_UUID not in uuids:
                return OtsPushResult(False, extension, 0,
                                     f"app-transfer service {SVC_UUID} not found on device")
        except Exception:
            pass

        await client.start_notify(STAT_UUID, on_status)

        # BEGIN declares content size + object-name length (fits the default MTU).
        name_bytes = object_name.encode("utf-8")
        await client.write_gatt_char(CTRL_UUID, build_begin_command(file_size, len(name_bytes)),
                                     response=True)
        # NAME writes carry the object name in MTU-sized pieces.
        for off in range(0, len(name_bytes), NAME_CHUNK):
            await client.write_gatt_char(CTRL_UUID,
                                         bytes([OP_NAME]) + name_bytes[off : off + NAME_CHUNK],
                                         response=True)

        chunk = _resolve_chunk(client)
        sent = 0
        while sent < len(payload):
            # Acknowledged (write-with-response) writes: write-without-response is
            # the UDP analogue -- the controller silently drops chunks under load,
            # so the transfer never reaches its declared size. The data char
            # advertises both; ACKed writes trade throughput for a deterministic,
            # lossless transfer, which is the right call for small SQBC payloads.
            await client.write_gatt_char(DATA_UUID, payload[sent : sent + chunk], response=True)
            sent += chunk
        sent = min(sent, len(payload))

        # Wait for the device's completion/error notification.
        try:
            await asyncio.wait_for(done.wait(), timeout=30)
        except asyncio.TimeoutError:
            await client.write_gatt_char(CTRL_UUID, bytes([OP_ABORT]), response=True)
            return OtsPushResult(False, extension, sent, "timed out waiting for completion")
        await client.stop_notify(STAT_UUID)

    if status["code"] != STATUS_COMPLETE:
        return OtsPushResult(False, extension, sent,
                             f"device reported error status {status['code']}")
    return OtsPushResult(True, extension, file_size)


def _resolve_chunk(client) -> int:
    """Use (ATT MTU - 3) when bleak exposes it, else a safe default."""
    mtu = getattr(client, "mtu_size", None)
    if isinstance(mtu, int) and mtu > 3:
        return min(mtu - 3, 512)
    return DEFAULT_CHUNK


def main(argv: Optional[list[str]] = None) -> int:
    import argparse
    import sys

    parser = argparse.ArgumentParser(prog="ots_push")
    sub = parser.add_subparsers(dest="command", required=True)
    push_p = sub.add_parser("push", help="upload a file to a device over the GATT transport")
    push_p.add_argument("device", help="BLE device name or address")
    push_p.add_argument("source", help="path to the SQBC file to upload")
    args = parser.parse_args(argv)

    result = asyncio.run(push_file(args.device, args.source))
    if not result.pushed:
        print(f"OK ble-push skipped because {result.skipped_reason or 'unknown'}", file=sys.stdout)
        # A missing adapter/device is a skip (0); a real transfer failure is 1.
        reason = result.skipped_reason or ""
        return 0 if ("unavailable" in reason or "not found" in reason) else 1
    print(f"OK ble-push uploaded ext={result.extension} bytes={result.bytes_sent}")
    return 0


if __name__ == "__main__":
    import sys

    sys.exit(main())
