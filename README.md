# f9-talk

f9-talk is a system-wide hold-to-talk dictation tool for Linux. The user
holds a hotkey, speaks, and releases; the transcribed text is typed into
the focused window via `xdotool`, regardless of application or keyboard
layout. The default backend is Deepgram Nova-3 streaming over WebSocket.
An offline whisper.cpp backend with optional CUDA support is available
behind a flag.

The project ships as a single statically-linked Rust binary distributed
as a `.deb` package. It is currently Linux-only and X11-only; Wayland
support is on the roadmap.

## Requirements

| Component | Notes |
|---|---|
| Linux on X11 | Tested on Pop!_OS, Ubuntu, Fedora. Wayland with layer-shell is not yet supported. |
| Kernel 5.4 or newer | Required for `uinput` and `evdev` |
| `xdotool` | Auto-installed by the `.deb`; required for layout-independent typing |
| `input` group membership | Added by the `.deb` postinst script; requires one logout/login to take effect |
| Deepgram API key | Required only when using the cloud backend (default). Free tier available at [console.deepgram.com](https://console.deepgram.com/signup) |
| NVIDIA GPU | Optional, used by the local Whisper backend when built with `--features cuda` |

## Installation

Three install paths are supported. The `.deb` is the most automated;
the AppImage and cargo installer reach the same end state after one
extra `f9-talk install` call.

| Method | Sets up automatically | One-time follow-up |
|---|---|---|
| `.deb` package | binary, apps menu, autostart, udev rule, `input` group, secrets stub | log out + back in once |
| AppImage | nothing on the system | `./f9-talk-*.AppImage install --user` then `sudo ./f9-talk-*.AppImage install --system` |
| `curl \| sh` (cargo-dist) | binary in `~/.cargo/bin/` | `f9-talk install --user` then `sudo f9-talk install --system` |

### `.deb` (recommended)

[Download the latest release](https://github.com/moamen1358/f9-talk/releases/latest) and install:

```bash
sudo dpkg -i f9-talk_*.deb
sudo apt-get install -f
```

The package installs the binary at `/usr/bin/f9-talk`, adds the user to
the `input` group, installs the udev rule for `/dev/uinput`, registers
an autostart entry, and seeds `~/.config/F9_talk/secrets.env` with a
placeholder. Log out and log back in once so the group membership and
udev rule take effect in the GUI session.

### AppImage

```bash
chmod +x f9-talk-*.AppImage
./f9-talk-*.AppImage install --user         # apps menu + autostart + secrets stub
sudo ./f9-talk-*.AppImage install --system  # udev rule + adds you to `input`
```

The first command writes `~/.local/share/applications/f9-talk.desktop`,
`~/.config/autostart/f9-talk.desktop`, and seeds
`~/.config/F9_talk/secrets.env` (only if missing). The second installs
`/etc/udev/rules.d/99-f9-talk.rules` and adds you to the `input` group.
Log out + back in once. Use `f9-talk uninstall --user` /
`--system` to reverse either side; secrets are always preserved.

### cargo-dist (`curl | sh`)

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/moamen1358/f9-talk/releases/latest/download/f9-talk-installer.sh | sh
f9-talk install --user
# `sudo` strips PATH; pass the absolute path or your $HOME for it to find the binary:
sudo "$(command -v f9-talk)" install --system
```

### Configuring the API key

After installing, paste a Deepgram API key into
`~/.config/F9_talk/secrets.env` (or, on session-tray-supporting
desktops, right-click the tray icon → **API Keys…**). The
configuration hot-reloads on save.

## Usage

Hold F9, speak, release. The transcript is typed at the cursor.

The tray icon supports three states: active (full color), paused
(desaturated), and error (red tint, set after a failed session and
cleared on the next successful one).

| Tray action | Result |
|---|---|
| Left-click | Pause or resume the F9 hotkey |
| Right-click → Pause / Resume listening | Same as left-click |
| Right-click → API Keys… | Edit and hot-reload the Deepgram key |
| Right-click → Quit | Exit |

## Command-line options

| Command | Description |
|---|---|
| `f9-talk` | Cloud STT, types at the cursor (default) |
| `f9-talk --backend local` | Offline whisper.cpp; downloads `ggml-large-v3-turbo.bin` on first press |
| `f9-talk --target ar` | Translates English to Arabic before typing |
| `f9-talk --keyword Anthropic --keyword kubectl` | Biases recognition toward custom terms |
| `f9-talk --local-hotkey '<ctrl>+<alt>+space'` | Custom hotkey chord |
| `f9-talk --headless` | Disables the indicator window; the tray icon stays active |
| `f9-talk -v` | Verbose logging |
| `f9-talk install --user` | Writes apps-menu entry, autostart entry, and `secrets.env` stub under `~/.config` / `~/.local/share` |
| `f9-talk install --system` | Installs udev rule + adds user to `input` group (needs `sudo`) |
| `f9-talk uninstall [--user\|--system]` | Reverses the matching `install` step. Secrets are always preserved. |

To make a flag permanent, edit the autostart entry at
`/etc/xdg/autostart/f9-talk.desktop` and update the `Exec=` line.

## Building from source

```bash
git clone https://github.com/moamen1358/f9-talk.git
cd f9-talk

# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Linux build dependencies
sudo apt install build-essential pkg-config \
    libasound2-dev libdbus-1-dev libudev-dev libevdev-dev \
    libgtk-3-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev \
    libxcb-xfixes0-dev libxkbcommon-dev libfontconfig1-dev \
    libayatana-appindicator3-dev libssl-dev libxdo-dev libclang-dev

cargo build --release
./target/release/f9-talk --help
```

`run.sh` rebuilds on demand and works around the `input`-group session
issue described in the troubleshooting section. Use it instead of
reinstalling the `.deb` on every change:

```bash
./run.sh             # launch the existing release binary
./run.sh --build     # rebuild first, then launch
./run.sh --target ar # any f9-talk flag is forwarded
```

To enable the local Whisper backend with CUDA:

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

## Architecture

A single statically-linked Rust binary. Three thread categories
cooperate over `tokio::mpsc` and `Arc<Mutex>` channels:

```
main thread (winit/eframe)         tokio runtime workers              cpal callback (RT)
─────────────────────────          ─────────────────────              ──────────────────
ViewportApp::update              ┌─ hotkey-listener task ─┐           build_input_stream
  paint wave + status            │   evdev events on F9   │             down-mix to mono
  ViewportCommand::OuterPosition │                        │             resample 44.1→16k
  Visible(true/false)            ├─ session loop ─────────┤             s16le bytes
                                 │   tokio::select! over: │
  reads RmsHandle (Arc<Mutex>)   │   - hotkey events      │ ◄──── mpsc::channel(64)
                                 │   - mic frame_rx       │       drop-oldest on overflow
                                 │   - tray cmd_rx        │
                                 │   - keys_save_tick     │
                                 │   - backend events     │
                                 └────────┬───────────────┘
                                          │
                                 ┌── STT WS client ───┐
                                 │   tokio-tungstenite│
                                 │   Deepgram Nova-3  │ ◄── frame_rx → send_audio()
                                 │   (or local Whisper)│  end_session() → oneshot
                                 └────────────────────┘

GTK thread (tray-icon)
  gtk::main() loop, MenuEvent → tokio mpsc
```

The workspace under `crates/` is organized as:

| Crate | Role |
|---|---|
| `f9-talk-core` | Shared constants (frame size, sample rate, channel capacity) |
| `f9-talk-input` | Hotkey-listener chord parser with 50 ms auto-repeat debounce; typer dispatcher (xdotool, clipboard, uinput) |
| `f9-talk-audio` | cpal mic streamer with linear resampler and RMS extraction for the wave indicator |
| `f9-talk-stt` | `Stt` trait, Deepgram Nova-3 streaming client, whisper.cpp local backend |
| `f9-talk-ui` | egui indicator viewport, tray icon, API-keys dialog, X11 positioner |
| `f9-talk-translate` | Lingva primary client with MyMemory fallback |
| `f9-talk` (binary) | clap CLI, secrets loader, abstract-socket lock, glue |

Reliability mechanisms:

- WebSocket auto-reconnect on socket close and on three consecutive
  send failures. Backoff resets after a healthy connection drops.
- Mic auto-restart on cpal stream errors with the same backoff.
- Wake-from-suspend detection via 5 s polling that flags clock drift
  greater than 30 s and reconnects the STT client.
- Permission preflight at startup that prints actionable instructions
  and exits non-zero if the `input` group or `/dev/uinput` access is
  missing.
- Single-instance lock on the abstract Unix socket
  `\0f9-talk-instance-lock`.

## Troubleshooting

| Symptom | Resolution |
|---|---|
| `/dev/uinput is not writable` | The `.deb` adds the user to the `input` group, but the GUI session must restart for it to take effect. Log out and back in once. |
| Tray icon invisible on vanilla GNOME | `sudo apt install gnome-shell-extension-appindicator`. Pop!_OS (on GNOME), Ubuntu, and KDE ship native support. |
| Tray icon invisible on Pop!_OS COSMIC | COSMIC does not yet ship a StatusNotifierItem applet, so the tray menu has nowhere to render. Configure the Deepgram key directly in `~/.config/F9_talk/secrets.env`; everything else works without the tray. Track [pop-os/cosmic-applets#status-area](https://github.com/pop-os/cosmic-applets) for native support. |
| `no speech detected` | Hold F9 for at least 0.3 s before releasing. |
| Wrong characters under non-en-US layout | Verify `xdotool` is installed; the typer logs `primary=xdotool` at startup. |
| "Already running" with no visible window | `pkill -f /usr/bin/f9-talk` and relaunch. |
| `wgpu` panic at startup | The shipped binary uses the OpenGL `glow` renderer; do not mix CUDA and wgpu in custom builds. |

Logs are available via `journalctl --user -t f9-talk -f`. Per-press
latency lines use the target `f9_talk::press`.

## License

MIT.
