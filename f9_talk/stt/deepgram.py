"""Deepgram Nova-2 streaming STT for hold-to-talk dictation."""
from __future__ import annotations

import logging
import os
import threading
import time

from deepgram import (
    DeepgramClient,
    DeepgramClientOptions,
    LiveOptions,
    LiveTranscriptionEvents,
)

log = logging.getLogger(__name__)


class DeepgramStreamingSTT:
    """Persistent Deepgram WebSocket for hold-to-talk dictation.

    The connection opens once at app start and stays alive across many key
    presses (with keepalive pings). Each session is bounded by
    ``begin_session()`` / ``end_session()``.

    On ``end_session()`` we send a Finalize control message so Deepgram flushes
    any buffered audio immediately rather than waiting for silence-detection
    endpointing.
    """

    _ENV_KEY = "DEEPGRAM_API_KEY"

    def __init__(
        self,
        api_key: str | None = None,
        sample_rate: int = 16000,
        model: str = "nova-3",
        language: str = "en",
        keywords: list[str] | None = None,
    ) -> None:
        self.api_key = api_key or os.environ.get("DEEPGRAM_API_KEY")
        if not self.api_key:
            raise RuntimeError(
                "DEEPGRAM_API_KEY not set. Sign up at https://console.deepgram.com/signup "
                "and set the key in ~/.config/F9_talk/secrets.env"
            )
        self.sample_rate = sample_rate
        self.model = model
        self.language = language
        self.keywords = keywords or []

        self._client = DeepgramClient(
            self.api_key, DeepgramClientOptions(options={"keepalive": "true"})
        )
        self._conn = None
        self._connected = threading.Event()

        self._lock = threading.Lock()
        self._recording = False
        self._session_finals: list[str] = []
        self._final_arrived = threading.Event()
        self.last_error: str | None = None
        self._shutting_down = False
        self._reconnect_lock = threading.Lock()

    # ---------- lifecycle ----------

    def start(self) -> None:
        self._shutting_down = False
        self._open_connection()

    def _open_connection(self) -> None:
        """Open a fresh Deepgram WebSocket. Used by start() and the reconnect loop."""
        conn = self._client.listen.websocket.v("1")
        conn.on(LiveTranscriptionEvents.Open, self._on_open)
        conn.on(LiveTranscriptionEvents.Transcript, self._on_message)
        conn.on(LiveTranscriptionEvents.Close, self._on_close)
        conn.on(LiveTranscriptionEvents.Error, self._on_error)

        kw: dict = {}
        if self.keywords:
            kw["keywords"] = self.keywords
        opts = LiveOptions(
            model=self.model,
            language=self.language,
            encoding="linear16",
            sample_rate=self.sample_rate,
            channels=1,
            interim_results=False,  # dictate only commits finals
            smart_format=True,
            punctuate=True,
            endpointing=25,
            no_delay=True,
            **kw,
        )
        if not conn.start(opts):
            raise RuntimeError("Failed to start Deepgram WebSocket")
        self._conn = conn
        self._connected.wait(timeout=4.0)
        log.info("Deepgram socket open (model=%s, language=%s)", self.model, self.language)

    def stop(self) -> None:
        self._shutting_down = True
        if self._conn is not None:
            try:
                self._conn.finish()
            except Exception:
                pass
            self._conn = None

    def _reconnect_loop(self) -> None:
        """Auto-reconnect with exponential backoff after an unexpected close."""
        if not self._reconnect_lock.acquire(blocking=False):
            return  # another reconnect already in flight
        try:
            delay = 1.0
            while not self._shutting_down and not self._connected.is_set():
                time.sleep(delay)
                if self._shutting_down:
                    return
                if self._conn is not None:
                    try:
                        self._conn.finish()
                    except Exception:
                        pass
                    self._conn = None
                try:
                    log.info("Reconnecting Deepgram WebSocket...")
                    self._open_connection()
                    if self._connected.is_set():
                        log.info("Deepgram WebSocket reconnected")
                        return
                except Exception as e:  # noqa: BLE001
                    log.warning("Deepgram reconnect failed: %s", e)
                delay = min(delay * 2, 30.0)
        finally:
            self._reconnect_lock.release()

    # ---------- session API ----------

    def begin_session(self) -> None:
        with self._lock:
            self._recording = True
            self._session_finals = []
            self._final_arrived.clear()
            self.last_error = None

    def send_audio(self, pcm: bytes) -> None:
        if not self._connected.is_set():
            return
        with self._lock:
            if not self._recording:
                return
        try:
            self._conn.send(pcm)
        except Exception as e:  # noqa: BLE001
            log.debug("send failed: %s", e)

    def end_session(self, max_wait_ms: int = 350) -> str:
        with self._lock:
            self._recording = False
        # Force Deepgram to flush right now.
        try:
            if self._conn is not None and hasattr(self._conn, "finalize"):
                self._conn.finalize()
        except Exception as e:  # noqa: BLE001
            log.debug("finalize failed: %s", e)
        self._final_arrived.wait(timeout=max_wait_ms / 1000.0)
        time.sleep(0.03)  # grace for any straggler events
        with self._lock:
            return " ".join(self._session_finals).strip()

    # ---------- websocket handlers ----------

    def _on_open(self, *_args, **_kwargs) -> None:
        self._connected.set()

    def _on_close(self, *_args, **_kwargs) -> None:
        self._connected.clear()
        log.info("Deepgram socket closed")
        if not self._shutting_down:
            threading.Thread(
                target=self._reconnect_loop, daemon=True, name="deepgram-reconnect"
            ).start()

    def _on_error(self, *_args, error=None, **_kwargs) -> None:
        log.error("Deepgram error: %s", error)
        with self._lock:
            self.last_error = str(error) if error else "unknown error"

    def _on_message(self, *_args, result=None, **_kwargs) -> None:
        if result is None:
            return
        try:
            transcript = result.channel.alternatives[0].transcript
        except (AttributeError, IndexError):
            return
        if not transcript:
            return
        if not getattr(result, "is_final", False):
            return
        with self._lock:
            self._session_finals.append(transcript.strip())
            if not self._recording:
                self._final_arrived.set()
