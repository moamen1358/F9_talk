"""Inject text at the current cursor position."""
from __future__ import annotations

import logging
import shutil
import subprocess
import time

log = logging.getLogger(__name__)


class Typer:
    """Synthesizes keystrokes via xdotool / wtype / ydotool.

    Strategy: ``type_text(text)`` types each character via ``xdotool type --delay 0``.
    Reliable across X11 apps, no clipboard side-effects. ~5 ms per character.
    """

    def __init__(self) -> None:
        self._tool: str | None = None
        for cmd in ("xdotool", "wtype", "ydotool"):
            if shutil.which(cmd) is not None:
                self._tool = cmd
                break
        if self._tool is None:
            log.warning(
                "No type-injection tool found (xdotool/wtype/ydotool). "
                "Falling back to stdout. Install xdotool: sudo apt install xdotool"
            )

    def type_text(self, text: str) -> None:
        text = (text or "").strip()
        if not text:
            return
        if self._tool == "xdotool":
            time.sleep(0.08)  # let the hotkey fully release before typing
            subprocess.run(
                ["xdotool", "type", "--clearmodifiers", "--delay", "0", "--", text],
                check=False,
            )
        elif self._tool == "wtype":
            subprocess.run(["wtype", "--", text], check=False)
        elif self._tool == "ydotool":
            subprocess.run(["ydotool", "type", text], check=False)
        else:
            print(text, flush=True)
