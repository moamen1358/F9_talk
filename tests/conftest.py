"""Shared pytest fixtures."""
from __future__ import annotations

import os

import pytest


@pytest.fixture(scope="session")
def qapp():
    """Single QApplication for all Qt-using tests in the session."""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication
    app = QApplication.instance() or QApplication([])
    yield app
