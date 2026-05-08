# f9-talk

> Hold a key. Speak. Release. Text appears at your cursor.

System-wide hold-to-talk dictation for Linux. Works in any focused application — browser, editor, Slack, terminal — with no clipboard side-effects.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/moamen1358/F9_talk?label=release)](https://github.com/moamen1358/F9_talk/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20X11-lightgrey)](https://github.com/moamen1358/F9_talk)
[![Python](https://img.shields.io/badge/python-3.10%2B-blue)](https://python.org)

```
F9 ↓  →  🎙  Listening...
F9 ↑  →  ⌨   Text appears at cursor   (~300 ms)
```

---

## Features

- **Any application** — browser, terminal, IDE, Slack, anything with a text field
- **Cloud STT** via [Deepgram Nova-2](https://deepgram.com) — fast, accurate, free $200 credit
- **Local offline STT** via [faster-whisper](https://github.com/SYSTRAN/faster-whisper) (NVIDIA GPU)
- **Real-time translation** — speak English, type Arabic (or other languages)
- **Audio-reactive indicator** — six animation styles: `wave` `bars` `pulse` `dots` `ripple` `blob`
- **Custom hotkey** — configure any key or chord, not just F9
- **Dual-backend mode** — run cloud and local side-by-side for comparison
- **Domain keyword boosting** — improve accuracy for proper nouns and technical terms
- **Single-instance lock** — prevents accidental duplicate processes

---

## Install

### Option 1 — Debian package (recommended)

Download the latest `.deb` from the [Releases page](https://github.com/moamen1358/F9_talk/releases/latest) and install:

```bash
sudo dpkg -i f9-talk_*.deb
sudo apt-get install -f   # resolves any missing system dependencies
```

Add your Deepgram API key (get one free at [console.deepgram.com](https://console.deepgram.com/signup)):

```bash
nano ~/.config/F9_talk/secrets.env
# Add the line:  DEEPGRAM_API_KEY=your_key_here
```

f9-talk starts automatically on your next login. To launch immediately:

```bash
f9-talk --backend cloud
```

---

### Option 2 — From source

```bash
git clone https://github.com/moamen1358/F9_talk.git
cd F9_talk

python3 -m venv .venv && .venv/bin/pip install -e .
sudo apt install pulseaudio-utils xdotool libxcb-cursor0

./install.sh   # sets up autostart, desktop entry, and secrets file
./run.sh
```

For the local GPU backend, install additional dependencies:

```bash
.venv/bin/pip install -e '.[local]'
```

---

## Usage

Hold **F9**, speak, release. The transcribed text is typed at your cursor.

| Command | Description |
|---|---|
| `f9-talk` | Cloud STT, type in English |
| `f9-talk --target ar` | Speak English → type Arabic |
| `f9-talk --keyword Anthropic --keyword kubectl` | Boost recognition of custom terms |
| `f9-talk --backend local` | Offline Whisper on GPU |
| `f9-talk --backend both` | F9 = local · F8 = cloud |
| `f9-talk --local-hotkey '<ctrl>+<alt>+space'` | Custom hotkey |
| `f9-talk --style ripple` | Indicator style (`wave` `bars` `pulse` `dots` `ripple` `blob`) |
| `f9-talk -v` | Verbose / debug output |

---

## Configuration

All settings live in `~/.config/F9_talk/secrets.env`:

```env
DEEPGRAM_API_KEY=your_key_here
```

To change the hotkey or backend permanently, edit the autostart entry:

```bash
# .deb install
nano /etc/xdg/autostart/f9-talk.desktop

# Source install
nano ~/.config/autostart/f9-talk.desktop
```

Update the `Exec=` line with your preferred options, e.g.:

```
Exec=f9-talk --backend cloud --target ar --style pulse
```

---

## Troubleshooting

| Problem | Fix |
|---|---|
| `DEEPGRAM_API_KEY not set` | Add your key to `~/.config/F9_talk/secrets.env` |
| `parec not found` | `sudo apt install pulseaudio-utils` |
| `No type-injection tool found` | `sudo apt install xdotool` |
| Text appears in terminal instead of the app | Click into the target input field before pressing F9 |
| `no speech detected` | Clip is too short — speak for at least 0.3 s |
| CUDA errors with `--backend local` | `.venv/bin/pip install -e '.[local]'` |
| App won't start — another instance is running | `pkill f9-talk` then restart |

---

## Build the .deb locally

```bash
bash build_deb.sh
sudo dpkg -i f9-talk_*.deb
```

**Requirements:** `dpkg-dev`, `rsync`, optionally `librsvg2-bin` for high-quality icon scaling.

Release `.deb` packages are built and published automatically via GitHub Actions on every `v*` version tag.

---

## Requirements

| Requirement | Notes |
|---|---|
| Linux with X11 | Wayland not yet supported |
| Python 3.10+ | |
| PulseAudio or PipeWire | For microphone capture via `parec` |
| Deepgram API key | Cloud backend — [free signup](https://console.deepgram.com/signup) |
| NVIDIA GPU (optional) | Local backend only |

---

## Architecture

Threading model, data flow, and latency budget are documented in [`docs/architecture.md`](docs/architecture.md).

---

## License

[MIT](LICENSE) — Moamen Ghareeb
