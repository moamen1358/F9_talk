# f9-talk

> Hold a key. Speak. Release. Text appears at your cursor.

System-wide hold-to-talk dictation for Linux. Works in any focused application — browser, editor, terminal, anything that takes typed input — under any keyboard layout.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/moamen1358/F9_talk?label=release)](https://github.com/moamen1358/F9_talk/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20X11-lightgrey)](https://github.com/moamen1358/F9_talk)
[![Rust](https://img.shields.io/badge/rust-stable-orange)](https://rustup.rs)

```
F9 ↓  →  🎙   Listening (audio-reactive overlay over your focused window)
F9 ↑  →  ⌨    Text appears at cursor   (~250–300 ms with Deepgram Nova-3)
```

> ⚠️ **Linux only.** v0.4 is Rust + X11. Wayland-compositor support requires layer-shell (sway/hypr) and is not yet wired up. Pop!_OS / Ubuntu / Fedora on X11 work out of the box.

---

## Install

**[→ Download the latest .deb release](https://github.com/moamen1358/F9_talk/releases/latest)**

```bash
sudo dpkg -i f9-talk_*.deb
sudo apt-get install -f          # fills in any missing system deps
```

The .deb does three things:

1. Drops the binary at `/usr/bin/f9-talk` (12 MB).
2. Adds you to the `input` group (needed for the kernel-level hotkey + uinput typer).
3. Installs the udev rule for `/dev/uinput` so the typer can write without sudo.

**Log out and log back in once** for the input-group membership and the udev rule to take effect, then f9-talk launches automatically. The tray icon appears top-right; right-click → **API Keys…** to paste a key.

---

## Use it

Hold **F9**, speak, release. The wave overlay anchors itself to your focused window while you hold; the transcript types itself at your cursor when you release.

That's it. No menus, no shortcuts.

---

## Tray

| Action | Result |
|---|---|
| **Left-click** | Pause / resume the F9 hotkey (icon dims when paused) |
| **Right-click → Pause / Resume listening** | Same as left-click |
| **Right-click → API Keys…** | Paste your Deepgram key; saving hot-reloads the active backend |
| **Right-click → Quit** | Exit |

Three icon states: **active** (full colour), **paused** (desaturated 50 % alpha), **error** (red tint, set after a failed session, cleared on the next successful one).

---

## Cloud STT — Deepgram Nova-3

f9-talk uses [Deepgram Nova-3](https://deepgram.com/) streaming over a persistent WebSocket. Latency is ~250–300 ms from F9 release to typed text on a warm connection. Free tier ($200 in starting credit) is plenty for personal use.

Get a key at [console.deepgram.com](https://console.deepgram.com/signup) and either drop it in `~/.config/F9_talk/secrets.env`:

```bash
DEEPGRAM_API_KEY=your-key-here
```

…or paste it via the tray's **API Keys…** dialog (saves to the same file with `0600` perms).

If you'd rather not depend on a cloud, use `--backend local` for offline whisper.cpp.

---

## Features

- **Any application** — browser, terminal, IDE, Slack, any text field.
- **Multi-monitor smart positioning** — the wave overlay snaps to the bottom of whichever window has focus, on whichever monitor it's on.
- **Layout-independent typing** — uses `xdotool` keysym-level injection, so dictation works even when you're typing under an Arabic / Cyrillic / etc. layout.
- **Cloud (Deepgram Nova-3) or local (whisper.cpp)** — pick at startup with `--backend cloud|local`. `--features cuda` for GPU on the local backend.
- **Real-time translation** — speak English, type Arabic (or any Lingva / MyMemory pair).
- **Audio-reactive indicator** — 56-point Bézier wave with four-layer paint, RMS-driven amplitude.
- **Auto-reconnect** — WS closes are recovered transparently with exponential backoff (1 s → 30 s cap).
- **Mic auto-restart** — if PipeWire/PulseAudio restarts, cpal reopens the stream and you keep dictating.
- **Wake-from-suspend detection** — long sleeps trigger explicit reconnects so the first F9 press after wake works.
- **Custom hotkey** — any chord, e.g. `<ctrl>+<alt>+space`.
- **Single-instance lock** — abstract Unix socket prevents duplicate processes.
- **Per-press tracing** — one line per F9 in `journalctl --user -t f9-talk` with `press_to_release`, `first_byte_sent`, `release_to_final`, `transcript`.

---

## Command-line options

| Command | Description |
|---|---|
| `f9-talk` | Deepgram cloud STT, type at cursor |
| `f9-talk --target ar` | Speak English → type Arabic |
| `f9-talk --keyword Anthropic --keyword kubectl` | Boost recognition of custom terms |
| `f9-talk --backend local` | Offline whisper.cpp (lazy-downloads `ggml-large-v3-turbo.bin` on first press) |
| `f9-talk --local-hotkey '<ctrl>+<alt>+space'` | Custom hotkey |
| `f9-talk --headless` | No indicator window — pure CLI mode (still uses the tray) |
| `f9-talk -v` | Verbose / debug output |

To make any option permanent, edit the autostart entry:

```bash
sudo nano /etc/xdg/autostart/f9-talk.desktop
# update the Exec= line, e.g.:
#   Exec=f9-talk --target ar
```

---

## Build from source

```bash
git clone https://github.com/moamen1358/f9-talk.git
cd f9-talk

# Rust toolchain (rustup); skip if already installed
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

For local Whisper with CUDA acceleration:

```bash
sudo apt install nvidia-cuda-toolkit
cargo build --release --features cuda
```

To rebuild the `.deb`:

```bash
cargo install cargo-deb
cargo deb -p f9-talk
sudo dpkg -i target/debian/f9-talk_*.deb
```

---

## Architecture

Single statically-linked Rust binary. Three thread categories:

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
  │                                  ┌── STT WS clients ──┐
  │                                  │   tokio-tungstenite│
  │                                  │   one task per     │ ◄── frame_rx → send_audio()
  │                                  │   active backend   │     end_session() → oneshot
  │                                  └────────────────────┘
  │
  ▼
GTK thread (tray-icon)
  gtk::main() loop, MenuEvent → tokio mpsc
```

**Data flow on a press**:

1. `evdev` event for `F9` press → `hotkey-listener` → tokio mpsc → session loop.
2. Session loop calls `backend.begin_session()` and flips `IndicatorState.recording = true`.
3. Indicator window switches `Visible(true)`, queries X11 for the focused-window geometry, sends `OuterPosition` for 5 frames so cross-monitor moves stick.
4. cpal pushes audio into the mpsc; session loop forwards every 25 ms frame to `backend.send_audio()`.
5. Backend WS client streams raw int16 PCM at 16 kHz.
6. On release: session loop calls `backend.end_session(350 ms)`. A fresh `oneshot::Sender<()>` is installed; the message handler `take()`s it on the first `is_final` after `recording=false`.
7. Returned transcript → optional translation (Lingva/MyMemory) → typer.
8. Typer prefers `xdotool type` (keysym-level, layout-independent), falls back to clipboard + Ctrl+V via uinput, falls back to raw scancode synthesis.
9. Indicator goes `Visible(false)`.

**Crates** (workspace under `crates/`):

| Crate | Role |
|---|---|
| `f9-talk-core` | Shared constants — frame size, sample rate, channel capacity. |
| `f9-talk-input` | `hotkey-listener` integration with chord parsing + 50 ms auto-repeat debounce, plus the typer (xdotool / clipboard / uinput). |
| `f9-talk-audio` | cpal mic streamer, linear resampler, RMS for the indicator. |
| `f9-talk-stt` | `Stt` trait + Deepgram Nova-3 streaming, whisper.cpp local. |
| `f9-talk-ui` | egui `IndicatorApp`, `tray-icon` tray, keys dialog, X11 positioner. |
| `f9-talk-translate` | Lingva (primary) + MyMemory (fallback) HTTP client. |
| `f9-talk` (bin) | clap CLI + secrets loader + abstract-socket lock + glue. |

**Reliability features baked in**:

- **WS auto-reconnect** on close + on three consecutive send failures (1 s → 30 s exponential backoff).
- **Mic auto-restart** on cpal stream errors (same backoff).
- **Wake-from-suspend detection** via 5 s `Instant::now()` polling; >30 s drift broadcasts a `WakeUp` so STT clients reconnect.
- **Permission preflight** at startup; missing `input` group / `/dev/uinput` access prints actionable instructions and exits non-zero.
- **Single-instance lock** on abstract Unix socket `\0f9-talk-instance-lock` — same name as the v0.3 Python build, so they can't run simultaneously.

---

## Troubleshooting

| Problem | Fix |
|---|---|
| `/dev/uinput is not writable` | `sudo usermod -aG input "$USER"`, then log out + back in |
| Tray icon invisible (vanilla GNOME) | `sudo apt install gnome-shell-extension-appindicator` (Pop!_OS / Ubuntu have it already) |
| Indicator window has a title bar | This was a v0.4-rc bug — make sure you're on `0.4.0+` (with `X11WindowType::Notification`) |
| `no speech detected` | Held F9 too briefly — speak for at least 0.3 s |
| Wrong characters typed under non-en-US layout | Confirm `which xdotool` returns a path; the typer logs `primary=xdotool` at startup |
| App won't start — "already running" | `pkill -f /usr/bin/f9-talk` then relaunch |
| `wgpu` panics at startup | The shipped builds use the `glow` (OpenGL) renderer; CUDA + wgpu mixing isn't supported |

Logs: `journalctl --user -t f9-talk -f`. Per-press latency lines have target `f9_talk::press`.

---

## Requirements

| Requirement | Notes |
|---|---|
| Linux + X11 | Wayland with layer-shell (sway/hyprland) is post-v0.4 |
| Recent kernel (≥5.4) | `uinput` + `evdev` |
| `xdotool` installed | Auto-installed by the .deb; required for layout-independent typing |
| Membership in `input` group | The .deb postinst adds you; you must log out + back in once |
| Deepgram API key | Free tier at [console.deepgram.com](https://console.deepgram.com/signup); not needed if you use `--backend local` |
| NVIDIA GPU *(optional)* | Local Whisper with `--features cuda` |

---

## License

[MIT](LICENSE) — Moamen Ghareeb. No proprietary or paid components in the project itself; the cloud STT (Deepgram) and the optional MyMemory translation API have their own pricing tiers.
