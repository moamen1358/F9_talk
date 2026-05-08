"""Unit tests for hotkey spec parsing."""
import pytest
from pynput import keyboard

from f9_talk.input.hotkey import canonical_key, parse_hotkey


# ---------- parse_hotkey ----------

def test_bare_function_key():
    assert keyboard.Key.f9 in parse_hotkey("f9")


def test_function_key_in_angle_brackets():
    assert keyboard.Key.f9 in parse_hotkey("<f9>")


def test_single_character_key():
    assert keyboard.KeyCode.from_char("a") in parse_hotkey("a")


def test_modifier_plus_key_combo():
    keys = parse_hotkey("<ctrl>+<alt>+<space>")
    assert keyboard.Key.ctrl in keys
    assert keyboard.Key.alt in keys
    assert keyboard.Key.space in keys


def test_all_named_modifiers():
    for spec, expected in [
        ("<ctrl>", keyboard.Key.ctrl),
        ("<alt>", keyboard.Key.alt),
        ("<shift>", keyboard.Key.shift),
        ("<space>", keyboard.Key.space),
        ("<tab>", keyboard.Key.tab),
        ("<enter>", keyboard.Key.enter),
        ("<esc>", keyboard.Key.esc),
    ]:
        assert expected in parse_hotkey(spec)


def test_spec_is_case_insensitive():
    assert keyboard.Key.f9 in parse_hotkey("F9")
    assert keyboard.Key.ctrl in parse_hotkey("<CTRL>")


def test_empty_spec_raises_value_error():
    with pytest.raises(ValueError, match="Empty hotkey"):
        parse_hotkey("")


def test_unknown_angle_bracket_key_raises():
    with pytest.raises(ValueError, match="Unknown special key"):
        parse_hotkey("<xyz>")


def test_unknown_word_raises():
    with pytest.raises(ValueError, match="Cannot parse hotkey part"):
        parse_hotkey("ctrl")


# ---------- canonical_key ----------

@pytest.mark.parametrize("variant,expected", [
    (keyboard.Key.ctrl_l, keyboard.Key.ctrl),
    (keyboard.Key.ctrl_r, keyboard.Key.ctrl),
    (keyboard.Key.alt_l, keyboard.Key.alt),
    (keyboard.Key.alt_r, keyboard.Key.alt),
    (keyboard.Key.alt_gr, keyboard.Key.alt),
    (keyboard.Key.shift_l, keyboard.Key.shift),
    (keyboard.Key.shift_r, keyboard.Key.shift),
    (keyboard.Key.cmd_l, keyboard.Key.cmd),
    (keyboard.Key.cmd_r, keyboard.Key.cmd),
])
def test_canonical_normalizes_left_right_modifiers(variant, expected):
    assert canonical_key(variant) == expected


def test_canonical_passes_through_non_modifier():
    assert canonical_key(keyboard.Key.f9) == keyboard.Key.f9


def test_canonical_passes_through_regular_key():
    key = keyboard.KeyCode.from_char("a")
    assert canonical_key(key) == key
