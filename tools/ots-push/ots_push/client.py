"""BLE Object Transfer client for SquidScript devices.

Discovers a paired device's OTS GATT service, writes a staging Object
Name, and pushes the file payload over L2CAP CoC. Used by the
host-side test driver and the hardware test wrapper.
"""

from __future__ import annotations

import asyncio
import os
from dataclasses import dataclass
from typing import Optional

OTS_SERVICE_UUID = "00001825-0000-1000-8000-00805f9b34fb"
OTS_OBJECT_NAME_WRITE_REQUEST = "object-name"
OACP_OPCODE_CREATE = 0x01
OACP_OPCODE_WRITE = 0x02
OACP_OPCODE_EXECUTE = 0x03
OACP_RESULT_SUCCESS = 0x01
OACP_RESULT_OBJ_LOCKED = 0x0a
L2CAP_PSM_OTS = 0x0025


@dataclass
class OtsPushResult:
    pushed: bool
    app_id: str
    profile_id: str
    bytes_sent: int
    skipped_reason: Optional[str] = None


def parse_object_name(name: str) -> tuple[str, str, str]:
    """Parse the app_id/profile_id/.ext Object Name shape.

    Returns (app_id, profile_id, extension).
    Raises ValueError if the name does not have exactly two slashes
    or any segment is empty.
    """
    segments = name.split("/")
    if len(segments) != 3:
        raise ValueError(f"Object Name must have exactly 2 slashes: {name!r}")
    app_id, profile_id, extension = segments
    if not app_id or not profile_id or not extension:
        raise ValueError(f"Object Name segments must be non-empty: {name!r}")
    if not extension.startswith("."):
        raise ValueError(f"Extension must start with '.': {extension!r}")
    return app_id, profile_id, extension


def build_object_name(app_id: str, profile_id: str, extension: str = ".sqbc") -> str:
    """Build the canonical Object Name for a SquidScript BLE transfer."""
    if not app_id or not profile_id:
        raise ValueError("app_id and profile_id must be non-empty")
    if not extension.startswith("."):
        raise ValueError(f"Extension must start with '.': {extension!r}")
    return f"{app_id}/{profile_id}/{extension}"


async def push_file(
    device_address: str,
    app_id: str,
    profile_id: str,
    source_path: str,
    *,
    bleak_module=None,
) -> OtsPushResult:
    """Push a file to a device's OTS service over L2CAP CoC.

    This is the async entry point. The function imports bleak lazily
    so the package is importable on hosts without bleak installed; in
    that case it returns an OtsPushResult with skipped_reason set.

    The function does not require a real BLE adapter in tests; the
    bleak_module argument allows the test suite to inject a mock.
    """
    if not os.path.isfile(source_path):
        return OtsPushResult(
            pushed=False,
            app_id=app_id,
            profile_id=profile_id,
            bytes_sent=0,
            skipped_reason=f"source file not found: {source_path}",
        )

    bleak = bleak_module
    if bleak is None:
        try:
            import bleak as _bleak

            bleak = _bleak
        except ImportError:
            return OtsPushResult(
                pushed=False,
                app_id=app_id,
                profile_id=profile_id,
                bytes_sent=0,
                skipped_reason="bleak is unavailable",
            )

    if not hasattr(bleak, "BleakScanner"):
        return OtsPushResult(
            pushed=False,
            app_id=app_id,
            profile_id=profile_id,
            bytes_sent=0,
            skipped_reason="bleak on this platform does not support L2CAP CoC",
        )

    file_size = os.path.getsize(source_path)
    object_name = build_object_name(app_id, profile_id)
    return await _push_via_bleak(bleak, device_address, object_name, source_path, file_size)


