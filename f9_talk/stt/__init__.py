"""Speech-to-text backends.

Two implementations behind a common protocol:
  - DeepgramStreamingSTT: persistent cloud WebSocket (low latency, paid)
  - LocalWhisperSTT:      on-device faster-whisper on CUDA (free, private)

Both expose: start(), stop(), begin_session(), send_audio(pcm), end_session()
"""
from f9_talk.stt.deepgram import DeepgramStreamingSTT

# Local backend is optional — the import may fail if faster-whisper isn't installed.
try:
    from f9_talk.stt.local_whisper import LocalWhisperSTT
except ImportError:  # pragma: no cover
    LocalWhisperSTT = None  # type: ignore[assignment]

__all__ = ["DeepgramStreamingSTT", "LocalWhisperSTT"]
