"""Unit tests for cli.py secrets load/save helpers."""
from __future__ import annotations

from pathlib import Path

from f9_talk.cli import _load_secrets, _save_secrets


def test_load_returns_three_managed_keys(tmp_path: Path):
    p = tmp_path / "secrets.env"
    p.write_text(
        "# comment\n"
        "DEEPGRAM_API_KEY=dg-key\n"
        "ASSEMBLYAI_API_KEY=aa-key\n"
        "GLADIA_API_KEY=gl-key\n"
        "MYMEMORY_EMAIL=user@example.com\n"
    )
    out = _load_secrets(p)
    assert out == {
        "deepgram":   "dg-key",
        "assemblyai": "aa-key",
        "gladia":     "gl-key",
    }


def test_load_missing_file_returns_empty(tmp_path: Path):
    out = _load_secrets(tmp_path / "no-such-file.env")
    assert out == {}


def test_load_strips_quotes(tmp_path: Path):
    p = tmp_path / "secrets.env"
    p.write_text('DEEPGRAM_API_KEY="dg-key-quoted"\n')
    assert _load_secrets(p) == {"deepgram": "dg-key-quoted"}


def test_save_updates_existing_key_in_place(tmp_path: Path):
    p = tmp_path / "secrets.env"
    p.write_text(
        "# header\n"
        "DEEPGRAM_API_KEY=old-dg\n"
        "MYMEMORY_EMAIL=user@example.com\n"
    )
    _save_secrets({"deepgram": "new-dg"}, p)
    text = p.read_text()
    assert "DEEPGRAM_API_KEY=new-dg" in text
    assert "DEEPGRAM_API_KEY=old-dg" not in text
    assert "# header" in text
    assert "MYMEMORY_EMAIL=user@example.com" in text


def test_save_appends_new_key_when_absent(tmp_path: Path):
    p = tmp_path / "secrets.env"
    p.write_text("DEEPGRAM_API_KEY=dg-key\n")
    _save_secrets({"gladia": "gl-key"}, p)
    text = p.read_text()
    assert "DEEPGRAM_API_KEY=dg-key" in text
    assert "GLADIA_API_KEY=gl-key" in text


def test_save_preserves_unrelated_lines(tmp_path: Path):
    p = tmp_path / "secrets.env"
    original = (
        "# F9 Talk secrets\n"
        "\n"
        "DEEPGRAM_API_KEY=dg-old\n"
        "MYMEMORY_EMAIL=user@example.com\n"
        "# trailing comment\n"
    )
    p.write_text(original)
    _save_secrets({"deepgram": "dg-new"}, p)
    text = p.read_text()
    assert "# F9 Talk secrets" in text
    assert "MYMEMORY_EMAIL=user@example.com" in text
    assert "# trailing comment" in text
    assert "DEEPGRAM_API_KEY=dg-new" in text


def test_save_creates_file_with_correct_permissions(tmp_path: Path):
    p = tmp_path / "config" / "secrets.env"
    _save_secrets({"deepgram": "fresh"}, p)
    assert p.exists()
    assert oct(p.stat().st_mode)[-3:] == "600"
    assert oct(p.parent.stat().st_mode)[-3:] == "700"


def test_save_then_load_roundtrip(tmp_path: Path):
    p = tmp_path / "secrets.env"
    _save_secrets({"deepgram": "a", "assemblyai": "b", "gladia": "c"}, p)
    assert _load_secrets(p) == {
        "deepgram": "a",
        "assemblyai": "b",
        "gladia": "c",
    }
