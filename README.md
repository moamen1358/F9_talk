# 🎙️ f9-talk

**Hold F9, speak, release → text types at your cursor.** Linux dictation in any focused app (browser, editor, Slack, terminal).

```
F9 down → 🎙  Listening
F9 up   → ⌨  Typing → text appears
```

⚡ ~300 ms from key-release to text on screen.

---

## 🚀 Quick start

```bash
# 1. Install
git clone https://github.com/moamen1358/F9_talk.git
cd F9_talk
python3 -m venv .venv && .venv/bin/pip install -e .
sudo apt install pulseaudio-utils xdotool libxcb-cursor0

# 2. Add your Deepgram key  →  https://console.deepgram.com/signup  (free $200)
mkdir -p ~/.config/F9_talk
echo "DEEPGRAM_API_KEY=your_key_here" > ~/.config/F9_talk/secrets.env
chmod 600 ~/.config/F9_talk/secrets.env

# 3. Run
./run.sh
```

Hold **F9**, speak, release. Done.

---

## ⌨️ Run modes

| Command                                      | What it does                          |
| -------------------------------------------- | ------------------------------------- |
| `./run.sh`                                   | 🎤 Cloud STT, type English            |
| `./run.sh --target ar`                       | 🌍 Speak English → type Arabic        |
| `./run.sh --keyword Anthropic --keyword kubectl` | 🎯 Boost custom terms              |
| `./run.sh --local-hotkey '<ctrl>+<alt>+space'` | ⚡ Custom hotkey                    |
| `./run.sh --backend local`                   | 🔒 Local Whisper on GPU (offline)     |
| `./run.sh --backend both`                    | 🆚 F9=local · F8=cloud (compare)      |
| `./run.sh --style ripple`                    | 🎨 Indicator style: `wave` `bars` `pulse` `dots` `ripple` `blob` |
| `./run.sh -v`                                | 🐛 Verbose / debug                    |

For the local backend, install GPU deps once: `.venv/bin/pip install -e '.[local]'`

---

## 🔧 Troubleshooting

| Problem                                    | Fix                                                     |
| ------------------------------------------ | ------------------------------------------------------- |
| `DEEPGRAM_API_KEY not set`                 | Put it in `~/.config/F9_talk/secrets.env`               |
| `parec not found`                          | `sudo apt install pulseaudio-utils`                     |
| `No type-injection tool found`             | `sudo apt install xdotool`                              |
| Text appears in terminal, not the app      | Click into the target input **before** pressing F9      |
| `no speech detected`                       | Clip too short (<0.2 s). Speak a bit longer.            |
| CUDA errors on `--backend local`           | `.venv/bin/pip install -e '.[local]'`                   |

---

## 📋 Requirements

- 🐧 Linux (X11) · Python 3.10+
- 🎤 PulseAudio / PipeWire
- 🔑 Deepgram API key (cloud) **or** NVIDIA GPU (local)

---

## 📜 License

MIT — see [LICENSE](LICENSE).
