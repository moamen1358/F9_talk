# System Tray Icon for F9 Talk — Design

Date: 2026-05-08
Status: Approved (verbal)

## Problem

f9-talk runs as a hidden background process. Users have no visible signal that
the app is active, no way to pause F9 capture without killing the process, and
no quick exit path other than the command line. After the indicator-visibility
regression earlier today, the user explicitly asked for a status indicator and
an on/off toggle.

## Goals

- A persistent, always-visible signal that f9-talk is running and listening.
- A one-click way to pause F9 capture (without quitting).
- A one-click way to quit cleanly.

## Non-goals

- Settings dialog (style, hotkey, target language remain CLI flags).
- Persisted pause state across restarts.
- Recording-state animation on the tray icon (the existing wave indicator
  already shows that).
- Wayland-native indicator. Pop!_OS is X11; tray works natively. On Wayland
  sessions we degrade gracefully to "no icon, app still works".

## UX

### States

| State | Tray icon | Tooltip |
|---|---|---|
| Active (listening) | full-color `f9-talk.png` | `F9 Talk — Listening` |
| Paused | grayscale (50% opacity) | `F9 Talk — Paused` |

### Interactions

| Input | Action |
|---|---|
| Left-click tray icon | Toggle Active ⇄ Paused |
| Right-click tray icon | Open menu |

### Right-click menu

```
Pause listening   ← label flips with state
─────────────────
Quit
```

(Label becomes "Resume listening" when paused.)

### Recording flow (unchanged)

The existing wave indicator (`DictateIndicator`) still appears during F9 hold.
The tray icon does not animate during recording — the active/paused distinction
is the only state it conveys.

## Architecture

### New file: `f9_talk/ui/tray.py`

```
class DictateTray(QSystemTrayIcon):
    pause_changed = Signal(bool)     # emits True when paused, False when active
    quit_requested = Signal()

    def __init__(self, qapp): ...
    def toggle_pause(self): ...
    def set_paused(self, paused: bool): ...
    def _build_menu(self): ...
    def _build_icons(self) -> tuple[QIcon, QIcon]:
        # returns (active_icon, paused_icon) — paused is grayscale + 50% alpha
```

The tray owns its pause state. It emits `pause_changed` whenever the state
flips (whether via left-click or programmatic call). The application connects
to that signal.

### Hook in `f9_talk/cli.py`

After `DictateApp` is constructed and `dictate.start()` is queued:

```python
tray = DictateTray(qapp) if QSystemTrayIcon.isSystemTrayAvailable() else None
if tray is not None:
    tray.pause_changed.connect(dictate.set_paused)
    tray.quit_requested.connect(qapp.quit)
    tray.show()
else:
    log.warning("System tray not available; running without tray icon.")
```

### Change in `f9_talk/app.py`

Add `self._paused = False` in `__init__`. Add the public method:

```python
def set_paused(self, paused: bool) -> None:
    self._paused = paused
```

Add a single guard at the top of `_on_press`:

```python
def _on_press(self, key) -> None:
    if self._paused:
        return
    canon = canonical_key(key)
    ...
```

The guard sits *before* the canonical-key computation so a paused F9 hold
costs essentially nothing and never starts a session. Auto-repeat handling,
debounce timer, and existing release logic are untouched — paused means
"ignore presses entirely."

### Why guard only press, not release?

If the user pauses *while* holding F9, the in-flight session keeps running
until release — that's correct (cutting off mid-utterance would lose audio).
After release, the session ends normally and no new sessions start.

## Files touched

| File | Change |
|---|---|
| `f9_talk/ui/tray.py` | New — `DictateTray` class |
| `f9_talk/ui/__init__.py` | Export `DictateTray` |
| `f9_talk/cli.py` | Instantiate tray, wire signals |
| `f9_talk/app.py` | `_paused` field + `set_paused()` + early-return in `_on_press` |
| `tests/unit/test_tray.py` | New — pause/menu/icon tests |
| `tests/unit/test_session.py` | New test: paused press is a no-op |
| `CHANGELOG.md` | "Added: tray icon with pause/resume toggle" |

## Edge cases & decisions

- **Tray unavailable** (Wayland-only, headless CI): log a warning, run without
  tray. The app still works; this is degraded UX, not an error.
- **Pause during in-flight session**: don't cut off; finish the session, then
  ignore subsequent presses.
- **Toggle from different threads**: pause state is a plain bool. Reads in
  `_on_press` (pynput thread) and writes in `set_paused` (Qt main thread) are
  atomic in CPython. No lock needed.
- **Icon assets**: reuse `f9_talk/assets/f9-talk.png`. Build the paused
  variant in code via `QImage` grayscale conversion + alpha multiply, so we
  don't ship a second asset file.
- **Quit path**: tray Quit emits `quit_requested` → `qapp.quit()`. Existing
  teardown in `cli.main()` runs `dictate.stop()` after the event loop exits.

## Tests

`tests/unit/test_tray.py`:

1. New tray starts active (`is_paused() == False`).
2. `toggle_pause()` flips to paused; emits `pause_changed(True)`.
3. Second `toggle_pause()` flips back; emits `pause_changed(False)`.
4. Menu has exactly 2 visible actions: pause/resume + quit.
5. Menu first-action label is `Pause listening` when active, `Resume listening`
   when paused.
6. `quit_requested` is emitted when Quit action is triggered.

All tests run under a single `QApplication` instance created in a
`conftest.py` fixture (no new dev dependency — `PySide6` already provides
`QApplication`).

`tests/unit/test_session.py` (extension):

7. `app.set_paused(True)` then simulating a press → no `_begin_session` call.
8. Toggle back to False → next press starts a session normally.

## Acceptance

**Manual smoke test (must pass before merge):**

1. Build .deb, install, launch from app menu
2. Tray icon appears in top-right of Pop!_OS panel
3. Hover → tooltip "F9 Talk — Listening"
4. Hold F9, speak → text appears as today
5. Left-click tray icon → icon dims, tooltip "F9 Talk — Paused"
6. Hold F9, speak → no text appears, no recording log
7. Left-click again → icon restored, F9 works
8. Right-click → menu shows pause/resume + quit
9. Click Quit → app exits, tray icon disappears

**Automated:** all 8 new unit tests green; existing 57 tests still green;
`ruff check` clean.
