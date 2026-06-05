"""ots-push: push SQBC files to a SquidScript device over BLE Object Transfer.

This package drives the OTS GATT service and L2CAP CoC data path on a
paired SquidScript device. It uses bleak for cross-platform BLE access
and exits cleanly with a skip message when the host environment
cannot support the transfer (no adapter, no bleak, no CoC support).
"""

__version__ = "0.1.0"
