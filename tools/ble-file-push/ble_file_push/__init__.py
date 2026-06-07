"""ble-file-push: push SQBC files to a SquidScript device over BLE.

This package drives the firmware's custom GATT file-transfer service. It uses
bleak for cross-platform BLE access and exits cleanly with a skip message when
the host environment cannot support the transfer.
"""

__version__ = "0.1.0"
