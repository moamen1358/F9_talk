"""Command-line entry point. Runs `f9-talk` console script."""
from __future__ import annotations

import argparse
import logging
import os
import sys
from pathlib import Path

from PySide6.QtCore import Qt, QTimer
from PySide6.QtWidgets import (
    QApplication,
    QDialog,
    QDialogButtonBox,
    QLabel,
    QLineEdit,
    QVBoxLayout,
)

from f9_talk import __version__
from f9_talk.app import DictateApp
from f9_talk.ui import APIKeysDialog, DictateIndicator, DictateTray

_SECRETS_FILE = Path.home() / ".config" / "F9_talk" / "secrets.env"

_PROVIDER_ENV = {
    "deepgram":   "DEEPGRAM_API_KEY",
    "assemblyai": "ASSEMBLYAI_API_KEY",
    "gladia":     "GLADIA_API_KEY",
}


def _load_secrets(path: Path = _SECRETS_FILE) -> dict[str, str]:
    """Return current values for the three managed providers from a .env file."""
    if not path.exists():
        return {}
    out: dict[str, str] = {}
    env_to_provider = {v: k for k, v in _PROVIDER_ENV.items()}
    try:
        for line in path.read_text().splitlines():
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            k, v = line.split("=", 1)
            k, v = k.strip(), v.strip().strip('"').strip("'")
            if k in env_to_provider:
                out[env_to_provider[k]] = v
    except OSError:
        pass
    return out


