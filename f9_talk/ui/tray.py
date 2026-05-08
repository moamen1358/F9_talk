"""System tray icon for F9 Talk: status indicator + pause/resume + quit."""
from __future__ import annotations

import logging
from pathlib import Path

from PySide6.QtCore import Signal
from PySide6.QtGui import QAction, QColor, QIcon, QImage, QPixmap, qAlpha, qGray, qRgba
from PySide6.QtWidgets import QApplication, QMenu, QSystemTrayIcon

log = logging.getLogger(__name__)

_ICON_PATH = Path(__file__).resolve().parent.parent / "assets" / "f9-talk.png"


def _make_paused_icon(source: QIcon) -> QIcon:
    """Return a desaturated, half-opacity variant of an icon for the paused state."""
    pixmap = source.pixmap(64, 64)
    image = pixmap.toImage().convertToFormat(QImage.Format_ARGB32)
    for y in range(image.height()):
        for x in range(image.width()):
            px = image.pixel(x, y)
            g = qGray(px)
            a = qAlpha(px) // 2
            image.setPixelColor(x, y, QColor(qRgba(g, g, g, a)))
    return QIcon(QPixmap.fromImage(image))


class DictateTray(QSystemTrayIcon):
    """Tray icon that exposes pause/resume + quit and visualizes the listen state.

    Signals:
        pause_changed(bool) — True when paused, False when active
        quit_requested()    — user clicked Quit in the menu
    """

    pause_changed = Signal(bool)
    quit_requested = Signal()

    def __init__(self, qapp: QApplication) -> None:
        super().__init__(qapp)
        self._paused = False
        self._active_icon = QIcon(str(_ICON_PATH))
        self._paused_icon = _make_paused_icon(self._active_icon)

        self._toggle_action = QAction("Pause listening", self)
        self._toggle_action.triggered.connect(self.toggle_pause)
        self._quit_action = QAction("Quit", self)
        self._quit_action.triggered.connect(self.quit_requested.emit)

        menu = QMenu()
        menu.addAction(self._toggle_action)
        menu.addSeparator()
        menu.addAction(self._quit_action)
        self.setContextMenu(menu)

        self.activated.connect(self._on_activated)
        self._refresh_visuals()

    def is_paused(self) -> bool:
        return self._paused

    def toggle_pause(self) -> None:
        self.set_paused(not self._paused)

    def set_paused(self, paused: bool) -> None:
        if paused == self._paused:
            return
        self._paused = paused
        self._refresh_visuals()
        self.pause_changed.emit(paused)

    def _refresh_visuals(self) -> None:
        if self._paused:
            self.setIcon(self._paused_icon)
            self.setToolTip("F9 Talk — Paused")
            self._toggle_action.setText("Resume listening")
        else:
            self.setIcon(self._active_icon)
            self.setToolTip("F9 Talk — Listening")
            self._toggle_action.setText("Pause listening")

    def _on_activated(self, reason: QSystemTrayIcon.ActivationReason) -> None:
        if reason == QSystemTrayIcon.Trigger:
            self.toggle_pause()
