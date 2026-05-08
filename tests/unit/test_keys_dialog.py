"""Unit tests for the API keys dialog."""
from __future__ import annotations

import pytest
from PySide6.QtWidgets import QDialog, QLineEdit

from f9_talk.ui.keys_dialog import APIKeysDialog


@pytest.fixture
def current():
    return {
        "deepgram":   "dg-existing",
        "assemblyai": "aa-existing",
        "gladia":     "gl-existing",
    }


def test_populates_fields_from_current(qapp, current):
    dlg = APIKeysDialog(current)
    assert dlg._fields["deepgram"].text() == "dg-existing"
    assert dlg._fields["assemblyai"].text() == "aa-existing"
    assert dlg._fields["gladia"].text() == "gl-existing"


def test_fields_masked_by_default(qapp, current):
    dlg = APIKeysDialog(current)
    for edit in dlg._fields.values():
        assert edit.echoMode() == QLineEdit.Password


def test_show_button_toggles_one_field_only(qapp, current):
    dlg = APIKeysDialog(current)
    dlg._show_buttons["deepgram"].setChecked(True)
    assert dlg._fields["deepgram"].echoMode() == QLineEdit.Normal
    assert dlg._fields["assemblyai"].echoMode() == QLineEdit.Password
    assert dlg._fields["gladia"].echoMode() == QLineEdit.Password


def test_edited_keys_returns_only_changed(qapp, current):
    dlg = APIKeysDialog(current)
    dlg._fields["assemblyai"].setText("aa-NEW")
    edits = dlg.edited_keys()
    assert edits == {"assemblyai": "aa-NEW"}


def test_edited_keys_skips_empty_fields(qapp, current):
    dlg = APIKeysDialog(current)
    dlg._fields["deepgram"].setText("")  # empty = keep existing
    dlg._fields["assemblyai"].setText("aa-NEW")
    edits = dlg.edited_keys()
    assert edits == {"assemblyai": "aa-NEW"}
    assert "deepgram" not in edits


def test_edited_keys_handles_no_current_value(qapp):
    dlg = APIKeysDialog({})
    dlg._fields["gladia"].setText("gl-fresh")
    edits = dlg.edited_keys()
    assert edits == {"gladia": "gl-fresh"}


def test_construction_with_none_uses_empty_dict(qapp):
    dlg = APIKeysDialog(None)
    for edit in dlg._fields.values():
        assert edit.text() == ""


def test_dialog_result_codes(qapp, current):
    dlg = APIKeysDialog(current)
    dlg.accept()
    assert dlg.result() == QDialog.Accepted
    dlg2 = APIKeysDialog(current)
    dlg2.reject()
    assert dlg2.result() == QDialog.Rejected
