# Contributing to f9-talk

Thank you for your interest. This guide covers everything you need to submit a quality contribution.

## Development setup

```bash
git clone https://github.com/moamen1358/F9_talk.git
cd F9_talk

python3 -m venv .venv
.venv/bin/pip install -e '.[dev]'

sudo apt install pulseaudio-utils xdotool libxcb-cursor0
```

For the local Whisper backend (optional, requires NVIDIA GPU):

```bash
.venv/bin/pip install -e '.[local]'
```

## Running tests

```bash
.venv/bin/pytest tests/ -v
```

Tests are organised under `tests/unit/`. They use `unittest.mock` to avoid requiring real audio hardware, a Deepgram API key, or a display.

## Linting

```bash
.venv/bin/ruff check f9_talk/
.venv/bin/ruff format --check f9_talk/
```

Both checks run automatically on every push and pull request via GitHub Actions.

## Project structure

```
f9_talk/
├── app.py          — orchestration: hotkey → STT → translate → type
├── cli.py          — argument parsing and application bootstrap
├── audio/          — PulseAudio mic capture (parec subprocess)
├── input/          — hotkey parsing and xdotool keystroke injection
├── stt/            — Deepgram cloud and local Whisper backends
├── translate/      — HTTP translation (Lingva + MyMemory fallback)
└── ui/             — audio-reactive overlay indicator (PySide6/Qt)

tests/
└── unit/           — fast, hardware-free unit tests
```

See `docs/architecture.md` for a detailed threading model and latency budget.

## Submitting a pull request

1. Fork the repository and create a branch from `main`
2. Make your changes with tests where appropriate
3. Ensure `ruff check` and `pytest` both pass locally
4. Open a pull request — describe what changed and why

## Commit style

Use the imperative mood and keep the subject line under 72 characters:

```
Fix garbled typing when modifier keys are held
Add --style flag for indicator animation
```

Reference issues with `Fixes #N` in the commit body when applicable.
