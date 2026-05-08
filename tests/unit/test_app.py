"""Unit tests for DictateApp pause behavior."""
from __future__ import annotations

from unittest.mock import MagicMock, patch


def _build_app():
    """Build a DictateApp with all heavy collaborators mocked."""
    with (
        patch("f9_talk.app.DeepgramStreamingSTT"),
        patch("f9_talk.app.GladiaStreamingSTT"),
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
        app.cloud_stt_deepgram = MagicMock()
        app._on_press(MagicMock())

    app._begin_session.assert_called_once_with("cloud")


def test_set_cloud_provider_switches_active_backend():
    app = _build_app()
    app.cloud_stt_deepgram = MagicMock(name="dg")
    app.cloud_stt_gladia = MagicMock(name="gl")

    assert app.cloud_stt is app.cloud_stt_deepgram

    app.set_cloud_provider("gladia")
    assert app.cloud_stt is app.cloud_stt_gladia

    app.set_cloud_provider("deepgram")
    assert app.cloud_stt is app.cloud_stt_deepgram


def test_set_cloud_provider_rejects_unknown():
    app = _build_app()
    app.set_cloud_provider("googlestt")
    assert app._cloud_provider == "deepgram"


def test_set_cloud_provider_falls_back_when_gladia_missing():
    app = _build_app()
    app.cloud_stt_gladia = None

    app.set_cloud_provider("gladia")

    assert app._cloud_provider == "deepgram"


def test_on_error_callback_fires_when_backend_reports_error():
    received: list[str] = []
    with (
        patch("f9_talk.app.DeepgramStreamingSTT"),
        patch("f9_talk.app.GladiaStreamingSTT"),
        patch("f9_talk.app.LocalWhisperSTT"),
        patch("f9_talk.app.MicStreamer"),
        patch("f9_talk.app.Typer"),
    ):
        from f9_talk.app import DictateApp
        indicator = MagicMock()
        app = DictateApp(
            indicator=indicator,
            backend="cloud",
            on_error=received.append,
        )

    app.cloud_stt_deepgram.end_session = MagicMock(return_value="")
    app.cloud_stt_deepgram.last_error = "401 Unauthorized"
    app._record_started_at = 0.0
    with patch("f9_talk.app.time.monotonic", return_value=1.0):
        app._finish("cloud", duration=1.0)

    assert received == ["deepgram: 401 Unauthorized"]


def test_reload_keys_updates_backend_attributes(monkeypatch):
    app = _build_app()
    app.cloud_stt_deepgram = MagicMock(api_key="old-dg")
    app.cloud_stt_deepgram.__class__._ENV_KEY = "DEEPGRAM_API_KEY"
    app.cloud_stt_gladia = MagicMock(api_key="old-gl")
    app.cloud_stt_gladia.__class__._ENV_KEY = "GLADIA_API_KEY"

    monkeypatch.setenv("DEEPGRAM_API_KEY", "new-dg")
    monkeypatch.setenv("GLADIA_API_KEY", "new-gl")

    app._recording = False
    app.reload_keys()

    assert app.cloud_stt_deepgram.api_key == "new-dg"
    assert app.cloud_stt_gladia.api_key == "new-gl"


def test_reload_keys_reconnects_deepgram_when_idle(monkeypatch):
    app = _build_app()
    app.cloud_stt_deepgram = MagicMock(api_key="old")
    app.cloud_stt_deepgram.__class__._ENV_KEY = "DEEPGRAM_API_KEY"
    app.cloud_stt_gladia = None
    monkeypatch.setenv("DEEPGRAM_API_KEY", "new")

    app._recording = False
    app.reload_keys()

    app.cloud_stt_deepgram.stop.assert_called_once()
    app.cloud_stt_deepgram.start.assert_called_once()


def test_reload_keys_skips_deepgram_reconnect_mid_session(monkeypatch):
    app = _build_app()
    app.cloud_stt_deepgram = MagicMock(api_key="old")
    app.cloud_stt_deepgram.__class__._ENV_KEY = "DEEPGRAM_API_KEY"
    app.cloud_stt_gladia = None
    monkeypatch.setenv("DEEPGRAM_API_KEY", "new")

    app._recording = True  # mid-recording, don't disturb the WS
    app.reload_keys()

    app.cloud_stt_deepgram.stop.assert_not_called()
    app.cloud_stt_deepgram.start.assert_not_called()


def test_on_success_callback_fires_when_text_typed():
    received: list[bool] = []
    with (
        patch("f9_talk.app.DeepgramStreamingSTT"),
        patch("f9_talk.app.GladiaStreamingSTT"),
        patch("f9_talk.app.LocalWhisperSTT"),
        patch("f9_talk.app.MicStreamer"),
        patch("f9_talk.app.Typer"),
    ):
        from f9_talk.app import DictateApp
        indicator = MagicMock()
        app = DictateApp(
            indicator=indicator,
            backend="cloud",
            on_success=lambda: received.append(True),
        )

    app.cloud_stt_deepgram.end_session = MagicMock(return_value="hello world")
    app.cloud_stt_deepgram.last_error = None
    app._record_started_at = 0.0
    app.translator = None
    app._finish("cloud", duration=1.0)

    assert received == [True]
