"""Unit tests for the custom-GATT BLE file-transfer client.

Uses a fake bleak backend (no real adapter) to verify the client's call
sequence: discover, BEGIN control write, chunked data writes, status notify,
completion.
"""

import asyncio
import os
import tempfile
import unittest.mock as mock

import pytest

from ble_file_push.client import (
    CTRL_UUID,
    DATA_UUID,
    OP_BEGIN,
    OP_NAME,
    STAT_UUID,
    STATUS_COMPLETE,
    STATUS_ERROR,
    SVC_UUID,
    build_begin_command,
    build_file_name,
    parse_file_name,
    push_file,
)


def test_parse_file_name_valid():
    assert parse_file_name(".sqbc") == ".sqbc"


def test_parse_file_name_rejects_routed_segments():
    with pytest.raises(ValueError):
        parse_file_name("break-reminder/wallpaper/.sqbc")


def test_parse_file_name_empty():
    with pytest.raises(ValueError):
        parse_file_name("")


def test_parse_file_name_extension_must_start_with_dot():
    with pytest.raises(ValueError):
        parse_file_name("sqbc")


def test_build_file_name_canonical():
    assert build_file_name() == ".sqbc"


def test_build_begin_command_frames_opcode_size_namelen():
    cmd = build_begin_command(0x01020304, 30)
    assert cmd[0] == OP_BEGIN
    assert cmd[1:5] == bytes([0x04, 0x03, 0x02, 0x01])  # size, little-endian
    assert cmd[5:7] == bytes([30, 0x00])                # name_len, little-endian
    assert len(cmd) == 7


class _FakeService:
    def __init__(self, uuid):
        self.uuid = uuid


class _FakeClient:
    """Async-context-manager stand-in for bleak.BleakClient."""

    def __init__(self, *, status=STATUS_COMPLETE, has_service=True):
        self._status = status
        self.services = [_FakeService(SVC_UUID if has_service else "00000000-0000-0000-0000-000000000000")]
        self.mtu_size = 247
        self.writes = []
        self.notify_started = False
        self.notify_stopped = False

    async def __aenter__(self):
        return self

    async def __aexit__(self, *exc):
        return False

    async def start_notify(self, uuid, callback):
        assert uuid == STAT_UUID
        self.notify_started = True
        callback(0, bytearray([self._status]))  # simulate device notification

    async def stop_notify(self, uuid):
        self.notify_stopped = True

    async def write_gatt_char(self, uuid, data, response=True):
        self.writes.append((uuid, bytes(data), response))


def _fake_bleak(client):
    fake = mock.MagicMock()
    fake.BleakScanner.find_device_by_filter = mock.AsyncMock(return_value=mock.MagicMock())
    fake.BleakClient = mock.MagicMock(return_value=client)
    return fake


def _write_sqbc(tmpdir, payload):
    path = os.path.join(tmpdir, "app.sqbc")
    with open(path, "wb") as handle:
        handle.write(payload)
    return path


def test_push_happy_path_writes_begin_then_chunks():
    payload = b"SQBC" + bytes(range(256)) * 2  # larger than one chunk
    client = _FakeClient()
    with tempfile.TemporaryDirectory() as tmp:
        src = _write_sqbc(tmp, payload)
        result = asyncio.run(
            push_file("AA:BB:CC:DD:EE:FF", src, bleak_module=_fake_bleak(client))
        )

    assert result.pushed is True
    assert result.bytes_sent == len(payload)
    assert client.notify_started and client.notify_stopped

    name = b".sqbc"
    ctrl_writes = [w for w in client.writes if w[0] == CTRL_UUID]
    data_writes = [w for w in client.writes if w[0] == DATA_UUID]

    begin = next(w[1] for w in ctrl_writes if w[1][0] == OP_BEGIN)
    assert int.from_bytes(begin[1:5], "little") == len(payload)
    assert int.from_bytes(begin[5:7], "little") == len(name)
    assert len(begin) == 7  # fits the default 23-byte ATT MTU

    name_writes = [w[1] for w in ctrl_writes if w[1][0] == OP_NAME]
    assert b"".join(w[1:] for w in name_writes) == name
    assert all(len(w) <= 23 for w in name_writes)  # each NAME write fits the MTU

    assert data_writes, "expected chunked data writes"
    assert sum(len(w[1]) for w in data_writes) == len(payload)
    assert all(w[2] is True for w in data_writes)  # acknowledged writes


def test_push_reports_device_error_status():
    client = _FakeClient(status=STATUS_ERROR)
    with tempfile.TemporaryDirectory() as tmp:
        src = _write_sqbc(tmp, b"SQBCxxxx")
        result = asyncio.run(push_file("dev", src, bleak_module=_fake_bleak(client)))
    assert result.pushed is False
    assert "error status" in (result.skipped_reason or "")


def test_push_skips_when_service_absent():
    client = _FakeClient(has_service=False)
    with tempfile.TemporaryDirectory() as tmp:
        src = _write_sqbc(tmp, b"SQBCxxxx")
        result = asyncio.run(push_file("dev", src, bleak_module=_fake_bleak(client)))
    assert result.pushed is False
    assert "service" in (result.skipped_reason or "")


def test_push_missing_source_file():
    result = asyncio.run(push_file("dev", "/nope/missing.sqbc", bleak_module=object()))
    assert result.pushed is False
    assert "source file not found" in (result.skipped_reason or "")
