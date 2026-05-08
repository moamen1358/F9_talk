"""Unit tests for MicStreamer."""
import threading
from unittest.mock import MagicMock, patch

import pytest

from f9_talk.audio.mic import MicStreamer


@pytest.fixture
def mock_parec():
    """Patch shutil.which so parec appears available."""
    with patch("shutil.which", return_value="/usr/bin/parec"):
        yield


def _make_proc(frames: list[bytes]) -> MagicMock:
    """Build a mock Popen whose stdout yields the given frames then EOF."""
    proc = MagicMock()
    proc.stdout.read.side_effect = frames + [b""]
    return proc


# ---------- construction ----------

def test_raises_when_parec_missing():
    with patch("shutil.which", return_value=None):
        with pytest.raises(RuntimeError, match="parec not found"):
            MicStreamer(on_frame=lambda _: None)


def test_constructs_successfully_when_parec_present(mock_parec):
    s = MicStreamer(on_frame=lambda _: None)
    assert s._proc is None
    assert s._reader is None


# ---------- start ----------

def test_start_spawns_parec_with_correct_args(mock_parec):
    with patch("subprocess.Popen") as mock_popen:
        proc = _make_proc([])
        mock_popen.return_value = proc

        s = MicStreamer(on_frame=lambda _: None)
        s.start()
        s.stop()

    cmd = mock_popen.call_args[0][0]
    assert cmd[0] == "parec"
    assert "--rate=16000" in cmd
    assert "--channels=1" in cmd
    assert "--format=s16le" in cmd


def test_start_is_idempotent(mock_parec):
    with patch("subprocess.Popen") as mock_popen:
        mock_popen.return_value = _make_proc([])

        s = MicStreamer(on_frame=lambda _: None)
        s.start()
        s.start()  # second call must be a no-op
        s.stop()

    assert mock_popen.call_count == 1


# ---------- frame delivery ----------

def test_single_frame_delivered_to_callback(mock_parec):
    received: list[bytes] = []
    frame = b"\x01\x02" * (MicStreamer.FRAME_BYTES // 2)

    with patch("subprocess.Popen") as mock_popen:
        mock_popen.return_value = _make_proc([frame])

        s = MicStreamer(on_frame=received.append)
        s.start()
        s._reader.join(timeout=1.0)  # type: ignore[union-attr]

    assert len(received) == 1
    assert len(received[0]) == MicStreamer.FRAME_BYTES


def test_two_frames_in_one_read_both_delivered(mock_parec):
    received: list[bytes] = []
    frame = b"\x00" * MicStreamer.FRAME_BYTES

    with patch("subprocess.Popen") as mock_popen:
        mock_popen.return_value = _make_proc([frame * 2])

        s = MicStreamer(on_frame=received.append)
        s.start()
        s._reader.join(timeout=1.0)  # type: ignore[union-attr]

    assert len(received) == 2


def test_partial_read_not_delivered_until_complete(mock_parec):
    received: list[bytes] = []
    half = b"\x00" * (MicStreamer.FRAME_BYTES // 2)

    with patch("subprocess.Popen") as mock_popen:
        mock_popen.return_value = _make_proc([half])  # only half a frame, then EOF

        s = MicStreamer(on_frame=received.append)
        s.start()
        s._reader.join(timeout=1.0)  # type: ignore[union-attr]

    assert received == []  # incomplete frame must not be delivered


def test_callback_exception_does_not_crash_reader(mock_parec):
    frame = b"\x00" * MicStreamer.FRAME_BYTES

    def bad_callback(_: bytes) -> None:
        raise RuntimeError("callback error")

    with patch("subprocess.Popen") as mock_popen:
        mock_popen.return_value = _make_proc([frame])

        s = MicStreamer(on_frame=bad_callback)
        s.start()
        s._reader.join(timeout=1.0)  # type: ignore[union-attr]
    # reader thread must exit cleanly without propagating the exception


# ---------- stop ----------

def test_stop_kills_parec_process(mock_parec):
    with patch("subprocess.Popen") as mock_popen:
        proc = _make_proc([])
        mock_popen.return_value = proc

        s = MicStreamer(on_frame=lambda _: None)
        s.start()
        s.stop()

    proc.kill.assert_called_once()


def test_stop_clears_proc_reference(mock_parec):
    with patch("subprocess.Popen") as mock_popen:
        mock_popen.return_value = _make_proc([])

        s = MicStreamer(on_frame=lambda _: None)
        s.start()
        s.stop()

    assert s._proc is None
