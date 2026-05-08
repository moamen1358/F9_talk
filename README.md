# f9-talk

> Hold a key. Speak. Release. Text appears at your cursor.

System-wide hold-to-talk dictation for Linux. Works in every text field — browser, terminal, IDE, Slack, anywhere — under any keyboard layout. Default backend is **Deepgram Nova-3** streaming; an offline **whisper.cpp** backend is one flag away if you'd rather not depend on a cloud.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/moamen1358/F9_talk?label=release)](https://github.com/moamen1358/F9_talk/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20X11-lightgrey)](https://github.com/moamen1358/F9_talk)
[![Rust](https://img.shields.io/badge/rust-stable-orange)](https://rustup.rs)

```
F9 ↓  →  🎙   Listening   (audio-reactive overlay over your focused window)
F9 ↑  →  ⌨    Typing      (~250–300 ms with Deepgram Nova-3)
```

> ⚠️ **Linux only.** X11 is fully supported; Wayland needs layer-shell (sway / hyprland) and isn't wired up yet. Pop!_OS / Ubuntu / Fedora on X11 work out of the box.

## Highlights

- **Drops into any app** — types straight into the focused field; works under Arabic / Cyrillic / any keyboard layout.
- **Cloud or offline** — Deepgram Nova-3 streaming by default; `--backend local` for offline whisper.cpp (CUDA optional).
- **Real-time translation** — speak English, type Arabic (or any pair Lingva / MyMemory supports).
- **Stays out of your way** — wave overlay only paints while you hold the key, then disappears.
- **Self-healing** — auto-reconnect on WS / mic / suspend events.

---

## Quick start

**1. Install the `.deb`**

[→ Download the latest release](https://github.com/moamen1358/F9_talk/releases/latest)

```bash
sudo dpkg -i f9-talk_*.deb
sudo apt-get install -f          # fills any missing system deps
```

The package drops the binary at `/usr/bin/f9-talk`, adds you to the `input` group, installs the udev rule for `/dev/uinput`, and registers an autostart entry.

**2. Log out and log back in once.** Required so the input-group membership and udev rule take effect in your GUI session.

**3. Paste your Deepgram API key.** Right-click the tray icon → **API Keys…**. Get one free at [console.deepgram.com](https://console.deepgram.com/signup) ($200 starting credit covers personal use comfortably).

**4. Hold F9, speak, release.** That's the whole UI. The transcript types itself at your cursor.

---

## Tray

| Action | Result |
|---|---|
| Left-click | Pause / resume the F9 hotkey (icon dims when paused) |
| Right-click → **Pause / Resume listening** | Same as left-click |
| Right-click → **API Keys…** | Paste your Deepgram key; saving hot-reloads the backend |
| Right-click → **Quit** | Exit |

Three icon states: **active** (full colour), **paused** (desaturated 50 % alpha), **error** (red tint, set after a failed session, cleared on the next successful one).

---

## Command-line options

| Command | Description |
|---|---|
| `f9-talk` | Cloud STT, type at cursor (default) |
| `f9-talk --backend local` | Offline whisper.cpp; downloads `ggml-large-v3-turbo.bin` on first press |
| `f9-talk --target ar` | Speak English → type Arabic |
| `f9-talk --keyword Anthropic --keyword kubectl` | Bias recognition toward custom terms |
| `f9-talk --local-hotkey '<ctrl>+<alt>+space'` | Custom hotkey chord |
| `f9-talk --headless` | No indicator window (still uses the tray) |
| `f9-talk -v` | Verbose / debug logging |

To make any flag permanent, edit the autostart entry:

```bash
sudo nano /etc/xdg/autostart/f9-talk.desktop
# Update the Exec= line, e.g.:
#   Exec=f9-talk --target ar
```

---

## Build from source

```bash
git clone https://github.com/moamen1358/f9-talk.git
cd f9-talk

# Rust toolchain (skip if already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Linux build deps
sudo apt install build-essential pkg-config \
    libasound2-dev libdbus-1-dev libudev-dev libevdev-dev \
    libgtk-3-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev \
    libxcb-xfixes0-dev libxkbcommon-dev libfontconfig1-dev \
    libayatana-appindicator3-dev libssl-dev libxdo-dev libclang-dev

cargo build --release
./target/release/f9-talk --help
```

### Run during development

`run.sh` is a thin wrapper that rebuilds on demand and works around the `input`-group session issue (see Troubleshooting). Use it instead of reinstalling the `.deb` on every change.

```bash
./run.sh                       # launch the existing release binary
./run.sh --build               # rebuild first, then launch
./run.sh -v                    # pass -v through to f9-talk
./run.sh --target ar           # or any other f9-talk flag
```

What it does, in order:

1. `cd` to the repo root, regardless of where you call it from.
2. `cargo build --release` if `--build` is passed or the binary doesn't exist.
3. `pkill -f 'f9-talk$'` so the abstract-socket lock doesn't reject the new process.
4. `exec sg input -c "RUST_LOG=info ./target/release/f9-talk …"` so the binary runs with the `input` group active even if your GUI shell doesn't have it yet.

### Local Whisper with CUDA

```bash
sudo apt install nvidia-cuda-toolkit
cargo build --release --features cuda
```

### Rebuild the `.deb`

```bash
cargo install cargo-deb
cargo deb -p f9-talk
sudo dpkg -i target/debian/f9-talk_*.deb
```

---

## Architecture

Single statically-linked Rust binary. Three thread categories cooperate:

```
main thread (winit/eframe)             tokio runtime workers              cpal callback (RT)
─────────────────────────              ─────────────────────              ──────────────────
ViewportApp::update                  ┌─ hotkey-listener task  ─┐         build_input_stream
  │ paint wave + status              │   evdev events on F9    │           │
  │ ViewportCommand::OuterPosition   │                         │           │ down-mix to mono
  │ Visible(true/false)              ├─ session loop ──────────┤           │ resample 44.1→16k
  │                                  │   tokio::select! over   │           │ s16le bytes
  │  ▲                               │   - hotkey events       │           │
  │  └─ reads RmsHandle (Arc<Mutex>) │   - mic frame_rx        │ ◄───── mpsc::channel(64)
  │                                  │   - tray cmd_rx         │       drop-oldest on overflow
  │                                  │   - keys_save_tick      │
  │                                  │   - backend events      │
  │                                  └────────┬────────────────┘
  │                                           │
  │                                  ┌── STT WS client ───┐
  │                                  │   tokio-tungstenite│
  │                                  │   Deepgram Nova-3  │ ◄── frame_rx → send_audio()
  │                                  │   (or local Whisper)│   end_session() → oneshot
  │                                  └────────────────────┘
  │
  ▼
GTK thread (tray-icon)
  gtk::main() loop, MenuEvent → tokio mpsc
```

**Crates** (workspace under `crates/`):

| Crate | Role |
|---|---|
| `f9-talk-core` | Shared constants (frame size, sample rate, channel capacity). |
| `f9-talk-input` | `hotkey-listener` chord parser + 50 ms auto-repeat debounce; typer (xdotool / clipboard / uinput). |
| `f9-talk-audio` | cpal mic streamer with linear resampler and RMS for the wave indicator. |
| `f9-talk-stt` | `Stt` trait + Deepgram Nova-3 streaming client + whisper.cpp local backend. |
| `f9-talk-ui` | egui indicator viewport, tray icon, API-keys dialog, X11 positioner. |
| `f9-talk-translate` | Lingva (primary) + MyMemory (fallback) HTTP client. |
| `f9-talk` (bin) | clap CLI + secrets loader + abstract-socket lock + glue. |

**Reliability features baked in:**

- WS auto-reconnect on socket close + on three consecutive send failures. Backoff resets after a healthy connection drops, so a network blip after an hour of uptime reconnects in 1 s instead of the 30 s cap.
- Mic auto-restart on cpal stream errors with the same backoff.
- Wake-from-suspend detection: 5 s polling spots clock drift > 30 s and broadcasts a `WakeUp` so the STT client reconnects.
- Permission preflight at startup; missing `input` group or `/dev/uinput` access prints actionable instructions and exits non-zero.
- Single-instance lock on the abstract Unix socket `\0f9-talk-instance-lock`.

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| `/dev/uinput is not writable` / "no keyboards found" | The `.deb` adds you to the `input` group, but the GUI session needs to be restarted. **Log out and log back in once.** Until then, run via `./run.sh`. |
| Tray icon invisible (vanilla GNOME) | `sudo apt install gnome-shell-extension-appindicator`. (Pop!_OS / Ubuntu / KDE / COSMIC have native support.) |
| `no speech detected` | You released F9 too fast. Hold for at least 0.3 s. |
| Wrong characters under non-en-US layout | Make sure `which xdotool` returns a path; the typer logs `primary=xdotool` at startup. |
| "Already running" with no visible window | `pkill -f /usr/bin/f9-talk` and relaunch. |
| `wgpu` panic at startup | The shipped binary uses the OpenGL `glow` renderer; don't mix CUDA and wgpu in custom builds. |

Logs: `journalctl --user -t f9-talk -f`. Per-press latency lines have the target `f9_talk::press`.

---

## Requirements

| | Notes |
|---|---|
| **Linux + X11** | Wayland with layer-shell (sway / hyprland) is on the roadmap. |
| **Kernel ≥ 5.4** | For `uinput` + `evdev`. |
| **`xdotool`** | Auto-installed by the `.deb`; required for layout-independent typing. |
| **`input` group membership** | Added by the `.deb` postinst; one logout/login is required to take effect. |
| **Deepgram API key** | Free tier at [console.deepgram.com](https://console.deepgram.com/signup). Skip if you only use `--backend local`. |
| **NVIDIA GPU** *(optional)* | For local Whisper with `--features cuda`. |

---

## License

[MIT](LICENSE) — Moamen Ghareeb. No proprietary or paid components in the project itself; the cloud STT (Deepgram) and the optional MyMemory translation API have their own pricing tiers.
