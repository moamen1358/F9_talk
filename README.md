# f9-talk

> Hold a key. Speak. Release. Text appears at your cursor.

System-wide hold-to-talk dictation for Linux. Works in any focused application — browser, editor, Slack, terminal — with no clipboard side-effects.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/moamen1358/F9_talk?label=release)](https://github.com/moamen1358/F9_talk/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/moamen1358/F9_talk/ci.yml?label=CI)](https://github.com/moamen1358/F9_talk/actions/workflows/ci.yml)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20X11-lightgrey)](https://github.com/moamen1358/F9_talk)
[![Python](https://img.shields.io/badge/python-3.10%2B-blue)](https://python.org)

```
F9 ↓  →  🎙   Listening...
F9 ↑  →  ⌨    Text appears at cursor   (~250 ms with Deepgram Nova-3)
```

> ⚠️ **Linux + X11 only.** Windows, macOS, and Wayland are not supported. The app depends on `xdotool` for typing and `parec` for mic capture, both Linux-specific. Cross-platform support is a future goal — see [Requirements](#requirements).

---

## Install

**[→ Download the latest .deb release](https://github.com/moamen1358/F9_talk/releases/latest)**

```bash
sudo dpkg -i f9-talk_*.deb
sudo apt-get install -f          # resolves any missing system deps
```

f9-talk launches automatically on your next login. On first run, a setup dialog asks for your Deepgram API key — get one free at [console.deepgram.com](https://console.deepgram.com/signup).

To start immediately without rebooting:

```bash
f9-talk
```

---

## Use it

Hold **F9**, speak, release. The transcribed text types itself at your cursor.

That's it. No menus, no shortcuts to remember.

---

## Tray icon

f9-talk lives in your system tray (top-right of the panel on most Linux desktops):

| Action | Result |
|---|---|
| **Left-click** | Pause / resume the F9 hotkey (icon dims when paused) |
| **Right-click → Pause / Resume listening** | Same as left-click |
| **Right-click → Cloud provider ▶** | Switch live between Deepgram and Gladia |
| **Right-click → API Keys…** | Edit any provider's key — no file editing |
| **Right-click → Quit** | Exit |

A red icon + desktop notification appears if a session fails (bad API key, network drop, rate limit). The icon resets on the next successful transcription.

---

## Cloud providers

f9-talk supports two streaming STT providers; switch any time from the tray menu.

| Provider | Default model | Latency (warm) | Notes |
|---|---|---|---|
| **Deepgram** *(default)* | `nova-3` | ~250 ms | Persistent WebSocket — fastest for hold-to-talk |
| **Gladia** | v2 live | ~400 ms | Native multilingual code-switching |

Add the keys you have via the **API Keys…** dialog — both have free signup tiers:

- Deepgram: [console.deepgram.com](https://console.deepgram.com/signup) — $200 free credit
- Gladia: [app.gladia.io](https://app.gladia.io/) — free tier

Providers without a key simply appear greyed out in the menu.

---

## Features

- **Any application** — browser, terminal, IDE, Slack, anything with a text field
- **Two cloud providers** with live tray switching (Deepgram, Gladia)
- **Local offline backend** via [faster-whisper](https://github.com/SYSTRAN/faster-whisper) (NVIDIA GPU)
- **Real-time translation** — speak English, type Arabic (or other languages)
- **Audio-reactive indicator** — six animation styles: `wave` `bars` `pulse` `dots` `ripple` `blob`
- **Custom hotkey** — any key or chord, not just F9
- **Dual-backend mode** — run cloud and local side-by-side for comparison
- **Single-instance lock** — prevents accidental duplicate processes
- **Error feedback** — failed sessions surface as desktop notifications + a red tray icon

---

## Command-line options

| Command | Description |
|---|---|
| `f9-talk` | Cloud STT (default Deepgram), type in English |
| `f9-talk --target ar` | Speak English → type Arabic |
| `f9-talk --keyword Anthropic --keyword kubectl` | Boost recognition of custom terms |
| `f9-talk --backend local` | Offline Whisper on GPU |
| `f9-talk --backend both` | F9 = local · F8 = cloud |
| `f9-talk --local-hotkey '<ctrl>+<alt>+space'` | Custom hotkey |
| `f9-talk --style ripple` | Indicator style |
| `f9-talk -v` | Verbose / debug output |

To make any option permanent, edit the autostart entry:

```bash
sudo nano /etc/xdg/autostart/f9-talk.desktop
```

Update the `Exec=` line, e.g.:

```
Exec=f9-talk --target ar --style pulse
```

---

## Install from source

```bash
git clone https://github.com/moamen1358/F9_talk.git
cd f9-talk

python3 -m venv .venv && .venv/bin/pip install -e .
sudo apt install pulseaudio-utils xdotool libxcb-cursor0 libegl1

./install.sh        # sets up autostart, desktop entry, and secrets file
./run.sh
```

For the local GPU backend:

```bash
.venv/bin/pip install -e '.[local]'
```

To rebuild the `.deb` locally: `bash build_deb.sh && sudo dpkg -i f9-talk_*.deb`. Releases on `v*` tags are built automatically via GitHub Actions.

---

## Troubleshooting

| Problem | Fix |
|---|---|
| First-run dialog never appears | Existing `~/.config/F9_talk/secrets.env` already has a key; use the tray's **API Keys…** dialog to change it |
| `parec not found` | `sudo apt install pulseaudio-utils` |
| Text appears in terminal instead of the app | Click into the target input field before pressing F9 |
| `no speech detected` | Held F9 too briefly — speak for at least 0.3 s |
| Tray icon invisible (vanilla GNOME) | Install the AppIndicator extension; on Pop!_OS / Ubuntu it's already enabled |
| `App won't start — already running` | `pkill f9-talk` then relaunch |
| CUDA errors with `--backend local` | `.venv/bin/pip install -e '.[local]'` |

---

## Requirements

| Requirement | Notes |
|---|---|
| Linux with X11 | Wayland not yet supported |
| Python 3.10+ | |
| PulseAudio or PipeWire | For microphone capture via `parec` |
| At least one cloud key | Deepgram or Gladia — both have free tiers |
| NVIDIA GPU *(optional)* | Local Whisper backend only |

---

## Architecture

Threading model, data flow, and latency budget are documented in [`docs/architecture.md`](docs/architecture.md).

---

## License

[MIT](LICENSE) — Moamen Ghareeb
