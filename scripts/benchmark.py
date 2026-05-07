#!/usr/bin/env python3
"""Benchmark Deepgram vs local-Whisper backends on identical audio.

Usage:
    .venv/bin/python scripts/benchmark.py path/to/clip.wav

Streams the audio in 25 ms frames at real-time pacing through each backend's
session API and measures the time `end_session()` takes to return — the
critical "key-up to text-ready" latency.
"""
from __future__ import annotations

import os
import sys
import time
from pathlib import Path

import numpy as np
import soundfile as sf

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import f9_talk  # noqa: F401  (CUDA preload)
from f9_talk.stt import DeepgramStreamingSTT, LocalWhisperSTT


FRAME_MS = 25
SAMPLE_RATE = 16000
FRAME_BYTES = SAMPLE_RATE * FRAME_MS // 1000 * 2


def load_pcm(path: Path) -> bytes:
    audio, sr = sf.read(str(path), dtype="float32")
    if audio.ndim > 1:
        audio = audio[:, 0]
    if sr != SAMPLE_RATE:
        from scipy.signal import resample_poly  # type: ignore
        audio = resample_poly(audio, SAMPLE_RATE, sr)
    pcm = np.clip(audio, -1.0, 1.0)
    return (pcm * 32767.0).astype(np.int16).tobytes()


def stream(stt, pcm: bytes) -> float:
    t0 = time.monotonic()
    dt = FRAME_MS / 1000.0
    n = (len(pcm) + FRAME_BYTES - 1) // FRAME_BYTES
    next_send = time.monotonic()
    for i in range(n):
        chunk = pcm[i * FRAME_BYTES : (i + 1) * FRAME_BYTES]
        stt.send_audio(chunk)
        next_send += dt
        now = time.monotonic()
        if next_send > now:
            time.sleep(next_send - now)
    return time.monotonic() - t0


def bench(name: str, stt, pcm: bytes, runs: int = 3) -> dict:
    print(f"\n=== {name} ===")
    print("  warm-up...")
    stt.start()
    stt.begin_session()
    stream(stt, pcm[: SAMPLE_RATE * 2])
    _ = stt.end_session() if isinstance(stt, LocalWhisperSTT) else stt.end_session(max_wait_ms=1000)
    print("  warm-up done.")

    finalize_ms_list: list[float] = []
    transcripts: list[str] = []
    for r in range(runs):
        stt.begin_session()
        stream_secs = stream(stt, pcm)
        t0 = time.monotonic()
        text = (
            stt.end_session(max_wait_ms=600)
            if isinstance(stt, DeepgramStreamingSTT)
            else stt.end_session()
        )
        finalize_ms = (time.monotonic() - t0) * 1000
        finalize_ms_list.append(finalize_ms)
        transcripts.append(text)
        print(f"  run {r+1}: streamed {stream_secs:.2f}s, finalize {finalize_ms:.0f}ms")
        print(f"          transcript: {text[:100]!r}")

    return {
        "name": name,
        "finalize_avg": sum(finalize_ms_list) / len(finalize_ms_list),
        "finalize_min": min(finalize_ms_list),
        "finalize_max": max(finalize_ms_list),
    }


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: benchmark.py <wav-file>", file=sys.stderr)
        return 1

    path = Path(sys.argv[1])
    if not path.exists():
        print(f"audio file not found: {path}", file=sys.stderr)
        return 1

    print(f"Loading audio: {path}")
    pcm = load_pcm(path)
    print(f"  duration: {len(pcm) / 2 / SAMPLE_RATE:.2f}s")

    if not os.environ.get("DEEPGRAM_API_KEY"):
        print("WARN: DEEPGRAM_API_KEY not set — cloud bench will fail.", file=sys.stderr)

    cloud = DeepgramStreamingSTT(language="en")
    cloud_r = bench("Deepgram Nova-2 (cloud)", cloud, pcm)
    cloud.stop()

    local = LocalWhisperSTT(language="en")
    local_r = bench("Whisper-large-v3-turbo (local CUDA)", local, pcm)
    local.stop()

    print("\n" + "=" * 60)
    print("SUMMARY (lower finalize_avg is better)")
    print("=" * 60)
    for r in (cloud_r, local_r):
        print(f"  {r['name']}")
        print(f"    finalize avg = {r['finalize_avg']:.0f}ms (min {r['finalize_min']:.0f}, max {r['finalize_max']:.0f})")
    diff = cloud_r["finalize_avg"] - local_r["finalize_avg"]
    if diff > 20:
        print(f"\n  → LOCAL is faster by {diff:.0f}ms on average")
    elif diff < -20:
        print(f"\n  → CLOUD is faster by {-diff:.0f}ms on average")
    else:
        print(f"\n  → Effectively tied (within {abs(diff):.0f}ms)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
