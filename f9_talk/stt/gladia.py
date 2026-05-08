"""Gladia v2 live transcription STT for hold-to-talk dictation.

Per-session WebSocket flow:
  1. POST /v2/live with audio config → get session_id and ws URL
  2. Connect to ws URL, send raw int16 PCM bytes as binary frames
  3. Receive transcript JSON messages, accumulate finals
  4. Send {"type": "stop_recording"} to close cleanly
"""
from __future__ import annotations

import json
import logging
import os
import threading

import requests
from websockets.exceptions import ConnectionClosed
from websockets.sync.client import connect as ws_connect

log = logging.getLogger(__name__)

_INIT_URL = "https://api.gladia.io/v2/live"


class GladiaStreamingSTT:
    """Drop-in alternative to ``DeepgramStreamingSTT`` using Gladia v2 live API.

    Same interface: ``start()``, ``stop()``, ``begin_session()``,
    ``send_audio()``, ``end_session()``.
    """

    def __init__(
        self,
        api_key: str | None = None,
        sample_rate: int = 16000,
        language: str = "en",
    ) -> None:
        self.api_key = api_key or os.environ.get("GLADIA_API_KEY")
        if not self.api_key:
            raise RuntimeError(
                "GLADIA_API_KEY not set. Sign up at https://app.gladia.io/ "
                "and add the key to ~/.config/F9_talk/secrets.env"
            )
        self.sample_rate = sample_rate
        self.language = language

        self._ws = None
        self._reader_thread: threading.Thread | None = None

        self._lock = threading.Lock()
        self._recording = False
        self._finals: list[str] = []
        self._closed_event = threading.Event()

    # ---------- lifecycle ----------

    def start(self) -> None:
        """No-op: connections are opened per session in ``begin_session()``."""

    def stop(self) -> None:
        self._teardown_ws()

    # ---------- session API ----------

    def begin_session(self) -> None:
        with self._lock:
            self._recording = True
            self._finals = []
        self._closed_event.clear()

        # 1. Create the live session
        try:
            r = requests.post(
                _INIT_URL,
                headers={"x-gladia-key": self.api_key, "Content-Type": "application/json"},
                json={
                    "encoding": "wav/pcm",
                    "sample_rate": self.sample_rate,
                    "bit_depth": 16,
                    "channels": 1,
                    "language_config": {"languages": [self.language]} if self.language else None,
                },
                timeout=5.0,
            )
            r.raise_for_status()
            session = r.json()
            ws_url = session["url"]
        except (requests.RequestException, KeyError) as e:
            log.error("Gladia session init failed: %s", e)
            return

        # 2. Open the WebSocket
        try:
            self._ws = ws_connect(ws_url, max_size=10_000_000, open_timeout=5.0)
        except Exception as e:  # noqa: BLE001
            log.error("Gladia WebSocket open failed: %s", e)
            self._ws = None
            return

        # 3. Reader thread accumulates finals
        self._reader_thread = threading.Thread(
            target=self._reader_loop, daemon=True, name="gladia-reader"
        )
        self._reader_thread.start()

    def send_audio(self, pcm: bytes) -> None:
        with self._lock:
            if not self._recording:
                return
        ws = self._ws
        if ws is None:
            return
        try:
            ws.send(pcm)
        except (ConnectionClosed, OSError) as e:
            log.debug("gladia send failed: %s", e)

    def end_session(self, max_wait_ms: int = 800) -> str:
        with self._lock:
            self._recording = False
        ws = self._ws
        if ws is not None:
            try:
                ws.send(json.dumps({"type": "stop_recording"}))
            except (ConnectionClosed, OSError) as e:
                log.debug("gladia stop_recording failed: %s", e)
        # Wait for the server to flush and close the socket.
        self._closed_event.wait(timeout=max_wait_ms / 1000.0)
        self._teardown_ws()
        with self._lock:
            return " ".join(self._finals).strip()

    # ---------- internals ----------

    def _reader_loop(self) -> None:
        ws = self._ws
        if ws is None:
            return
        try:
            for raw in ws:
                self._handle_message(raw)
        except (ConnectionClosed, OSError):
            pass
        finally:
            self._closed_event.set()

    def _handle_message(self, raw) -> None:
        if isinstance(raw, bytes):
            return  # Gladia sends JSON text frames only
        try:
            msg = json.loads(raw)
        except (TypeError, json.JSONDecodeError):
            return
        if msg.get("type") != "transcript":
            return
        data = msg.get("data") or {}
        if not data.get("is_final"):
            return
        text = (data.get("utterance") or {}).get("text", "").strip()
        if not text:
            return
        with self._lock:
            self._finals.append(text)

    def _teardown_ws(self) -> None:
        ws = self._ws
        self._ws = None
        if ws is None:
            return
        try:
            ws.close()
        except Exception as e:  # noqa: BLE001
            log.debug("gladia close failed: %s", e)
