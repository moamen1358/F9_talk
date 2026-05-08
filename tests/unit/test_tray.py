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


def test_menu_has_four_top_level_items(tray):
    """Pause/Resume + Cloud-provider submenu + API Keys + Quit."""
    actions = [a for a in tray.contextMenu().actions() if not a.isSeparator()]
    assert len(actions) == 4
    assert actions[2].text() == "API Keys…"


def test_keys_menu_action_emits_keys_edit_requested(tray):
    received: list[bool] = []
    tray.keys_edit_requested.connect(lambda: received.append(True))

    actions = [a for a in tray.contextMenu().actions() if not a.isSeparator()]
    actions[2].trigger()

    assert received == [True]


def test_menu_label_flips_with_state(tray):
    actions = [a for a in tray.contextMenu().actions() if not a.isSeparator()]
    assert actions[0].text() == "Pause listening"

    tray.toggle_pause()

    assert actions[0].text() == "Resume listening"


def test_provider_submenu_has_two_choices(tray):
    actions = [a for a in tray.contextMenu().actions() if not a.isSeparator()]
    submenu = actions[1].menu()
    sub_actions = submenu.actions()
    assert sub_actions[0].text() == "Deepgram (Nova-3)"
    assert sub_actions[0].isChecked() is True
    assert sub_actions[1].text() == "Gladia (v2 live)"


def test_provider_change_emits_signal(tray):
    received: list[str] = []
    tray.provider_changed.connect(received.append)

    actions = [a for a in tray.contextMenu().actions() if not a.isSeparator()]
    submenu = actions[1].menu()
    submenu.actions()[1].trigger()

    assert received == ["gladia"]


def test_gladia_disabled_when_unavailable(qapp):
    t = DictateTray(qapp, gladia_available=False)
    actions = [a for a in t.contextMenu().actions() if not a.isSeparator()]
    submenu = actions[1].menu()
    gladia_action = submenu.actions()[1]
    assert gladia_action.isEnabled() is False
    t.hide()


def test_show_error_switches_to_error_state(tray):
    tray.show_error("Deepgram: something broke")
    assert tray._error_active is True
    assert tray.toolTip() == "F9 Talk — Last session failed"


def test_clear_error_reverts_to_active(tray):
    tray.show_error("oops")
    tray.clear_error()
    assert tray._error_active is False
    assert tray.toolTip() == "F9 Talk — Listening"


def test_clear_error_is_noop_when_not_in_error(tray):
    # Should not change tooltip or visuals
    tray.clear_error()
    assert tray._error_active is False
    assert tray.toolTip() == "F9 Talk — Listening"


def test_pause_takes_priority_over_error(tray):
    tray.show_error("boom")
    tray.toggle_pause()
    # Paused tooltip wins, even with error active
    assert tray.toolTip() == "F9 Talk — Paused"


def test_quit_action_emits_quit_requested(tray):
    received: list[bool] = []
    tray.quit_requested.connect(lambda: received.append(True))

    actions = [a for a in tray.contextMenu().actions() if not a.isSeparator()]
    quit_action = actions[3]
    quit_action.trigger()

    assert received == [True]
