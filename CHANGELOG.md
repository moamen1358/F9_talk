# Changelog

All notable changes are documented here. Versions follow [Semantic Versioning](https://semver.org/).

---

## [0.3.1] — 2026-05-08

### Fixed
- **Deepgram WebSocket auto-reconnect**: after long uptime (~1 hour) the persistent Deepgram socket would close silently. F9 presses still showed the indicator animation but no audio reached the server (`finalize failed` in journalctl) and no text was typed — only an app restart recovered it. The close handler now kicks off a reconnect loop with exponential backoff (1 s → 30 s cap) and sessions resume automatically. `stop()` sets a shutdown flag so clean teardown does not trigger reconnect spam (`f9_talk/stt/deepgram.py`)

---

## [0.3.0] — 2026-05-08

### Removed
- **BREAKING**: AssemblyAI cloud backend removed entirely. Universal Streaming v3's end-of-turn detection requires sustained silence after speech, but our hold-to-talk sessions close as soon as F9 is released. Multiple workarounds (`speech_model="universal-streaming-english"`, `format_turns=True`, partial-transcript fallback, lower `min_end_of_turn_silence_when_confident`) didn't produce reliable results for sub-2-second holds. Removed rather than ship a flaky provider
- `assemblyai` Python dependency dropped from `pyproject.toml`
- `ASSEMBLYAI_API_KEY` no longer read or honored
- AssemblyAI option removed from the tray's **Cloud provider** submenu
- AssemblyAI field removed from the **API Keys…** dialog (two fields now: Deepgram + Gladia)

### Migration
- If you had `ASSEMBLYAI_API_KEY` in `~/.config/F9_talk/secrets.env`, it's safe to delete that line — the app ignores it
- Deepgram remains the default and recommended backend; Gladia stays as the alternative

### Added
- `websockets>=12` is now an explicit dependency (was previously transitive)

---

## [0.2.3] — 2026-05-08

### Fixed
- AssemblyAI: rejected our 25 ms mic frames with `Input Duration Violation: 25.0 ms. Expected between 50 and 1000 ms`. The AssemblyAI backend now buffers incoming audio frames until at least 50 ms (1600 bytes at 16 kHz mono int16) is available before yielding to the SDK. Deepgram/Gladia paths unaffected — they accept smaller chunks

---

## [0.2.2] — 2026-05-08

### Fixed
- AssemblyAI: `speech_model="universal"` was rejected as an invalid enum value in SDK 0.64. The accepted English value is `"universal-streaming-english"` (others: `universal-streaming-multilingual`, `u3-rt-pro`, `whisper-rt`, `u3-pro`)

---

## [0.2.1] — 2026-05-08

### Fixed
- AssemblyAI streaming sessions failed at connect with `1 validation error for StreamingParameters` after the `assemblyai` SDK 0.64 made `speech_model` a required field. v0.2.1 passed `speech_model="universal"` but that value is itself rejected — see v0.2.2 for the working fix

---

## [0.2.0] — 2026-05-08

### Added
- **Multi-provider cloud STT**: tray submenu lets you switch live between Deepgram Nova-3 (default), AssemblyAI Universal, and Gladia v2 — each backend implements the same `begin_session` / `send_audio` / `end_session` protocol
- **System tray icon** with three states (active / paused / error) and a right-click menu: Pause/Resume listening, Cloud provider ▶, API Keys…, Quit. Left-click toggles pause
- **API Keys dialog** reachable from the tray menu — three masked fields (Deepgram / AssemblyAI / Gladia) with per-field Show toggle. Save persists to `~/.config/F9_talk/secrets.env` preserving comments + unrelated entries; takes effect immediately (Deepgram reconnects, others pick up on next session)
- **Error feedback**: failed STT sessions now pop a desktop notification with the provider error message and turn the tray icon red until the next successful session
- **Spec-driven design docs** under `docs/superpowers/specs/` for tray icon, API Keys dialog

### Changed
- Default Deepgram model upgraded from `nova-2` to `nova-3` — ~30% lower WER, sub-300 ms streaming, marginal cost increase (~$0.46/hr)

### Fixed
- Recording indicator invisible on multi-monitor setups — async repositioning never dispatched back to the Qt main thread because `QTimer.singleShot` was called from a plain Python worker; reverted to synchronous `_reposition()` before `show()`
- Tray icon was empty / unrecognizable in GNOME — bundled the PNG/SVG assets into the package (`pyproject.toml` `package-data`), set `QIcon.fromTheme("f9-talk")` for AppIndicator extensions, set proper application name via `QApplication.setApplicationName("F9 Talk")`

### Tests
- Up to 109 unit tests (57 → 109): tray UI, app pause/provider/reload behavior, API Keys dialog, secrets file load/save, AssemblyAI/Gladia backends

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
- Recording indicator invisible on multi-monitor setups — async repositioning attempt left the widget at default (0,0) on a different screen because `QTimer.singleShot` from a non-Qt worker thread never dispatched back to the main thread; reverted to synchronous `_reposition()` before `show()`