def _save_secrets(edits: dict[str, str], path: Path = _SECRETS_FILE) -> None:
    """Write the given provider→key edits, preserving every other line."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.parent.chmod(0o700)

    new_lines = {_PROVIDER_ENV[p]: f"{_PROVIDER_ENV[p]}={v}" for p, v in edits.items()}
    seen: set[str] = set()
    out_lines: list[str] = []

    if path.exists():
        for raw in path.read_text().splitlines():
            stripped = raw.strip()
            if "=" in stripped and not stripped.startswith("#"):
                key = stripped.split("=", 1)[0].strip()
                if key in new_lines:
                    out_lines.append(new_lines[key])
                    seen.add(key)
                    continue
            out_lines.append(raw)

    for env_key, line in new_lines.items():
        if env_key not in seen:
            out_lines.append(line)

    path.write_text("\n".join(out_lines) + "\n")
    path.chmod(0o600)


def _load_env_files() -> None:
    """Best-effort load of `~/.config/F9_talk/secrets.env` and `./.env`."""
    candidates = [_SECRETS_FILE, Path.cwd() / ".env"]
    for path in candidates:
        if not path.exists():
            continue
        try:
            for line in path.read_text().splitlines():
                line = line.strip()
                if not line or line.startswith("#") or "=" not in line:
                    continue
                k, v = line.split("=", 1)
                k, v = k.strip(), v.strip().strip('"').strip("'")
                if k and k not in os.environ:
                    os.environ[k] = v
        except OSError:
            pass


def _prompt_for_api_key() -> bool:
    """Show a first-run setup dialog asking for the Deepgram API key.

    Saves the key to ~/.config/F9_talk/secrets.env and sets it in os.environ.
    Returns True if the key was provided and saved, False if the user cancelled.
    """
    dlg = QDialog()
    dlg.setWindowTitle("F9 Talk — First-time Setup")
    dlg.setMinimumWidth(440)
    dlg.setWindowFlags(dlg.windowFlags() | Qt.WindowStaysOnTopHint)

    layout = QVBoxLayout(dlg)
    layout.setSpacing(14)
    layout.setContentsMargins(20, 20, 20, 20)

    info = QLabel(
        "<h3 style='margin:0'>Welcome to F9 Talk</h3>"
        "<p>Speech recognition requires a <b>Deepgram API key</b>.<br>"
        "It's free — create one at "
        "<a href='https://console.deepgram.com/signup'>console.deepgram.com</a>, "
        "then paste it below.</p>"
        "<p style='color:grey;font-size:small'>Your key is saved locally to<br>"
        f"<code>{_SECRETS_FILE}</code></p>"
    )
    info.setOpenExternalLinks(True)
    info.setWordWrap(True)
    layout.addWidget(info)

    key_input = QLineEdit()
    key_input.setPlaceholderText("Paste your Deepgram API key here…")
    key_input.setEchoMode(QLineEdit.Password)
    key_input.setMinimumHeight(32)
    layout.addWidget(key_input)

    buttons = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
    buttons.button(QDialogButtonBox.Ok).setText("Save and Start")
    buttons.accepted.connect(dlg.accept)
    buttons.rejected.connect(dlg.reject)
    layout.addWidget(buttons)

    if dlg.exec() != QDialog.Accepted:
        return False

    key = key_input.text().strip().replace("\n", "").replace("\r", "")
    if not key:
        return False
    if "=" in key or len(key) > 512:
        return False

    # Write key into secrets.env (create or update existing line)
    _SECRETS_FILE.parent.mkdir(parents=True, exist_ok=True)
    _SECRETS_FILE.parent.chmod(0o700)

    if _SECRETS_FILE.exists():
        lines = _SECRETS_FILE.read_text().splitlines()
        updated = False
        for i, line in enumerate(lines):
            if line.startswith("DEEPGRAM_API_KEY="):
                lines[i] = f"DEEPGRAM_API_KEY={key}"
                updated = True
                break
        if not updated:
            lines.append(f"DEEPGRAM_API_KEY={key}")
        _SECRETS_FILE.write_text("\n".join(lines) + "\n")
    else:
        _SECRETS_FILE.write_text(f"DEEPGRAM_API_KEY={key}\n")

    _SECRETS_FILE.chmod(0o600)
    os.environ["DEEPGRAM_API_KEY"] = key
    return True


_instance_lock: "socket.socket | None" = None  # held for the process lifetime


def _acquire_lock() -> bool:
    """Return True if this is the only running instance, False if another is already running."""
    import socket as _socket
    global _instance_lock
    try:
        _instance_lock = _socket.socket(_socket.AF_UNIX, _socket.SOCK_DGRAM)
        _instance_lock.bind("\0f9-talk-instance-lock")
        return True
    except OSError:
        return False


def main() -> int:
    if not _acquire_lock():
        print("f9-talk is already running.", file=sys.stderr)
        return 0
    _load_env_files()

    p = argparse.ArgumentParser(
        prog="f9-talk",
        description="Hold-to-talk dictation. Speak, release, text appears at your cursor.",
    )
    p.add_argument(
        "--backend",
        default="cloud",
        choices=["cloud", "local", "both"],
        help="STT backend(s) to load (default: cloud).",
    )
    p.add_argument(
        "--local-hotkey",
        default="f9",
        help="Hold for LOCAL Whisper STT (default: f9). Examples: f9, <ctrl>+<alt>+space.",
    )
    p.add_argument(
        "--cloud-hotkey",
        default="f8",
        help="Hold for DEEPGRAM cloud STT (default: f8). Used only with --backend both.",
    )
    p.add_argument(
        "--target",
        default=None,
        help="Translate to this language code before typing (e.g. ar). Omit for raw English.",
    )
    p.add_argument(
        "--keyword",
        action="append",
        default=[],
        help="Boost a domain-specific term (repeatable). Helps with proper nouns / jargon.",
    )
    p.add_argument(
        "--style",
        default="wave",
        choices=list(DictateIndicator.STYLES),
        help="Indicator animation style (default: wave).",
    )
    p.add_argument("-v", "--verbose", action="store_true", help="Debug logging.")
    p.add_argument("-V", "--version", action="version", version=f"f9-talk {__version__}")
    args = p.parse_args()

    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s %(levelname)-7s %(name)s: %(message)s",
        datefmt="%H:%M:%S",
    )

    # Pass only argv[0] so QApplication doesn't try to interpret our argparse args
    qapp = QApplication.instance() or QApplication([sys.argv[0]])
    qapp.setQuitOnLastWindowClosed(False)
    qapp.setApplicationName("F9 Talk")
    qapp.setApplicationDisplayName("F9 Talk")
    qapp.setDesktopFileName("f9-talk")

    # First-run setup: if the cloud backend is needed but no key is configured, ask for it
    if args.backend in ("cloud", "both") and not os.environ.get("DEEPGRAM_API_KEY"):
        if not _prompt_for_api_key():
            logging.getLogger(__name__).error("No Deepgram API key provided. Exiting.")
            return 1

    indicator = DictateIndicator(style=args.style)

    # Lazy holders — populated below if the tray is available so the callbacks
    # can reach the live tray instance even though DictateApp is built first.
    _tray_ref: dict[str, DictateTray | None] = {"tray": None}

    def _on_stt_error(msg: str) -> None:
        t = _tray_ref["tray"]
        if t is not None:
            t.show_error(msg)

    def _on_stt_success() -> None:
        t = _tray_ref["tray"]
        if t is not None:
            t.clear_error()

    dictate = DictateApp(
        indicator=indicator,
        local_hotkey=args.local_hotkey,
        cloud_hotkey=args.cloud_hotkey,
        target_lang=args.target,
        keywords=args.keyword,
        backend=args.backend,
        on_error=_on_stt_error,
        on_success=_on_stt_success,
    )

    from PySide6.QtWidgets import QSystemTrayIcon
    if QSystemTrayIcon.isSystemTrayAvailable():
        tray = DictateTray(
            qapp,
            assemblyai_available=dictate.cloud_stt_assemblyai is not None,
            gladia_available=dictate.cloud_stt_gladia is not None,
        )

        def _on_keys_edit() -> None:
            current = _load_secrets()
            dlg = APIKeysDialog(current)
            if dlg.exec() != QDialog.Accepted:
                return
            edits = dlg.edited_keys()
            if not edits:
                return
            try:
                _save_secrets(edits)
            except OSError as e:
                logging.getLogger(__name__).error("Failed to save secrets: %s", e)
                tray.show_error(f"Couldn't save keys: {e}")
                return
            for provider, value in edits.items():
                os.environ[_PROVIDER_ENV[provider]] = value
            dictate.reload_keys()

        tray.pause_changed.connect(dictate.set_paused)
        tray.provider_changed.connect(dictate.set_cloud_provider)
        tray.keys_edit_requested.connect(_on_keys_edit)
        tray.quit_requested.connect(qapp.quit)
        tray.show()
        _tray_ref["tray"] = tray
    else:
        logging.getLogger(__name__).warning(
            "System tray not available; running without tray icon."
        )

    QTimer.singleShot(0, dictate.start)

    rc = qapp.exec()
    dictate.stop()
    return rc


if __name__ == "__main__":
    raise SystemExit(main())
