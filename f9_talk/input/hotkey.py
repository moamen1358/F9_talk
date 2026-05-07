"""Hotkey parsing helpers built on pynput's keyboard primitives."""
from __future__ import annotations

from pynput import keyboard

_SPECIAL = {
    "ctrl": keyboard.Key.ctrl,
    "alt": keyboard.Key.alt,
    "shift": keyboard.Key.shift,
    "cmd": keyboard.Key.cmd,
    "space": keyboard.Key.space,
    "tab": keyboard.Key.tab,
    "enter": keyboard.Key.enter,
    "esc": keyboard.Key.esc,
}

_LR_ALIASES = {
    keyboard.Key.ctrl_l: keyboard.Key.ctrl,
    keyboard.Key.ctrl_r: keyboard.Key.ctrl,
    keyboard.Key.alt_l: keyboard.Key.alt,
    keyboard.Key.alt_r: keyboard.Key.alt,
    keyboard.Key.alt_gr: keyboard.Key.alt,
    keyboard.Key.shift_l: keyboard.Key.shift,
    keyboard.Key.shift_r: keyboard.Key.shift,
    keyboard.Key.cmd_l: keyboard.Key.cmd,
    keyboard.Key.cmd_r: keyboard.Key.cmd,
}


def parse_hotkey(spec: str) -> set:
    """Parse a hotkey string like ``f9`` or ``<ctrl>+<alt>+space`` into a set of pynput keys."""
    result: set = set()
    for part in spec.lower().split("+"):
        p = part.strip()
        if not p:
            continue
        if p.startswith("<") and p.endswith(">"):
            name = p[1:-1]
            if name in _SPECIAL:
                result.add(_SPECIAL[name])
            elif name.startswith("f") and name[1:].isdigit():
                result.add(getattr(keyboard.Key, name))
            else:
                raise ValueError(f"Unknown special key: <{name}>")
        elif len(p) == 1:
            result.add(keyboard.KeyCode.from_char(p))
        elif p.startswith("f") and p[1:].isdigit():
            result.add(getattr(keyboard.Key, p))
        else:
            raise ValueError(f"Cannot parse hotkey part: {p!r}")
    if not result:
        raise ValueError(f"Empty hotkey: {spec!r}")
    return result


def canonical_key(key) -> object:
    """Collapse left/right modifier variants so press/release tracking is consistent."""
    return _LR_ALIASES.get(key, key)
