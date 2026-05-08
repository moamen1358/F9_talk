# API Keys Dialog — Design

Date: 2026-05-08
Status: Approved (verbal)

## Problem

API keys for Deepgram, AssemblyAI, and Gladia live in
`~/.config/F9_talk/secrets.env`. To add or rotate a key the user must edit
the file by hand and restart f9-talk. That is friction every time a key
expires, gets rotated, or a new provider is added.

## Goals

- Add or replace any of the three cloud-provider API keys from a GUI dialog
  reachable via the tray menu.
- Take effect immediately — no app restart.
- Preserve other entries (comments, `MYMEMORY_EMAIL`, etc.) in the secrets
  file untouched.

## Non-goals

- Validating keys against the providers (we already surface errors via the
  existing toast + red-icon flow when a session fails).
- Multi-account / profile management.
- Storing keys anywhere besides `secrets.env`.

## UX

### Menu placement

`Pause/Resume listening`
`Cloud provider ▶`
`API Keys…`             ← new
`―`
`Quit`

### Dialog

```
F9 Talk — API Keys
──────────────────────────────────────────
Deepgram        [••••••••••••••] [👁 Show]
AssemblyAI      [••••••••••••••] [👁 Show]
Gladia          [••••••••••••••] [👁 Show]
──────────────────────────────────────────
                        [Cancel] [Save]
```

- Pre-populated with current values, masked by default (`QLineEdit.Password`).
- Per-field **Show** button toggles plaintext for that field only.
- An empty field on Save means "leave existing value untouched" (so the user
  can change one key without retyping the others).
- Save: commits all three, closes dialog. Cancel: discards, closes.

## Architecture

### New file: `f9_talk/ui/keys_dialog.py`

```python
class APIKeysDialog(QDialog):
    """Three-field dialog for editing cloud-provider API keys."""

    PROVIDERS = ("deepgram", "assemblyai", "gladia")
    LABELS = {
        "deepgram":   "Deepgram",
        "assemblyai": "AssemblyAI",
        "gladia":     "Gladia",
    }

    def __init__(self, current: dict[str, str]):
        ...

    def edited_keys(self) -> dict[str, str]:
        """Return only fields the user changed (non-empty values)."""
```

The dialog is a pure UI component — no I/O, no signals. The caller passes
in current values, runs `dlg.exec()`, then reads `edited_keys()`.

### Tray changes

```python
class DictateTray(QSystemTrayIcon):
    ...
    keys_edit_requested = Signal()  # NEW
```

A new menu action `"API Keys…"` is inserted between the cloud-provider
submenu and the Quit separator. Triggering it emits
`keys_edit_requested`.

### cli.py wiring

```python
def _on_keys_edit():
    current = _load_secrets()       # read existing key=value pairs
    dlg = APIKeysDialog(current)
    if dlg.exec() != QDialog.Accepted:
        return
    edits = dlg.edited_keys()
    if not edits:
        return
    _save_secrets(edits)            # merge + write back, preserve other entries
    for k, v in edits.items():
        os.environ[f"{k.upper()}_API_KEY"] = v
    dictate.reload_keys()           # update backend instances + reconnect Deepgram
```

`_load_secrets` and `_save_secrets` are private helpers in `cli.py`. The
load returns a `{provider: key}` dict (DEEPGRAM_API_KEY → "deepgram", etc.).
The save preserves every line that isn't one of the three managed keys —
comments, blank lines, `MYMEMORY_EMAIL`, future additions — by reading the
file as `lines`, replacing only the matching lines, and appending new ones
that didn't exist.

### DictateApp changes

```python
def reload_keys(self) -> None:
    """Pick up new keys from os.environ. Reconnect persistent backends."""
    for backend in (
        self.cloud_stt_deepgram,
        self.cloud_stt_assemblyai,
        self.cloud_stt_gladia,
    ):
        if backend is None:
            continue
        backend.api_key = os.environ.get(f"{backend.__class__._ENV_KEY}", "")
    if self.cloud_stt_deepgram is not None and not self._recording:
        self.cloud_stt_deepgram.stop()
        self.cloud_stt_deepgram.start()
```