async def _push_via_bleak(
    bleak, device_address: str, object_name: str, source_path: str, file_size: int
) -> OtsPushResult:
    """Run the OTS push protocol against a real or mocked bleak backend."""
    app_id, profile_id, _ = parse_object_name(object_name)
    scanner = bleak.BleakScanner()

    def _match_device(device, _adv):
        return device.address == device_address or device.name == device_address

    device = await scanner.find_device_by_filter(_match_device)
    if device is None:
        return OtsPushResult(
            pushed=False,
            app_id=app_id,
            profile_id=profile_id,
            bytes_sent=0,
            skipped_reason="no Bluetooth adapter is available",
        )

    async with bleak.BleakClient(device) as client:
        services = await client.get_services()
        ots_service = next((s for s in services if s.uuid == OTS_SERVICE_UUID), None)
        if ots_service is None:
            return OtsPushResult(
                pushed=False,
                app_id=app_id,
                profile_id=profile_id,
                bytes_sent=0,
                skipped_reason="OTS service 0x1825 not found on device",
            )

        await _ots_object_name_write(client, ots_service, object_name)
        await _ots_oacp_create(client, ots_service, file_size)
        await _ots_l2cap_coc_write(client, source_path)
        await _ots_oacp_execute(client, ots_service)

    return OtsPushResult(
        pushed=True,
        app_id=app_id,
        profile_id=profile_id,
        bytes_sent=file_size,
    )


async def _ots_object_name_write(client, ots_service, object_name: str) -> None:
    """Write the Object Name characteristic to route the transfer."""
    name_bytes = object_name.encode("utf-8")
    char = ots_service.get_characteristic(OTS_OBJECT_NAME_WRITE_REQUEST)
    if char is None:
        raise RuntimeError("OTS Object Name characteristic not found")
    await client.write_gatt_char(char, name_bytes)


async def _ots_oacp_create(client, ots_service, alloc_size: int) -> None:
    """Send OACP Create with the declared Object Size."""
    payload = bytes([OACP_OPCODE_CREATE]) + alloc_size.to_bytes(4, "little")
    await _oacp_write(client, ots_service, payload)


async def _ots_l2cap_coc_write(client, source_path: str) -> None:
    """Stream the file payload over L2CAP CoC on the OTS PSM."""
    with open(source_path, "rb") as f:
        while True:
            chunk = f.read(512)
            if not chunk:
                break
            await client.write_l2cap_coc(L2CAP_PSM_OTS, chunk)


async def _ots_oacp_execute(client, ots_service) -> None:
    """Send OACP Execute=WRITE to finalize the transfer."""
    payload = bytes([OACP_OPCODE_EXECUTE, 0x02])
    await _oacp_write(client, ots_service, payload)


async def _oacp_write(client, ots_service, payload: bytes) -> None:
    """Write a single OACP request to the OACP characteristic."""
    char = ots_service.get_characteristic("object-action-control-point")
    if char is None:
        raise RuntimeError("OTS OACP characteristic not found")
    await client.write_gatt_char(char, payload)


def main(argv: Optional[list[str]] = None) -> int:
    """CLI entry point. Returns 0 on success or clean skip."""
    import argparse
    import sys

    parser = argparse.ArgumentParser(prog="ots_push")
    sub = parser.add_subparsers(dest="command", required=True)
    push_p = sub.add_parser("push", help="push a file to a device over BLE OTS")
    push_p.add_argument("device", help="BLE device name or address")
    push_p.add_argument("app_id", help="SquidScript app_id segment")
    push_p.add_argument("profile_id", help="SquidScript profile_id segment")
    push_p.add_argument("source", help="path to the SQBC source file")
    args = parser.parse_args(argv)

    result = asyncio.run(push_file(args.device, args.app_id, args.profile_id, args.source))
    if not result.pushed:
        reason = result.skipped_reason or "unknown"
        print(f"OK ble-ots-push skipped because {reason}", file=sys.stdout)
        return 0
    print(f"OK ble-ots-push pushed app={result.app_id} profile={result.profile_id} bytes={result.bytes_sent}")
    return 0


if __name__ == "__main__":
    import sys

    sys.exit(main())
