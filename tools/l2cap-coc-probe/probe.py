#!/usr/bin/env python3
"""Throwaway LE L2CAP CoC probe for the BLE transport spike.

Confirms whether a Linux/BlueZ host can open an LE L2CAP connection-oriented
channel to the device and stream bytes — the data path bleak cannot drive. Pair
it with a firmware built with CONFIG_SQUIDSCRIPT_BLE_L2CAP_PROBE=y, which
registers a CoC sink on PSM 0x0025 and logs received bytes over the serial log.

Linux only: it uses the BlueZ AF_BLUETOOTH/BTPROTO_L2CAP socket API. macOS and
Windows do not expose LE CoC through a portable socket API (see README.md), so
a clean run here only proves the Linux story.

Usage:
  python3 probe.py --addr AA:BB:CC:DD:EE:FF [--psm 0x0025] [--bytes 4096]
                   [--addr-type public|random] [--mtu 256]

Exit codes: 0 = bytes sent (probe succeeded), 2 = unsupported on this host
(treat as "L2CAP CoC not usable here"), 1 = a real error talking to the device.
"""

from __future__ import annotations

import argparse
import socket
import sys
import time

# Not all Python builds expose these constants even on Linux; define fallbacks.
BTPROTO_L2CAP = getattr(socket, "BTPROTO_L2CAP", 0)
AF_BLUETOOTH = getattr(socket, "AF_BLUETOOTH", 31)

# BlueZ address types for LE.
BDADDR_LE_PUBLIC = 1
BDADDR_LE_RANDOM = 2


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(prog="l2cap-coc-probe")
    parser.add_argument("--addr", required=True, help="device BLE address AA:BB:CC:DD:EE:FF")
    parser.add_argument("--psm", default="0x0025", help="L2CAP PSM (default 0x0025, OTS)")
    parser.add_argument("--bytes", type=int, default=4096, help="payload size to stream")
    parser.add_argument("--mtu", type=int, default=256, help="per-send chunk size")
    parser.add_argument(
        "--addr-type",
        choices=("public", "random"),
        default="random",
        help="LE address type of the device (XIAO ESP32-C3 default is random)",
    )
    return parser.parse_args(argv)


def open_le_coc(addr: str, psm: int, addr_type: int) -> socket.socket:
    """Open and connect an LE L2CAP CoC socket, or raise.

    The LE connect needs the address *type* (public/random). Older Python /
    kernels only accept a 2-tuple (BR/EDR) and will fail here — that failure is
    exactly the cross-platform limitation the spike is checking for.
    """
    sock = socket.socket(AF_BLUETOOTH, socket.SOCK_SEQPACKET, BTPROTO_L2CAP)
    # 4-tuple connect (addr, psm, cid, addr_type) is the LE form BlueZ expects.
    try:
        sock.connect((addr, psm, 0, addr_type))
    except OSError:
        sock.close()
        raise
    return sock


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    psm = int(args.psm, 0)
    addr_type = BDADDR_LE_PUBLIC if args.addr_type == "public" else BDADDR_LE_RANDOM

    if sys.platform != "linux":
        print(f"OK skipped: LE L2CAP CoC socket probe is Linux-only (this is {sys.platform})")
        return 2
    if not hasattr(socket, "BTPROTO_L2CAP"):
        print("OK skipped: this Python build has no BTPROTO_L2CAP (no BlueZ socket support)")
        return 2

    try:
        sock = open_le_coc(args.addr, psm, addr_type)
    except OSError as exc:
        # ENOTSUP/EINVAL here usually means the kernel/Python can't do LE CoC.
        print(f"L2CAP CoC connect to {args.addr} PSM 0x{psm:04x} failed: {exc}")
        # Distinguish "not supported" from "device unreachable" where we can.
        return 2 if exc.errno in (95, 22) else 1

    payload = bytes((i & 0xFF) for i in range(args.bytes))
    sent = 0
    try:
        while sent < len(payload):
            chunk = payload[sent : sent + args.mtu]
            sock.send(chunk)
            sent += len(chunk)
            time.sleep(0.01)
    except OSError as exc:
        print(f"L2CAP CoC send failed after {sent} bytes: {exc}")
        sock.close()
        return 1
    sock.close()
    print(f"OK L2CAP CoC streamed {sent} bytes to {args.addr} PSM 0x{psm:04x}")
    print("Check the device serial log for matching 'L2CAP CoC recv ... (total %d)' lines." % sent)
    return 0


if __name__ == "__main__":
    sys.exit(main())
