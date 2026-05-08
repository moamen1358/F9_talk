"""Modal dialog for editing cloud-provider API keys."""
from __future__ import annotations

from PySide6.QtWidgets import (
    QDialog,
    QDialogButtonBox,
    QFormLayout,
    QHBoxLayout,
    QLineEdit,
    QPushButton,
    QVBoxLayout,
    QWidget,
)


class APIKeysDialog(QDialog):
    """Two-field dialog for editing Deepgram / Gladia API keys.

    Pure UI: takes a ``current`` dict on construction, exposes the user's
    edits via :meth:`edited_keys`. The caller persists them.
    """

    PROVIDERS = ("deepgram", "gladia")
    LABELS = {
        "deepgram": "Deepgram",
        "gladia":   "Gladia",
    }
    PLACEHOLDERS = {
        "deepgram": "Sign up free at console.deepgram.com",
        "gladia":   "Sign up at app.gladia.io",
    }

    def __init__(self, current: dict[str, str] | None = None) -> None:
        super().__init__()
        self.setWindowTitle("F9 Talk — API Keys")
        self.setMinimumWidth(460)

        self._current = dict(current or {})
        self._fields: dict[str, QLineEdit] = {}
        self._show_buttons: dict[str, QPushButton] = {}

        layout = QVBoxLayout(self)
        layout.setSpacing(12)
        layout.setContentsMargins(20, 20, 20, 20)

        form = QFormLayout()
        form.setSpacing(10)
        for provider in self.PROVIDERS:
            row, edit, show_btn = self._build_row(provider)
            self._fields[provider] = edit
            self._show_buttons[provider] = show_btn
            form.addRow(self.LABELS[provider], row)
        layout.addLayout(form)

        buttons = QDialogButtonBox(QDialogButtonBox.Save | QDialogButtonBox.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addWidget(buttons)

    def _build_row(self, provider: str) -> tuple[QWidget, QLineEdit, QPushButton]:
        row = QWidget()
        layout = QHBoxLayout(row)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(6)

        edit = QLineEdit()
        edit.setEchoMode(QLineEdit.Password)
        edit.setPlaceholderText(self.PLACEHOLDERS[provider])
        edit.setText(self._current.get(provider, ""))
        edit.setMinimumHeight(28)
        layout.addWidget(edit, stretch=1)

        show_btn = QPushButton("Show")
        show_btn.setCheckable(True)
        show_btn.setFixedWidth(64)
        show_btn.toggled.connect(
            lambda on, e=edit, b=show_btn: self._toggle_visibility(e, b, on)
        )
        layout.addWidget(show_btn)

        return row, edit, show_btn

    @staticmethod
    def _toggle_visibility(edit: QLineEdit, btn: QPushButton, on: bool) -> None:
        edit.setEchoMode(QLineEdit.Normal if on else QLineEdit.Password)
        btn.setText("Hide" if on else "Show")

    def edited_keys(self) -> dict[str, str]:
        """Return only the fields the user actually changed (and non-empty)."""
        out: dict[str, str] = {}
        for provider, edit in self._fields.items():
            new_value = edit.text().strip()
            if not new_value:
                continue  # empty = keep existing
            if new_value == self._current.get(provider, ""):
                continue  # unchanged
            out[provider] = new_value
        return out
