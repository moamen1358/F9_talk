# Architecture

## Threading model

`f9-talk` runs four cooperating threads:

| Thread | Job | Lifetime |
|---|---|---|
| **GUI / main** | Qt event loop, indicator widget paint events, signal/slot dispatch | full app |
| **mic-streamer** | reads PCM from `parec` subprocess, calls `on_frame` per 25 ms chunk | full app (pre-spawned) |
| **hotkey-listener** | `pynput` keyboard event grab, dispatches press/release | full app |
| **dictate-finish** | runs `end_session()` → translate → type, off the GUI thread | per F9-release |

`DeepgramStreamingSTT` internally spawns its own WebSocket thread inside `deepgram-sdk`. `LocalWhisperSTT` runs synchronously on whatever thread calls `end_session()` — that's the `dictate-finish` worker.

## Data flow per dictation

1. **F9 down** (hotkey-listener thread)
   - sets `_recording = True`
   - emits `indicator.show_recording`
   - calls `stt.begin_session()` — clears any prior session state
2. **While F9 held** (mic-streamer thread, 40 frames/sec)
   - `_on_mic_frame(pcm)` runs:
     - if `_recording`, calls `stt.send_audio(pcm)` (queues to WebSocket / appends to local buffer)
     - computes RMS, emits `indicator.set_audio_level(level)` (drives the wave amplitude)
3. **F9 up** (hotkey-listener thread)
   - sets `_recording = False`
   - emits `indicator.set_status_text("✏ Transcribing…")`
   - spawns `dictate-finish` worker
4. **dictate-finish worker**
   - `text = stt.end_session()` — synchronous, blocks until transcript is ready
     - cloud: sends `{"type": "Finalize"}`, waits ≤ 350 ms for trailing final
     - local: runs `model.transcribe(buffered_audio)` synchronously (~50-450 ms)
   - if `target_lang` set: `text = translator.translate(text)` (HTTP roundtrip, ~150 ms)
   - emits `indicator.set_status_text("⌨ Typing…")`
   - `typer.type_text(text)` — synchronous xdotool subprocess call
   - emits `indicator.hide_recording`

## Why these threading choices

- **Mic always-on**: spawning `parec` per F9-press would add ~50 ms latency. Cheap to leave it running.
- **Hotkey listener separate from GUI**: `pynput` uses an X11 RECORD extension grab, can't share with the Qt event loop.
- **`dictate-finish` off the GUI thread**: `end_session()` may block up to ~500 ms. We don't want the indicator's 60 fps animation to stutter.
- **Qt signals with `Qt.QueuedConnection`** for indicator updates: cross-thread communication is automatic and ordered through the Qt event loop.

## Latency budget

For the cloud backend on this hardware (RTX 4060, 136 ms RTT to Deepgram):

| Stage | Time |
|---|---|
| Audio capture buffer | ~25 ms |
| Send last chunk → cloud | ~70 ms (RTT/2 + upload) |
| Deepgram inference of trailing audio | ~50-100 ms |
| Finalize round-trip | included above |
| `xdotool type` per ~50-char sentence | ~30-80 ms |
| Indicator status flash + Qt signal hops | ~20 ms |
| **Total perceived (key-up to text)** | **~250-350 ms** |

Local Whisper is similar except the "Deepgram inference" step is replaced by full-clip Whisper inference (~50-450 ms depending on clip length).

## Module dependency graph

```
cli.py
  └── app.py
        ├── audio/mic.py
        ├── stt/deepgram.py        (cloud path)
        ├── stt/local_whisper.py   (local path; optional dep)
        ├── translate/lingva.py
        │     └── translate/mymemory.py
        ├── ui/indicator.py
        └── input/
              ├── hotkey.py
              └── typer.py
```

`__init__.py` preloads CUDA cu12 libs as a side effect — required for the local backend, harmless otherwise.

## What lives where

| File | Responsibility | Lines |
|---|---|---|
| `app.py` | High-level orchestration: hotkey → STT → translate → type | ~210 |
| `stt/deepgram.py` | WebSocket session API; persistent connection w/ keepalive | ~150 |
| `stt/local_whisper.py` | faster-whisper buffered-batch transcription | ~110 |
| `ui/indicator.py` | Six animation styles, multi-monitor positioning, audio-reactive | ~330 |
| `audio/mic.py` | parec subprocess, frame reader thread | ~75 |
| `cli.py` | argparse, env-file loading, QApplication bootstrap | ~110 |
| `translate/lingva.py` + `mymemory.py` | HTTP translation w/ auto-fallback | ~130 |
| `input/hotkey.py` + `typer.py` | pynput key parsing + xdotool type wrapper | ~80 |

Total source: ~1,200 lines of Python.
