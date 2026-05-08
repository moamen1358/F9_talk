"""Speech-to-text backends.

Three implementations behind a common protocol:
  - DeepgramStreamingSTT:    persistent cloud WebSocket (low latency, paid)
  - AssemblyAIStreamingSTT:  per-session WebSocket (lower latency claimed)
  - LocalWhisperSTT:         on-device faster-whisper on CUDA (free, private)

All expose: start(), stop(), begin_session(), send_audio(pcm), end_session()
"""
from f9_talk.stt.deepgram import DeepgramStreamingSTT

# AssemblyAI backend is optional — the import may fail if assemblyai isn't installed.
try:
    from f9_talk.stt.assemblyai import AssemblyAIStreamingSTT
except ImportError:  # pragma: no cover
    AssemblyAIStreamingSTT = None  # type: ignore[assignment]

# Local backend is optional — the import may fail if faster-whisper isn't installed.
try:
    from f9_talk.stt.local_whisper import LocalWhisperSTT
except ImportError:  # pragma: no cover
    LocalWhisperSTT = None  # type: ignore[assignment]

__all__ = ["DeepgramStreamingSTT", "AssemblyAIStreamingSTT", "LocalWhisperSTT"]
