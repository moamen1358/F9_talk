"""Speech-to-text backends.

Four implementations behind a common protocol:
  - DeepgramStreamingSTT:    persistent cloud WebSocket (low latency, paid)
  - AssemblyAIStreamingSTT:  per-session WebSocket (lower latency claimed)
  - GladiaStreamingSTT:      per-session WebSocket (sub-100ms partials, multilingual)
  - LocalWhisperSTT:         on-device faster-whisper on CUDA (free, private)

All expose: start(), stop(), begin_session(), send_audio(pcm), end_session()
"""
from f9_talk.stt.deepgram import DeepgramStreamingSTT

# AssemblyAI backend is optional — the import may fail if assemblyai isn't installed.
try:
    from f9_talk.stt.assemblyai import AssemblyAIStreamingSTT
except ImportError:  # pragma: no cover
    AssemblyAIStreamingSTT = None  # type: ignore[assignment]

# Gladia backend uses websockets + requests (both already required deps).
try:
    from f9_talk.stt.gladia import GladiaStreamingSTT
except ImportError:  # pragma: no cover
    GladiaStreamingSTT = None  # type: ignore[assignment]

# Local backend is optional — the import may fail if faster-whisper isn't installed.
try:
    from f9_talk.stt.local_whisper import LocalWhisperSTT
except ImportError:  # pragma: no cover
    LocalWhisperSTT = None  # type: ignore[assignment]

__all__ = [
    "DeepgramStreamingSTT",
    "AssemblyAIStreamingSTT",
    "GladiaStreamingSTT",
    "LocalWhisperSTT",
]
