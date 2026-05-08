"""Unit tests for the system-tray icon."""
from __future__ import annotations

import pytest

from f9_talk.ui.tray import DictateTray


@pytest.fixture
def tray(qapp):
    t = DictateTray(qapp)
    yield t
    t.hide()


def test_starts_active(tray):
    assert tray.is_paused() is False
    assert tray.toolTip() == "F9 Talk — Listening"


def test_toggle_flips_to_paused_and_emits(tray, qtbot=None):
    received: list[bool] = []
    tray.pause_changed.connect(received.append)

    tray.toggle_pause()

    assert tray.is_paused() is True
    assert received == [True]
    assert tray.toolTip() == "F9 Talk — Paused"


def test_toggle_back_to_active(tray):
    received: list[bool] = []
    tray.pause_changed.connect(received.append)

    tray.toggle_pause()
    tray.toggle_pause()

    assert tray.is_paused() is False
    assert received == [True, False]


def test_set_paused_idempotent(tray):
    received: list[bool] = []
    tray.pause_changed.connect(received.append)

    tray.set_paused(False)  # already False
    tray.set_paused(True)
    tray.set_paused(True)   # already True

    assert received == [True]


def test_menu_has_two_actions(tray):
    actions = [a for a in tray.contextMenu().actions() if not a.isSeparator()]
    assert len(actions) == 2


def test_menu_label_flips_with_state(tray):
    actions = [a for a in tray.contextMenu().actions() if not a.isSeparator()]
    assert actions[0].text() == "Pause listening"

    tray.toggle_pause()

    assert actions[0].text() == "Resume listening"


def test_quit_action_emits_quit_requested(tray):
    received: list[bool] = []
    tray.quit_requested.connect(lambda: received.append(True))

    actions = [a for a in tray.contextMenu().actions() if not a.isSeparator()]
    quit_action = actions[1]
    quit_action.trigger()

    assert received == [True]
