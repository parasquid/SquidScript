"""Pytest suite for ots-push.

Uses a mock bleak backend to verify the OTS push protocol calls
the right GATT/CoC methods in the right order, without requiring a
real Bluetooth adapter. Also verifies the CLI skip patterns when
bleak is unavailable or the adapter is missing.
"""

from __future__ import annotations

import asyncio
import os
import sys
import tempfile
from unittest.mock import AsyncMock, MagicMock

import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from ots_push.client import (  # noqa: E402
    L2CAP_PSM_OTS,
    OACP_OPCODE_CREATE,
    OACP_OPCODE_EXECUTE,
    OACP_OPCODE_WRITE,
    OACP_RESULT_OBJ_LOCKED,
    OACP_RESULT_SUCCESS,
    OTS_SERVICE_UUID,
    build_object_name,
    main,
    parse_object_name,
    push_file,
)


def test_parse_object_name_valid():
    app_id, profile_id, ext = parse_object_name("break-reminder/wallpaper/.sqbc")
    assert app_id == "break-reminder"
    assert profile_id == "wallpaper"
    assert ext == ".sqbc"


def test_parse_object_name_wrong_slash_count():
    with pytest.raises(ValueError):
        parse_object_name("noslash")


def test_parse_object_name_empty_segment():
    with pytest.raises(ValueError):
        parse_object_name("/wallpaper/.sqbc")


def test_parse_object_name_extension_must_start_with_dot():
    with pytest.raises(ValueError):
        parse_object_name("app/wallpaper/sqbc")


def test_build_object_name_canonical():
    assert build_object_name("app-a", "wallpaper") == "app-a/wallpaper/.sqbc"


def test_build_object_name_rejects_empty_segments():
    with pytest.raises(ValueError):
        build_object_name("", "wallpaper")
    with pytest.raises(ValueError):
        build_object_name("app-a", "")


def test_build_object_name_rejects_bad_extension():
    with pytest.raises(ValueError):
        build_object_name("app-a", "wallpaper", "sqbc")


def test_push_file_calls_discovery_then_object_name_then_create_then_coc_then_execute():
    fake_bleak = MagicMock()
    fake_device = MagicMock()
    fake_device.address = "AA:BB:CC:DD:EE:FF"
    fake_device.name = "squid-xiao"

    fake_service = MagicMock()
    fake_service.uuid = OTS_SERVICE_UUID
    fake_name_char = MagicMock()
    fake_name_char.uuid = "object-name"
    fake_oacp_char = MagicMock()
    fake_oacp_char.uuid = "object-action-control-point"
    fake_service.get_characteristic = MagicMock(
        side_effect=lambda uuid: {"object-name": fake_name_char,
                                   "object-action-control-point": fake_oacp_char}.get(uuid)
    )
    fake_service.characteristics = [fake_name_char, fake_oacp_char]

    fake_services = [fake_service]
    fake_client = MagicMock()
    fake_client.__aenter__ = AsyncMock(return_value=fake_client)
    fake_client.__aexit__ = AsyncMock(return_value=None)
    fake_client.get_services = AsyncMock(return_value=fake_services)
    fake_client.write_gatt_char = AsyncMock()
    fake_client.write_l2cap_coc = AsyncMock()

    fake_scanner = MagicMock()
    fake_scanner.find_device_by_filter = AsyncMock(return_value=fake_device)
    fake_bleak.BleakScanner = MagicMock(return_value=fake_scanner)
    fake_bleak.BleakClient = MagicMock(return_value=fake_client)

    with tempfile.NamedTemporaryFile(delete=False, suffix=".sqbc") as f:
        f.write(b"SQBCpayload")
        path = f.name
    try:
        result = asyncio.run(push_file(
            "AA:BB:CC:DD:EE:FF", "break-reminder", "wallpaper", path, bleak_module=fake_bleak
        ))
    finally:
        os.unlink(path)

    assert result.pushed is True
    assert result.app_id == "break-reminder"
    assert result.profile_id == "wallpaper"
    assert result.bytes_sent == len(b"SQBCpayload")
    assert result.skipped_reason is None

    calls = fake_client.write_gatt_char.call_args_list
    assert len(calls) == 3, f"expected 3 write_gatt_char calls, got {len(calls)}"

    first_char, first_data = calls[0].args
    assert first_char is fake_name_char
    assert first_data == b"break-reminder/wallpaper/.sqbc"

    second_char, second_data = calls[1].args
    assert second_char is fake_oacp_char
    assert second_data[0] == OACP_OPCODE_CREATE
    assert int.from_bytes(second_data[1:5], "little") == len(b"SQBCpayload")

    third_char, third_data = calls[2].args
    assert third_char is fake_oacp_char
    assert third_data[0] == OACP_OPCODE_EXECUTE
    assert third_data[1] == 0x02

    coc_calls = fake_client.write_l2cap_coc.call_args_list
    assert len(coc_calls) >= 1
    for call in coc_calls:
        assert call.args[0] == L2CAP_PSM_OTS
        assert isinstance(call.args[1], (bytes, bytearray))


def test_push_file_skips_when_no_adapter():
    fake_bleak = MagicMock()
    fake_scanner = MagicMock()
    fake_scanner.find_device_by_filter = AsyncMock(return_value=None)
    fake_bleak.BleakScanner = MagicMock(return_value=fake_scanner)
    fake_bleak.BleakClient = MagicMock()

    with tempfile.NamedTemporaryFile(delete=False, suffix=".sqbc") as f:
        f.write(b"x")
        path = f.name
    try:
        result = asyncio.run(push_file(
            "missing-device", "app", "wallpaper", path, bleak_module=fake_bleak
        ))
    finally:
        os.unlink(path)

    assert result.pushed is False
    assert "no Bluetooth adapter" in (result.skipped_reason or "")


def test_push_file_skips_when_bleak_unavailable():
    with tempfile.NamedTemporaryFile(delete=False, suffix=".sqbc") as f:
        f.write(b"x")
        path = f.name
    try:
        result = asyncio.run(push_file("dev", "app", "wallpaper", path, bleak_module=None))
    finally:
        os.unlink(path)

    assert result.pushed is False
    assert result.skipped_reason is not None


def test_cli_returns_zero_on_clean_skip(monkeypatch, capsys):
    monkeypatch.setattr(sys, "argv", ["ots_push", "push", "dev", "app", "wallpaper", "/nonexistent"])
    exit_code = main()
    captured = capsys.readouterr()
    assert exit_code == 0
    assert "skipped" in captured.out or "skipped" in captured.err
