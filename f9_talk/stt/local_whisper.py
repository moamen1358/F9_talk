"""On-device STT via faster-whisper (CUDA).

Buffers raw PCM during the session, runs a single transcribe pass on
``end_session()``. Simpler and lower-overhead than streaming local
inference for short dictation clips.
"""
from __future__ import annotations

import logging
import threading
import time

import numpy as np

log = logging.getLogger(__name__)


class LocalWhisperSTT:
    DEFAULT_MODEL = "large-v3-turbo"
    SAMPLE_RATE = 16000

    def __init__(
        self,
        model_name: str = DEFAULT_MODEL,
        language: str = "en",
        device: str = "cuda",
        compute_type: str = "float16",
        keywords: list[str] | None = None,
    ) -> None:
        self.model_name = model_name
        self.language = language
        self.device = device
        self.compute_type = compute_type
        # Whisper supports `initial_prompt` for soft term-biasing
        self.initial_prompt = ", ".join(keywords) if keywords else ""

        self._lock = threading.Lock()
        self._recording = False
        self._buffer = bytearray()
        self._model = None  # lazy-loaded in start()

    # ---------- lifecycle ----------

    def start(self) -> None:
        from faster_whisper import WhisperModel

        log.info(
            "Loading faster-whisper '%s' on %s (%s)...",
            self.model_name,
            self.device,
            self.compute_type,
        )
        t0 = time.monotonic()
        self._model = WhisperModel(
            self.model_name, device=self.device, compute_type=self.compute_type
        )
        # Warm up so first user request isn't slower than subsequent ones
        try:
            warmup = np.zeros(self.SAMPLE_RATE // 4, dtype=np.float32)
            list(self._model.transcribe(warmup, language=self.language, beam_size=1)[0])
        except Exception:
            pass
        log.info("Local Whisper ready in %.1fs", time.monotonic() - t0)

    def stop(self) -> None:
        self._model = None  # release VRAM

    # ---------- session API ----------

    def begin_session(self) -> None:
        with self._lock:
            self._recording = True
            self._buffer = bytearray()

    def send_audio(self, pcm: bytes) -> None:
        with self._lock:
            if self._recording:
                self._buffer.extend(pcm)

    def end_session(self, max_wait_ms: int = 0) -> str:  # noqa: ARG002 (API parity)
        with self._lock:
            self._recording = False
            audio_bytes = bytes(self._buffer)
            self._buffer = bytearray()
        if not audio_bytes or self._model is None:
            return ""
        samples = np.frombuffer(audio_bytes, dtype=np.int16).astype(np.float32) / 32768.0
        if len(samples) < int(self.SAMPLE_RATE * 0.2):
            return ""
        try:
            t0 = time.monotonic()
            segments, _info = self._model.transcribe(
                samples,
                language=self.language,
                beam_size=5,
                vad_filter=True,
                condition_on_previous_text=False,
                initial_prompt=self.initial_prompt or None,
                no_speech_threshold=0.6,
                log_prob_threshold=-1.0,
                hallucination_silence_threshold=0.5,
            )
            text = " ".join((s.text or "").strip() for s in segments).strip()
            log.debug(
                "local whisper transcribe %.0fms -> %r",
                (time.monotonic() - t0) * 1000,
                text[:60],
            )
            return text
        except Exception as e:  # noqa: BLE001
            log.exception("local whisper transcribe failed: %s", e)
            return ""
