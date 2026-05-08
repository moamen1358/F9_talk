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
- Multiple-instance prevention via abstract Unix socket lock
- Debian package with autostart, icon integration, and first-run API key setup
- Automated `.deb` releases via GitHub Actions on version tags

### Fixed
- Garbled text output when modifier keys (Shift, F9) were still held during xdotool injection — fixed with `--clearmodifiers`
- Audio glitches during long recordings — increased parec latency buffer from 15 ms to 100 ms
- Per-character typing delay causing visible word-by-word output — reverted unnecessary `--delay 20`
