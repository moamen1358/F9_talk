"""System tray icon for F9 Talk: status indicator + pause/resume + quit."""
from __future__ import annotations

import logging
from pathlib import Path

from PySide6.QtCore import Signal
from PySide6.QtGui import QAction, QActionGroup, QColor, QIcon, QImage, QPixmap, qAlpha, qGray, qRgba
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


def _make_error_icon(source: QIcon) -> QIcon:
    """Return a red-tinted variant of an icon to flag the last session as failed."""
    pixmap = source.pixmap(64, 64)
    image = pixmap.toImage().convertToFormat(QImage.Format_ARGB32)
    for y in range(image.height()):
        for x in range(image.width()):
            px = image.pixel(x, y)
            a = qAlpha(px)
            if a == 0:
                continue
            g = qGray(px)
            # Map gray -> red ramp; preserve alpha.
            image.setPixelColor(x, y, QColor(qRgba(min(255, g + 80), 30, 30, a)))
    return QIcon(QPixmap.fromImage(image))


class DictateTray(QSystemTrayIcon):
    """Tray icon that exposes pause/resume + quit and visualizes the listen state.

    Signals:
        pause_changed(bool) — True when paused, False when active
        quit_requested()    — user clicked Quit in the menu
    """

    pause_changed = Signal(bool)
    provider_changed = Signal(str)  # "deepgram" | "assemblyai" | "gladia"
    quit_requested = Signal()

    def __init__(
        self,
        qapp: QApplication,
        *,
        assemblyai_available: bool = True,
        gladia_available: bool = True,
    ) -> None:
        super().__init__(qapp)
        self._paused = False
        self._assemblyai_available = assemblyai_available
        self._gladia_available = gladia_available
        # Prefer the system theme icon (installed by the .deb at
        # /usr/share/icons/hicolor/.../f9-talk.png) so GNOME's AppIndicator
        # extension can resolve it via the freedesktop icon-theme system.
        # Fall back to the bundled PNG when the theme entry is missing.
        theme_icon = QIcon.fromTheme("f9-talk")
        self._active_icon = theme_icon if not theme_icon.isNull() else QIcon(str(_ICON_PATH))
        self._paused_icon = _make_paused_icon(self._active_icon)
        self._error_icon = _make_error_icon(self._active_icon)
        self._error_active = False

        self._toggle_action = QAction("Pause listening", self)
        self._toggle_action.triggered.connect(self.toggle_pause)
        self._quit_action = QAction("Quit", self)
        self._quit_action.triggered.connect(self.quit_requested.emit)

        menu = QMenu()
        menu.addAction(self._toggle_action)
        menu.addSeparator()

        provider_menu = menu.addMenu("Cloud provider")
        provider_group = QActionGroup(self)
        provider_group.setExclusive(True)
        self._deepgram_action = QAction("Deepgram (Nova-3)", self, checkable=True)
        self._deepgram_action.setChecked(True)
        self._deepgram_action.triggered.connect(
            lambda: self.provider_changed.emit("deepgram")
        )
        provider_group.addAction(self._deepgram_action)
        provider_menu.addAction(self._deepgram_action)

        self._assemblyai_action = QAction("AssemblyAI (Universal)", self, checkable=True)
        self._assemblyai_action.setEnabled(self._assemblyai_available)
        self._assemblyai_action.triggered.connect(
            lambda: self.provider_changed.emit("assemblyai")
        )
        provider_group.addAction(self._assemblyai_action)
        provider_menu.addAction(self._assemblyai_action)

        self._gladia_action = QAction("Gladia (v2 live)", self, checkable=True)
        self._gladia_action.setEnabled(self._gladia_available)
        self._gladia_action.triggered.connect(
            lambda: self.provider_changed.emit("gladia")
        )
        provider_group.addAction(self._gladia_action)
        provider_menu.addAction(self._gladia_action)

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
        elif self._error_active:
            self.setIcon(self._error_icon)
            self.setToolTip("F9 Talk — Last session failed")
            self._toggle_action.setText("Pause listening")
        else:
            self.setIcon(self._active_icon)
            self.setToolTip("F9 Talk — Listening")
            self._toggle_action.setText("Pause listening")

    def show_error(self, message: str) -> None:
        """Pop a desktop notification and switch to the error icon."""
        self._error_active = True
        self._refresh_visuals()
        self.showMessage(
            "F9 Talk — STT error",
            message,
            QSystemTrayIcon.MessageIcon.Critical,
            5000,
        )

    def clear_error(self) -> None:
        """Restore the active icon after a successful session."""
        if not self._error_active:
            return
        self._error_active = False
        self._refresh_visuals()

    def _on_activated(self, reason: QSystemTrayIcon.ActivationReason) -> None:
        if reason == QSystemTrayIcon.Trigger:
            self.toggle_pause()
