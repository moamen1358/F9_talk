"""Unit tests for DictateApp pause behavior."""
from __future__ import annotations

from unittest.mock import MagicMock, patch


def _build_app():
    """Build a DictateApp with all heavy collaborators mocked."""
    with (
        patch("f9_talk.app.DeepgramStreamingSTT"),
        patch("f9_talk.app.LocalWhisperSTT"),
        patch("f9_talk.app.MicStreamer"),
        patch("f9_talk.app.Typer"),
    ):
        from f9_talk.app import DictateApp
        indicator = MagicMock()
        return DictateApp(indicator=indicator, backend="cloud")


def test_set_paused_toggles_flag():
    app = _build_app()
    assert app._paused is False

    app.set_paused(True)
    assert app._paused is True

    app.set_paused(False)
    assert app._paused is False


def test_paused_press_does_not_begin_session():
    app = _build_app()
    app._begin_session = MagicMock()

    app.set_paused(True)
    app._on_press(MagicMock(name="f9"))

    app._begin_session.assert_not_called()


def test_unpaused_press_starts_session():
    app = _build_app()
    app._begin_session = MagicMock()

    with patch("f9_talk.app.canonical_key", return_value="f9"):
        app.cloud_hotkey = {"f9"}
        app.cloud_stt = MagicMock()
        app._on_press(MagicMock())

    app._begin_session.assert_called_once_with("cloud")
