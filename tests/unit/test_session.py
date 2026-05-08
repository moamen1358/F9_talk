"""Unit tests for the Deepgram streaming STT session state machine."""
from unittest.mock import MagicMock, patch

import pytest

from f9_talk.stt.deepgram import DeepgramStreamingSTT


@pytest.fixture
def stt():
    with patch("f9_talk.stt.deepgram.DeepgramClient"):
        instance = DeepgramStreamingSTT(api_key="test-key-000")
    return instance


def _make_result(transcript: str, *, is_final: bool = True):
    result = MagicMock()
    result.channel.alternatives = [MagicMock(transcript=transcript)]
    result.is_final = is_final
    return result


# ---------- begin_session ----------

def test_begin_session_sets_recording(stt):
    stt.begin_session()
    assert stt._recording is True


def test_begin_session_clears_previous_finals(stt):
    stt._session_finals = ["old transcript"]
    stt.begin_session()
    assert stt._session_finals == []


def test_begin_session_clears_event(stt):
    stt._final_arrived.set()
    stt.begin_session()
    assert not stt._final_arrived.is_set()


# ---------- _on_message ----------

def test_on_message_ignores_interim_results(stt):
    stt.begin_session()
    stt._on_message(result=_make_result("partial", is_final=False))
    assert stt._session_finals == []


def test_on_message_ignores_empty_transcript(stt):
    stt.begin_session()
    stt._on_message(result=_make_result("", is_final=True))
    assert stt._session_finals == []


def test_on_message_appends_final_during_recording(stt):
    stt.begin_session()
    stt._on_message(result=_make_result("hello there"))
    assert stt._session_finals == ["hello there"]


def test_on_message_does_not_signal_while_recording(stt):
    stt.begin_session()
    stt._on_message(result=_make_result("hello"))
    assert not stt._final_arrived.is_set()


def test_on_message_signals_event_after_recording_ends(stt):
    stt._recording = False
    stt._session_finals = []
    stt._on_message(result=_make_result("done"))
    assert stt._final_arrived.is_set()


def test_on_message_handles_none_result(stt):
    stt.begin_session()
    stt._on_message(result=None)
    assert stt._session_finals == []


def test_on_message_handles_malformed_result(stt):
    stt.begin_session()
    bad = MagicMock()
    del bad.channel
    stt._on_message(result=bad)
    assert stt._session_finals == []


# ---------- end_session ----------

def test_end_session_joins_multiple_finals(stt):
    stt._conn = MagicMock(spec=["finalize"])
    stt._session_finals = ["Hello", "world."]
    stt._recording = False
    stt._final_arrived.set()
    assert stt.end_session(max_wait_ms=50) == "Hello world."


def test_end_session_returns_empty_on_no_speech(stt):
    stt._conn = MagicMock(spec=["finalize"])
    stt._session_finals = []
    stt._recording = False
    stt._final_arrived.set()
    assert stt.end_session(max_wait_ms=50) == ""


def test_end_session_strips_result(stt):
    stt._conn = MagicMock(spec=["finalize"])
    stt._session_finals = ["  hello  ", "  world  "]
    stt._recording = False
    stt._final_arrived.set()
    result = stt.end_session(max_wait_ms=50)
    assert result == result.strip()


def test_end_session_sets_recording_false(stt):
    stt._conn = MagicMock(spec=["finalize"])
    stt.begin_session()
    stt._final_arrived.set()
    stt.end_session(max_wait_ms=50)
    assert stt._recording is False
