"""Pytest configuration for ots-push tests."""

import pytest


def pytest_configure(config):
    config.addinivalue_line(
        "markers", "asyncio: mark test as an async coroutine"
    )


@pytest.fixture(autouse=True)
def _asyncio_mode():
    """Allow async test functions to run without explicit asyncio plugin."""
    return None
