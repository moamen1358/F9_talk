# Changelog

All notable changes are documented here. Versions follow [Semantic Versioning](https://semver.org/).

---

## [0.1.0] — 2026-05-08

### Added
- Hold-to-talk dictation via Deepgram Nova-2 cloud STT (~300 ms latency)
- Local offline backend via faster-whisper (GPU required)
- Audio-reactive overlay indicator with six animation styles: `wave`, `bars`, `pulse`, `dots`, `ripple`, `blob`
- Real-time speech-to-text translation to Arabic and other languages
- Custom hotkey support — any key combination, not just F9
- Dual-backend mode: F9 = local Whisper, F8 = Deepgram cloud (side-by-side comparison)
- Domain keyword boosting for proper nouns and jargon
- Debian package with autostart, icon integration, and first-run API key setup
- Automated `.deb` releases via GitHub Actions on version tags
- 57 unit tests covering typer, session state machine, hotkey parsing, and translation backends
- CI pipeline: lint (ruff) + test matrix (Python 3.10–3.12) on every push and pull request
- Dependabot for automated dependency updates
- `SECURITY.md`, `CONTRIBUTING.md`, `CHANGELOG.md`

### Fixed
- **Critical:** `_acquire_lock()` socket was garbage collected immediately, allowing multiple instances to run simultaneously — socket now held at module level
- **Security:** language code path injection in Lingva translator URL — codes now validated against `[a-zA-Z]{2,5}`
- **Security:** API key input in setup dialog could corrupt `secrets.env` with embedded newlines or `=` characters
- Garbled text output when modifier keys (Shift, F9) were held during xdotool injection — fixed with `--clearmodifiers`
- Per-character typing delay (`--delay 20`) caused visible word-by-word output — reverted to `--delay 0`
- Audio glitches during long recordings — increased parec latency buffer from 15 ms to 100 ms
- Indicator positioning blocked the Qt main thread for up to 500 ms — subprocess calls moved to a background thread
