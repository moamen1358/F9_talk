"""Command-line entry point. Runs `f9-talk` console script."""
from __future__ import annotations

import argparse
import logging
import os
import sys
from pathlib import Path

from PySide6.QtCore import QTimer
from PySide6.QtWidgets import QApplication

from f9_talk import __version__
from f9_talk.app import DictateApp
from f9_talk.ui import DictateIndicator


def _load_env_files() -> None:
    """Best-effort load of `~/.config/F9_talk/secrets.env` and `./.env`."""
    candidates = [
        Path.home() / ".config" / "F9_talk" / "secrets.env",
        Path.cwd() / ".env",
    ]
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


def main() -> int:
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

    indicator = DictateIndicator(style=args.style)
    dictate = DictateApp(
        indicator=indicator,
        local_hotkey=args.local_hotkey,
        cloud_hotkey=args.cloud_hotkey,
        target_lang=args.target,
        keywords=args.keyword,
        backend=args.backend,
    )

    QTimer.singleShot(0, dictate.start)

    rc = qapp.exec()
    dictate.stop()
    return rc


if __name__ == "__main__":
    raise SystemExit(main())