Each backend class declares a class-level `_ENV_KEY` attribute (e.g.,
`DEEPGRAM_API_KEY`) so the reload logic stays generic. AssemblyAI and
Gladia open per-session, so just updating `api_key` is enough — the next
`begin_session` reads the current value. Deepgram needs a reconnect
because its `WebSocket` is already authenticated with the old key; we
guard against reconnecting mid-recording.

## Edge cases

- **Secrets file missing**: `_load_secrets` returns `{}`, dialog opens with
  empty fields. `_save_secrets` creates `~/.config/F9_talk/` (mode 0700)
  and the file (mode 0600) on first write.
- **Empty field on Save**: treated as "keep existing", not "delete". The
  dialog's `edited_keys()` filters out empty strings before returning.
- **Permission denied**: `_save_secrets` catches `OSError`, surfaces it via
  the existing `tray.show_error()` toast, leaves the dialog open.
- **Reload mid-session**: Deepgram `_recording` guard skips the reconnect
  until the in-flight session finishes. The user retries by editing again
  after release.
- **Same key re-entered**: idempotent; treated as a no-op for Deepgram
  (still triggers a needless reconnect, acceptable cost ~200ms).

## Tests

`tests/unit/test_keys_dialog.py`:

1. Dialog populates fields from `current` dict on construction.
2. Fields are masked by default (`echoMode == Password`).
3. Show button toggles a single field to `Normal` and back.
4. `edited_keys()` returns only fields the user changed.
5. `edited_keys()` skips empty fields (empty = keep existing).
6. Save action accepts the dialog.
7. Cancel rejects the dialog.

`tests/unit/test_app.py` (extension):

8. `reload_keys()` updates each backend's `api_key` attribute from env.
9. `reload_keys()` calls `stop()` then `start()` on Deepgram only when
   `_recording is False`.
10. `reload_keys()` skips Deepgram reconnect if `_recording is True`.

`tests/unit/test_secrets.py` (new):

11. `_load_secrets` returns the three managed keys from a sample file.
12. `_load_secrets` returns `{}` when the file does not exist.
13. `_save_secrets` updates an existing key in place.
14. `_save_secrets` appends a new key when not present.
15. `_save_secrets` preserves unrelated lines (`MYMEMORY_EMAIL`, comments).
16. `_save_secrets` creates the directory + file with correct permissions.

## Files touched

| File | Change |
|---|---|
| `f9_talk/ui/keys_dialog.py` | **New** — `APIKeysDialog` |
| `f9_talk/ui/__init__.py` | Export `APIKeysDialog` |
| `f9_talk/ui/tray.py` | Add menu item + `keys_edit_requested` signal |
| `f9_talk/cli.py` | Add `_load_secrets`, `_save_secrets`, signal handler |
| `f9_talk/app.py` | Add `reload_keys()` method |
| `f9_talk/stt/deepgram.py` | Add class-level `_ENV_KEY = "DEEPGRAM_API_KEY"` |
| `f9_talk/stt/assemblyai.py` | Add class-level `_ENV_KEY = "ASSEMBLYAI_API_KEY"` |
| `f9_talk/stt/gladia.py` | Add class-level `_ENV_KEY = "GLADIA_API_KEY"` |
| `tests/unit/test_keys_dialog.py` | **New** — 7 tests |
| `tests/unit/test_app.py` | +3 reload_keys tests |
| `tests/unit/test_secrets.py` | **New** — 6 tests for load/save helpers |

## Acceptance

**Manual smoke test (must pass before merge):**

1. Right-click tray → menu shows "API Keys…" item
2. Click it → dialog opens with existing keys populated and masked
3. Click Show on Deepgram → that field switches to plaintext
4. Edit Gladia, leave others empty, click Save → only Gladia gets written
5. `cat ~/.config/F9_talk/secrets.env` → Gladia changed, Deepgram and
   AssemblyAI untouched, `MYMEMORY_EMAIL` and comments preserved
6. Hold F9 (with Deepgram active) → still works (reconnected with same key)
7. Switch to Gladia in tray → hold F9 → uses the new Gladia key

**Automated:** all 16 new unit tests green; existing 89 tests still green;
ruff clean on new files.
