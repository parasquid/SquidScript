"""CLI entry point for ots-push."""

from .client import main

if __name__ == "__main__":
    import sys

    sys.exit(main())
