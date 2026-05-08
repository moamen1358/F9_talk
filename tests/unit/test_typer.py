"""Unit tests for the xdotool keystroke injector."""
from unittest.mock import patch

import pytest

from f9_talk.input.typer import Typer


@pytest.fixture
def typer():
    t = Typer.__new__(Typer)
    t._tool = "xdotool"
    return t


def test_empty_string_is_ignored(typer):
    with patch("subprocess.run") as mock_run, patch("time.sleep"):
        typer.type_text("")
        mock_run.assert_not_called()


def test_whitespace_only_is_ignored(typer):
    with patch("subprocess.run") as mock_run, patch("time.sleep"):
        typer.type_text("   \t\n")
        mock_run.assert_not_called()


def test_xdotool_called_with_correct_flags(typer):
    with patch("subprocess.run") as mock_run, patch("time.sleep"):
        typer.type_text("hello world")
        mock_run.assert_called_once_with(
            ["xdotool", "type", "--clearmodifiers", "--delay", "0", "--", "hello world"],
            check=False,
        )


def test_text_is_stripped_before_injection(typer):
    with patch("subprocess.run") as mock_run, patch("time.sleep"):
        typer.type_text("  hello  ")
        args = mock_run.call_args[0][0]
        assert args[-1] == "hello"


def test_unicode_text_passes_through(typer):
    with patch("subprocess.run") as mock_run, patch("time.sleep"):
        typer.type_text("مرحبا بالعالم")
        args = mock_run.call_args[0][0]
        assert "مرحبا بالعالم" in args


def test_special_characters_pass_through(typer):
    with patch("subprocess.run") as mock_run, patch("time.sleep"):
        typer.type_text("hello! @#$%")
        mock_run.assert_called_once()


def test_pre_typing_sleep_is_80ms(typer):
    with patch("subprocess.run"), patch("time.sleep") as mock_sleep:
        typer.type_text("test")
        mock_sleep.assert_called_once_with(0.08)


def test_fallback_to_stdout_when_no_tool(capsys):
    t = Typer.__new__(Typer)
    t._tool = None
    t.type_text("hello from stdout")
    assert capsys.readouterr().out.strip() == "hello from stdout"


def test_wtype_command(monkeypatch):
    t = Typer.__new__(Typer)
    t._tool = "wtype"
    with patch("subprocess.run") as mock_run:
        t.type_text("hello")
        mock_run.assert_called_once_with(["wtype", "--", "hello"], check=False)
