"""Streaming microphone capture via parec (PulseAudio / PipeWire)."""
from __future__ import annotations

import logging
import shutil
import subprocess
import threading
from collections.abc import Callable

log = logging.getLogger(__name__)


class MicStreamer:
    """Stream raw 16 kHz mono s16le PCM frames from the default mic source.

    Each frame (FRAME_BYTES bytes = 25 ms) is delivered via the `on_frame`
    callback as soon as it's read. The callback runs on the parec reader
    thread — keep it lean and non-blocking.

    Designed to be pre-spawned at app start and left running indefinitely.
    The forwarding decision (record vs ignore) is the caller's responsibility.
    """

    SAMPLE_RATE = 16000
    FRAME_MS = 25
    FRAME_BYTES = SAMPLE_RATE * FRAME_MS // 1000 * 2  # 800 bytes

    def __init__(self, on_frame: Callable[[bytes], None]) -> None:
        if shutil.which("parec") is None:
            raise RuntimeError(
                "parec not found. Install pulseaudio-utils:\n"
                "  sudo apt install pulseaudio-utils"
            )
        self.on_frame = on_frame
        self._proc: subprocess.Popen[bytes] | None = None
        self._reader: threading.Thread | None = None
        self._stop = threading.Event()
        self._lock = threading.Lock()

    def start(self) -> None:
        with self._lock:
            if self._proc is not None:
                return
            self._stop.clear()
            cmd = [
                "parec",
                f"--rate={self.SAMPLE_RATE}",
                "--channels=1",
                "--format=s16le",
                "--latency-msec=100",
                "--client-name=f9-talk",
            ]
            self._proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            proc = self._proc
            self._reader = threading.Thread(
                target=self._reader_loop, args=(proc,), daemon=True, name="mic-streamer"
            )
            self._reader.start()

    def _reader_loop(self, proc: subprocess.Popen[bytes]) -> None:
        assert proc.stdout is not None
        buf = bytearray()
        try:
            while not self._stop.is_set():
                data = proc.stdout.read(2048)
                if not data:
                    return
                buf.extend(data)
                while len(buf) >= self.FRAME_BYTES:
                    try:
                        self.on_frame(bytes(buf[: self.FRAME_BYTES]))
                    except Exception as e:  # noqa: BLE001
                        log.error("on_frame failed: %s", e)
                    del buf[: self.FRAME_BYTES]
        except Exception as e:  # noqa: BLE001
            log.error("mic reader crashed: %s", e)

    def stop(self) -> None:
        self._stop.set()
        with self._lock:
            if self._proc is not None:
                try:
                    self._proc.kill()  # SIGKILL — instant teardown
                except Exception:
                    pass
                self._proc = None
        if self._reader is not None:
            self._reader.join(timeout=0.3)
            self._reader = None
