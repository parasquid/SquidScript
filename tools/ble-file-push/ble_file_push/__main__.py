"""CLI entry point for ble-file-push."""

from .client import main

if __name__ == "__main__":
    import sys

    sys.exit(main())
