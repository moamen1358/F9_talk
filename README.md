# f9-talk

**Hold-to-talk dictation for Linux.** Press a key, speak, release — the text types at your cursor in any focused application (browser, editor, Slack, terminal). Cloud or local STT, optional translation, animated audio-reactive overlay.

```
F9 down → 🎙  Listening...      (red waveform, audio-reactive)
F9 up   → ✏  Transcribing…
        → ⌨  Typing…
        → text appears at cursor
```

Total perceived latency: **~250-450 ms** from key-release to text-on-screen.

---

## Table of contents

- [Features](#features)
- [Quick start](#quick-start)
- [Installation](#installation)
- [Configuration](#configuration)
- [Usage](#usage)
- [Architecture](#architecture)
- [Latency benchmarks](#latency-benchmarks)
- [Cost & privacy](#cost--privacy)
- [Troubleshooting](#troubleshooting)
- [Limitations](#limitations)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [License](#license)

---

## Features

### Core

- **Hold-to-talk** with configurable hotkey (default `F9`)
- **Cloud STT (Deepgram Nova-2)** — sub-second latency over WebSocket
- **Optional local STT (Whisper-large-v3-turbo)** — runs on your CUDA GPU, no internet
- **Type at cursor** in whatever app is focused — works in Slack, browser, terminal, IDE, etc.
- **Sub-300 ms** average latency from key-release to text typed (cloud backend)

### Indicator

- **Animated floating widget** appears near your text input while you record
- **Six visual styles**: `wave` (default Siri-like waveform), `bars`, `pulse`, `dots`, `ripple`, `blob`
- **Audio-reactive** — wave amplitude follows your actual mic volume in real time
- **Multi-monitor aware** — pops up on the screen with your active window
- **Click-through** — never blocks your work
- **Status flashes** — shows "Listening", "Transcribing", "Translating", "Typing" so you always know what's happening

### Translation

- **Speak English, type Arabic** (or any language code) on the fly
- **Lingva.ml** primary (Google quality, free, no key)
- **MyMemory** automatic fallback if Lingva is down (50 K chars/day free with email)

### Power features

- **Persistent WebSocket** — pre-warmed at app start, no TCP/TLS handshake on each press
- **Pre-spawned mic** — `parec` runs continuously, F9 just toggles forwarding
- **Force-finalize on release** — sends Deepgram a Finalize control message so it flushes instantly
- **Custom keyword boost** — `--keyword Anthropic --keyword kubectl` improves recognition of jargon
- **Smart short-clip rejection** — anything under 0.2 s is discarded
- **Translator auto-fallback** — Lingva → MyMemory if the primary is unreachable
- **Three backend modes** — cloud only, local only, or both bound to different keys

---

## Quick start

```bash
# 1. Clone + set up venv
git clone https://github.com/moamen1358/F9_talk.git
cd f9-talk
python3 -m venv .venv
.venv/bin/pip install -e .

# 2. System dependencies (one-time)
sudo apt install pulseaudio-utils xdotool libxcb-cursor0

# 3. Get a Deepgram API key (free $200 credit)
#    Sign up at https://console.deepgram.com/signup
mkdir -p ~/.config/F9_talk
echo "DEEPGRAM_API_KEY=your_key_here" > ~/.config/F9_talk/secrets.env
chmod 600 ~/.config/F9_talk/secrets.env

# 4. Run
./run.sh
```

Now hold **F9**, speak, release — text appears wherever your cursor is.

---

## Installation

### System dependencies

Required (one-time):

```bash
# Audio capture (PulseAudio / PipeWire CLI tools)
sudo apt install pulseaudio-utils

# Type-at-cursor injection
sudo apt install xdotool

# Qt platform plugin (PySide6 needs this on Pop!_OS / Ubuntu 22.04+)
sudo apt install libxcb-cursor0
```

### Python package

`f9-talk` requires Python ≥ 3.10. Only Linux (X11) is supported as of v0.1.0.

```bash
python3 -m venv .venv
.venv/bin/pip install --upgrade pip
.venv/bin/pip install -e .             # cloud backend only
# OR
.venv/bin/pip install -e '.[local]'    # cloud + local Whisper backend
```

The local backend pulls in `faster-whisper`, `ctranslate2`, and the `nvidia-cu12` CUDA shared libraries (~600 MB download). Skip it if you only want the cloud backend.

### API key (Deepgram, cloud backend)

1. Sign up: <https://console.deepgram.com/signup> — no credit card required, $200 free credit (~ 750 hours of streaming).
2. Console → **API Keys** → **Create New API Key** → name it `f9-talk`, leave permissions at default → **Create Key**.
3. Copy the key (long alphanumeric string).
4. Save it:
   ```bash
   mkdir -p ~/.config/F9_talk
   echo "DEEPGRAM_API_KEY=your_actual_key" > ~/.config/F9_talk/secrets.env
   chmod 600 ~/.config/F9_talk/secrets.env
   ```

### Optional: MyMemory translation rate-limit bump

Anonymous MyMemory gives 5,000 chars/day. Adding any email bumps it to 50,000 chars/day.

```bash
echo "MYMEMORY_EMAIL=you@example.com" >> ~/.config/F9_talk/secrets.env
```

---

## Configuration

### Environment variables

Read from `~/.config/F9_talk/secrets.env` (preferred) or `./.env` in the project root.

| Variable | Required | Notes |
|---|---|---|
| `DEEPGRAM_API_KEY` | for cloud backend | Get free at https://console.deepgram.com/ |
| `MYMEMORY_EMAIL` | optional | Bumps translation fallback from 5K → 50K chars/day |

### Command-line flags

```text
f9-talk [-h] [--backend {cloud,local,both}]
             [--local-hotkey HOTKEY] [--cloud-hotkey HOTKEY]
             [--target LANG] [--keyword TERM]
             [--style STYLE] [-v] [-V]
```

| Flag | Default | Description |
|---|---|---|
| `--backend` | `cloud` | `cloud` (Deepgram), `local` (Whisper on CUDA), or `both` (F9=local, F8=cloud). |
| `--local-hotkey` | `f9` | Key to hold for the local backend. Examples: `f9`, `<ctrl>+<alt>+space`, `<pause>`. |
| `--cloud-hotkey` | `f8` | Key for cloud backend (only used when `--backend both`). |
| `--target` | _none_ | Target language code for translation. E.g. `ar`, `fr`, `es`. Omit to type raw English. |
| `--keyword` | _none_ | Boost a domain-specific term. Repeatable: `--keyword foo --keyword bar`. |
| `--style` | `wave` | Indicator animation: `wave`, `bars`, `pulse`, `dots`, `ripple`, `blob`. |
| `-v` | off | Debug logging. |
| `-V` | _–_ | Print version and exit. |

---

## Usage

### Default: cloud STT, type English

```bash
./run.sh
# Hold F9, speak, release. Text types at cursor.
```

### Translate to Arabic on the fly

```bash
./run.sh --target ar
# Speak English, Arabic types at cursor.
```

### Boost domain-specific terms

```bash
./run.sh --keyword Anthropic --keyword kubectl --keyword Moamen
# Now Deepgram will recognize "Anthropic" instead of "and tropic", etc.
```

### Use a custom hotkey

```bash
./run.sh --local-hotkey '<ctrl>+<alt>+space'
```

### Local STT (private, free, GPU-accelerated)

```bash
# Install the local backend deps first (one-time)
.venv/bin/pip install -e '.[local]'

./run.sh --backend local
# Now F9 uses Whisper-large-v3-turbo on your CUDA GPU. No internet needed.
```

### Compare both backends side-by-side

```bash
./run.sh --backend both
# F9 = local Whisper, F8 = Deepgram cloud. Try both, pick your favorite.
```

### Verbose mode (for debugging)

```bash
./run.sh -v
# Shows finalize timing, audio RMS, and full transcripts.
```

### Combine flags

```bash
./run.sh --backend cloud --target ar --keyword Anthropic --style ripple -v
```

---

## Architecture

```
                   ┌──────────────────┐
                   │  pynput hotkey   │   F9 down/up events
                   │     listener     │ ──────────────┐
                   └──────────────────┘               │
                                                      ▼
┌──────────────┐    raw 25 ms        ┌────────────────────────────┐
│   parec      │ ──── PCM frames ──► │       DictateApp           │
│ (always-on)  │                     │  (orchestrator, glue)      │
└──────────────┘                     └─────┬──────────────┬───────┘
                                           │              │
                              record? ────►│              │
                                           ▼              │
                  ┌────────────────────────┐              │
                  │  STT backend (active)  │              │
                  │  • DeepgramStreamingSTT│              │
                  │    (cloud WebSocket)   │              │
                  │  • LocalWhisperSTT     │              │
                  │    (CUDA, faster-      │              │
                  │     whisper)           │              │
                  └─────────┬──────────────┘              │
                            │ final transcript            │
                            ▼                             ▼
                  ┌─────────────────────┐    ┌──────────────────────┐
                  │ LingvaTranslator    │    │   DictateIndicator   │
                  │ (optional, on tgt   │    │  (Qt floating widget,│
                  │  set; falls back to │    │   60 fps animation)  │
                  │  MyMemory)          │    │                      │
                  └────────┬────────────┘    └──────────────────────┘
                           │
                           ▼
                  ┌─────────────────────┐
                  │  Typer              │
                  │  (xdotool type at   │
                  │   cursor)           │
                  └─────────────────────┘
```

### Key design choices

- **Persistent WebSocket** for the cloud backend so each F9 press doesn't pay a TLS handshake
- **Mic always running**, gated by a `_recording` flag — first press feels as fast as the hundredth
- **`Finalize` control message on release** instead of waiting for silence-based endpointing — instant commit
- **Buffer-then-transcribe for local Whisper** (rather than streaming inference) — simpler, faster for short clips
- **Asymmetric EMA** on the audio-level signal so the wave responds quickly to loud bursts but decays smoothly
- **Type at cursor via `xdotool`** (not clipboard paste) — reliable on bare X11 without `xclip`/`xsel`

### Module layout

```
f9_talk/
├── __init__.py          # CUDA preload (cu12 cublas/cudnn) + version
├── __main__.py          # python -m f9_talk
├── cli.py               # argparse + entry point
├── app.py               # DictateApp orchestrator
├── audio/
│   └── mic.py           # MicStreamer (parec subprocess)
├── stt/
│   ├── deepgram.py      # DeepgramStreamingSTT (cloud)
│   └── local_whisper.py # LocalWhisperSTT (CUDA)
├── translate/
│   ├── lingva.py        # primary (Google proxy, no key)
│   └── mymemory.py      # fallback (5K-50K chars/day)
├── ui/
│   └── indicator.py     # DictateIndicator (Qt floating widget)
└── input/
    ├── hotkey.py        # parse_hotkey, canonical_key
    └── typer.py         # Typer (xdotool/wtype/ydotool)
```

---

## Latency benchmarks

Measured on the same 10.75 s English clip, RTX 4060 Laptop GPU, ~136 ms RTT to Deepgram (Egypt → US datacenter), 3 runs each:

| Backend | Finalize avg | Min | Max | Notes |
|---|---|---|---|---|
| **Deepgram Nova-2** (cloud) | **312 ms** | 273 ms | 344 ms | Streaming — most audio is already transcribed when you release the key |
| **Whisper-large-v3-turbo** (local) | 452 ms | 450 ms | 455 ms | Batch — runs after release, very deterministic |

Cloud wins by ~140 ms here because it's transcribing while you speak. For short clips (< 2 s), local has an edge because there's less audio to process all at once.

Run the benchmark yourself:

```bash
.venv/bin/python scripts/benchmark.py path/to/clip.wav
```

---

## Cost & privacy

| Aspect | Cloud (Deepgram) | Local (Whisper) |
|---|---|---|
| **Cost — first year** | $0 (within $200 credit) | $0 |
| **Cost — after credit** | ~$0.26/hour streaming | $0 forever |
| **Cost — daily 30 min use** | ~12+ months free | always free |
| **Audio leaves machine** | yes (sent to Deepgram) | no (stays on GPU) |
| **Internet required** | yes | no |
| **VRAM held** | 0 | ~2.2 GB while running |
| **Startup time** | < 1 s | ~3-5 s (model load) |

Both backends are reasonable choices. Cloud is faster on long clips and slightly more robust on noisy audio; local is private, free forever, and works offline.

---

## Troubleshooting

### "DEEPGRAM_API_KEY not set"

You're using the cloud backend without setting your API key. See [API key setup](#api-key-deepgram-cloud-backend).

### "parec not found"

Install PulseAudio CLI utilities: `sudo apt install pulseaudio-utils`.

### "No type-injection tool found"

Install `xdotool`: `sudo apt install xdotool`.

### The text doesn't appear at the cursor

Make sure you have **a text input focused** before pressing F9 — `xdotool` types into whatever window has keyboard focus. If the text appears in the terminal that launched f9-talk, that's because the terminal still had focus when you released F9.

### "Failed to start Deepgram WebSocket" with DNS errors

Likely a transient network blip. Restart and try again. If it persists, test connectivity: `ping api.deepgram.com`.

### CUDA library errors when running with `--backend local`

The local backend needs the cu12 cublas/cudnn libs from the `nvidia-cu12-*` pip packages. Install with `.venv/bin/pip install -e '.[local]'`. The `f9_talk/__init__.py` preloads them at import time.

### Indicator appears on the wrong monitor

The indicator anchors to the bottom-center of the **focused window**'s monitor. If your text-input window is on a different monitor than the one you expect, click into the input field before pressing F9.

### "no speech detected" for short utterances

Anything under 0.2 s is discarded. Speak slightly longer.

---

## Limitations

- **Linux X11 only.** Wayland support requires `wtype` (auto-detected) but is not officially tested. macOS and Windows aren't supported (would need different audio capture, hotkey, and type-at-cursor backends).
- **English-only audio input.** Both backends are configured with `language=en`. Speaking another language and translating to English isn't supported — for that, the cloud backend's `model=nova-2 language=multi` option exists but is not exposed via flag yet.
- **No live word-by-word typing.** We commit the entire transcript on key-release, not per-word. This avoids the "typing then backspacing as the model revises" UX problem.
- **DRM-protected app audio is unreachable.** This is dictation, not system-audio capture, so this only matters if you ever extend to capturing other apps.
- **MyMemory daily quota.** If Lingva is down and you exceed 50 K chars/day on MyMemory, translation falls back to passing the English text through unchanged.

---

## Roadmap

Things this project does *not* do, but reasonable extensions:

- Tray icon / system notification for app status
- Save transcripts to a daily log file
- Per-app prompt presets (e.g., "format as email" via an LLM post-step)
- Voice commands like "delete that" / "newline"
- Multilingual source mode (`--source ar` etc.)
- macOS support (Metal-accelerated whisper.cpp + cmd-key + AXUIElement injection)

---

## Contributing

This is a personal project. Bug reports and feature requests welcome via issues; PRs welcome but please open an issue first to discuss scope.

Coding style: `ruff` with the config in `pyproject.toml`.

```bash
.venv/bin/pip install -e '.[dev]'
.venv/bin/ruff check f9_talk scripts
.venv/bin/ruff format f9_talk scripts
```

---

## License

MIT — see [LICENSE](LICENSE).

## Acknowledgments

- [Deepgram](https://deepgram.com) for the Nova-2 cloud STT and a generous free tier
- [faster-whisper](https://github.com/SYSTRAN/faster-whisper) — local Whisper inference
- [Lingva.ml](https://lingva.ml) — free Google Translate proxy
- [MyMemory](https://mymemory.translated.net/) — free translation fallback
- [PySide6](https://doc.qt.io/qtforpython-6/) — the Qt UI framework
- Inspired by [Wispr Flow](https://wisprflow.ai), [MacWhisper](https://goodsnooze.gumroad.com/l/macwhisper), [Glaido](https://glaido.com)
