"""Shared pytest fixtures.

Sets ``QT_QPA_PLATFORM=offscreen`` at module load (before any test imports
PySide6) so headless CI runners can construct widgets without a display.
"""
from __future__ import annotations

import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import pytest  # noqa: E402


@pytest.fixture(scope="session")
def qapp():
    """Single QApplication for all Qt-using tests in the session."""
    from PySide6.QtWidgets import QApplication
    app = QApplication.instance() or QApplication([])
    yield app
