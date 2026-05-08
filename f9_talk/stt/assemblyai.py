"""AssemblyAI Universal Streaming STT for hold-to-talk dictation.

Per-session WebSocket: open on ``begin_session()``, close on ``end_session()``.
This is ~100-200 ms higher latency than Deepgram's persistent connection on the
first frame but matches AssemblyAI's billing model (you pay only for active
streaming, not idle time).
"""
from __future__ import annotations

import logging
import os
import queue
import threading

log = logging.getLogger(__name__)


class AssemblyAIStreamingSTT:
    """Drop-in alternative to ``DeepgramStreamingSTT`` using AssemblyAI v3.

    Same interface: ``start()``, ``stop()``, ``begin_session()``, ``send_audio()``,
    ``end_session()``. The connection is opened per session because the SDK's
    ``StreamingClient`` is a one-shot object — reusing it across sessions
    requires shutting down and recreating it anyway.
    """

    def __init__(
        self,
        api_key: str | None = None,
        sample_rate: int = 16000,
    ) -> None:
        self.api_key = api_key or os.environ.get("ASSEMBLYAI_API_KEY")
        if not self.api_key:
            raise RuntimeError(
                "ASSEMBLYAI_API_KEY not set. Sign up at https://www.assemblyai.com/ "
                "and add the key to ~/.config/F9_talk/secrets.env"
            )
        self.sample_rate = sample_rate

        self._client = None
        self._stream_thread: threading.Thread | None = None
        self._queue: queue.Queue[bytes | None] = queue.Queue()

        self._lock = threading.Lock()
        self._recording = False
        self._final_text = ""
        self._final_event = threading.Event()
        self.last_error: str | None = None

    # ---------- lifecycle ----------

    def start(self) -> None:
        """No-op: connections are opened per session in ``begin_session()``."""

    def stop(self) -> None:
        self._teardown_client()

    # ---------- session API ----------

    def begin_session(self) -> None:
        from assemblyai.streaming.v3 import (
            StreamingClient,
            StreamingClientOptions,
            StreamingEvents,
            StreamingParameters,
        )

        with self._lock:
            self._recording = True
            self._final_text = ""
            self.last_error = None
        self._final_event.clear()
        self._queue = queue.Queue()

        try:
            client = StreamingClient(
                StreamingClientOptions(
                    api_key=self.api_key,
                    api_host="streaming.assemblyai.com",
                )
            )
            client.on(StreamingEvents.Turn, self._on_turn)
            client.on(StreamingEvents.Error, self._on_error)
            client.connect(StreamingParameters(sample_rate=self.sample_rate))
            self._client = client
        except Exception as e:  # noqa: BLE001
            log.error("AssemblyAI connect failed: %s", e)
            with self._lock:
                self.last_error = f"connect failed: {e}"
            self._client = None
            return

        self._stream_thread = threading.Thread(
            target=self._stream_loop, daemon=True, name="assemblyai-stream"
        )
        self._stream_thread.start()

    def send_audio(self, pcm: bytes) -> None:
        with self._lock:
            if not self._recording:
                return
        self._queue.put(pcm)

    def end_session(self, max_wait_ms: int = 800) -> str:
        with self._lock:
            self._recording = False
        self._queue.put(None)  # flush sentinel — ends the streaming generator
        self._final_event.wait(timeout=max_wait_ms / 1000.0)
        self._teardown_client()
        with self._lock:
            return self._final_text.strip()

    # ---------- internals ----------

    def _stream_loop(self) -> None:
        client = self._client
        if client is None:
            return

        def gen():
            while True:
                frame = self._queue.get()
                if frame is None:
                    return
                yield frame

        try:
            client.stream(gen())
        except Exception as e:  # noqa: BLE001
            log.debug("assemblyai stream loop ended: %s", e)

    def _teardown_client(self) -> None:
        client = self._client
        self._client = None
        if client is None:
            return
        try:
            client.disconnect(terminate=True)
        except Exception as e:  # noqa: BLE001
            log.debug("assemblyai disconnect failed: %s", e)

    # ---------- event handlers ----------

    def _on_turn(self, _client, event) -> None:
        transcript = getattr(event, "transcript", "") or ""
        if not transcript:
            return
        if not getattr(event, "end_of_turn", False):
            return
        with self._lock:
            # Concatenate finals across multiple turns within one session.
            if self._final_text:
                self._final_text = f"{self._final_text} {transcript}".strip()
            else:
                self._final_text = transcript.strip()
        self._final_event.set()

    def _on_error(self, _client, error) -> None:
        log.error("AssemblyAI streaming error: %s", error)
        with self._lock:
            self.last_error = str(error) if error else "streaming error"
