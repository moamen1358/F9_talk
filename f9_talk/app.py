"""Hold-to-talk dictation orchestrator.

Glues together: mic capture → STT backend(s) → optional translation → text injection
+ animated indicator.
"""
from __future__ import annotations

import logging
import threading
import time

import numpy as np
from pynput import keyboard

from f9_talk.audio import MicStreamer
from f9_talk.input import Typer, canonical_key, parse_hotkey
from f9_talk.stt import DeepgramStreamingSTT, LocalWhisperSTT
from f9_talk.translate import LingvaTranslator
from f9_talk.ui import DictateIndicator

log = logging.getLogger(__name__)


class DictateApp:
    """Dual-backend hold-to-talk dictation app.

    Args:
        indicator: shared Qt indicator (lives on the GUI thread)
        local_hotkey: key to hold for the local Whisper backend
        cloud_hotkey: key to hold for the Deepgram cloud backend
        target_lang: when set (e.g. "ar"), translate the transcript before typing
        keywords: domain-specific terms to bias STT toward (proper nouns / jargon)
        backend: "both" | "cloud" | "local" — which backends to load
    """

    def __init__(
        self,
        indicator: DictateIndicator,
        local_hotkey: str = "f9",
        cloud_hotkey: str = "f8",
        target_lang: str | None = None,
        keywords: list[str] | None = None,
        backend: str = "both",
    ) -> None:
        if backend not in ("both", "cloud", "local"):
            raise ValueError(f"Unknown backend: {backend!r}")

        self.indicator = indicator
        self.backend_mode = backend

        # In single-backend mode the chosen backend gets the local_hotkey (default F9)
        if backend == "cloud":
            cloud_hotkey = local_hotkey

        self.local_hotkey_spec = local_hotkey
        self.cloud_hotkey_spec = cloud_hotkey
        self.local_hotkey = parse_hotkey(local_hotkey)
        self.cloud_hotkey = parse_hotkey(cloud_hotkey)
        self.target_lang = target_lang
        self.translator = (
            LingvaTranslator(src_lang="en", target_lang=target_lang)
            if target_lang and target_lang != "en"
            else None
        )
        self.typer = Typer()

        if backend in ("both", "local"):
            if LocalWhisperSTT is None:
                raise RuntimeError(
                    "Local backend selected but faster-whisper is not installed.\n"
                    "Install with: pip install -e '.[local]'"
                )
            self.local_stt = LocalWhisperSTT(language="en", keywords=keywords)
        else:
            self.local_stt = None

        if backend in ("both", "cloud"):
            self.cloud_stt = DeepgramStreamingSTT(language="en", keywords=keywords)
        else:
            self.cloud_stt = None

        self.mic = MicStreamer(on_frame=self._on_mic_frame)

        self._pressed: set = set()
        self._recording = False
        self._active_backend: str | None = None
        self._record_started_at = 0.0
        self._busy_lock = threading.Lock()
        self._listener: keyboard.Listener | None = None
        self._release_timer: threading.Timer | None = None

    # ---------- hotkey routing ----------

    def _hotkey_for(self, backend: str) -> set:
        return self.local_hotkey if backend == "local" else self.cloud_hotkey

    def _on_press(self, key) -> None:
        canon = canonical_key(key)
        self._pressed.add(canon)
        # If a release timer is pending this press is X11 auto-repeat — cancel the
        # pending release so the session continues uninterrupted.
        if self._release_timer is not None:
            self._release_timer.cancel()
            self._release_timer = None
            return
        if self._recording:
            return
        if self.local_stt is not None and self.local_hotkey.issubset(self._pressed):
            self._begin_session("local")
        elif self.cloud_stt is not None and self.cloud_hotkey.issubset(self._pressed):
            self._begin_session("cloud")

    def _on_release(self, key) -> None:
        canon = canonical_key(key)
        self._pressed.discard(canon)
        if not self._recording:
            return
        active_hk = self._hotkey_for(self._active_backend or "local")
        if not active_hk.issubset(self._pressed):
            # 50 ms debounce: real releases are followed by silence;
            # X11 auto-repeat presses arrive within ~5 ms and cancel this timer.
            self._release_timer = threading.Timer(0.05, self._debounced_end)
            self._release_timer.start()

    def _debounced_end(self) -> None:
        self._release_timer = None
        self._end_session()

    # ---------- session lifecycle ----------

    def _begin_session(self, backend: str) -> None:
        self._active_backend = backend
        self._record_started_at = time.monotonic()
        log.info("🎙  recording [%s]...", backend)
        self.indicator.show_recording.emit()
        if backend == "local":
            assert self.local_stt is not None
            self.local_stt.begin_session()
        else:
            assert self.cloud_stt is not None
            self.cloud_stt.begin_session()
        self._recording = True

    def _end_session(self) -> None:
        backend = self._active_backend or "local"
        self._recording = False
        duration = time.monotonic() - self._record_started_at
        self.indicator.set_status_text.emit("✏  Transcribing…")
        threading.Thread(
            target=self._finish, args=(backend, duration), daemon=True, name="dictate-finish"
        ).start()

    def _on_mic_frame(self, pcm: bytes) -> None:
        if not self._recording:
            return
        # Forward to the active backend
        if self._active_backend == "local" and self.local_stt is not None:
            self.local_stt.send_audio(pcm)
        elif self.cloud_stt is not None:
            self.cloud_stt.send_audio(pcm)
        # Drive the audio-reactive wave amplitude
        try:
            samples = np.frombuffer(pcm, dtype=np.int16)
            if samples.size == 0:
                return
            rms = float(np.sqrt(np.mean(samples.astype(np.float32) ** 2))) / 32768.0
            self.indicator.set_audio_level.emit(rms)
        except Exception:  # noqa: BLE001
            pass

    def _finish(self, backend: str, duration: float) -> None:
        with self._busy_lock:
            try:
                if duration < 0.2:
                    log.info("(too short, ignored)")
                    if backend == "local" and self.local_stt is not None:
                        self.local_stt.end_session()
                    elif self.cloud_stt is not None:
                        self.cloud_stt.end_session(max_wait_ms=50)
                    return
                t0 = time.monotonic()
                if backend == "local":
                    assert self.local_stt is not None
                    text = self.local_stt.end_session()
                else:
                    assert self.cloud_stt is not None
                    text = self.cloud_stt.end_session(max_wait_ms=350)
                stt_ms = (time.monotonic() - t0) * 1000
                if not text:
                    log.info("(no speech detected)")
                    return
                if self.translator is not None:
                    self.indicator.set_status_text.emit("🌐  Translating…")
                    t1 = time.monotonic()
                    text = self.translator.translate(text) or text
                    log.debug("translate %.0fms", (time.monotonic() - t1) * 1000)
                self.indicator.set_status_text.emit("⌨  Typing…")
                log.info(
                    "✏  [%s, finalize %.0fms, %.1fs audio] -> %s",
                    backend,
                    stt_ms,
                    duration,
                    text,
                )
                self.typer.type_text(text)
            except Exception as e:  # noqa: BLE001
                log.exception("dictate finish failed: %s", e)
            finally:
                self.indicator.hide_recording.emit()

    # ---------- start / stop ----------

    def start(self) -> None:
        if self.cloud_stt is not None:
            self.cloud_stt.start()
        if self.local_stt is not None:
            self.local_stt.start()
        self.mic.start()
        self._listener = keyboard.Listener(on_press=self._on_press, on_release=self._on_release)
        self._listener.start()
        if self.backend_mode == "both":
            log.info(
                "Dictate ready. %s = local Whisper. %s = Deepgram cloud.",
                self.local_hotkey_spec,
                self.cloud_hotkey_spec,
            )
        elif self.backend_mode == "cloud":
            log.info("Dictate ready (cloud only). %s = Deepgram.", self.cloud_hotkey_spec)
        else:
            log.info("Dictate ready (local only). %s = Whisper.", self.local_hotkey_spec)

    def stop(self) -> None:
        if self._release_timer is not None:
            self._release_timer.cancel()
            self._release_timer = None
        if self._listener is not None:
            self._listener.stop()
            self._listener = None
        self.mic.stop()
        if self.cloud_stt is not None:
            self.cloud_stt.stop()
        if self.local_stt is not None:
            self.local_stt.stop()
