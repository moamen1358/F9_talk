"""Animated recording indicator: small frameless always-on-top widget."""
from __future__ import annotations

import math
import shutil
import subprocess
import threading
import time

from PySide6.QtCore import QPoint, QPointF, QRect, QSize, Qt, QTimer, Signal, Slot
from PySide6.QtGui import (
    QBrush,
    QColor,
    QFont,
    QGuiApplication,
    QLinearGradient,
    QPainter,
    QPainterPath,
    QPen,
    QRadialGradient,
)
from PySide6.QtWidgets import QWidget


def _cursor_pos() -> tuple[int, int] | None:
    if shutil.which("xdotool") is None:
        return None
    try:
        out = subprocess.check_output(["xdotool", "getmouselocation"], timeout=0.5).decode()
        parts: dict[str, str] = {}
        for tok in out.strip().split():
            if ":" in tok:
                k, v = tok.split(":", 1)
                parts[k] = v
        return int(parts["x"]), int(parts["y"])
    except Exception:
        return None


def _focused_window_geometry() -> tuple[int, int, int, int] | None:
    if shutil.which("xdotool") is None:
        return None
    try:
        out = subprocess.check_output(
            ["xdotool", "getactivewindow", "getwindowgeometry", "--shell"],
            timeout=0.5,
        ).decode()
        kvs: dict[str, str] = {}
        for line in out.strip().splitlines():
            if "=" in line:
                k, v = line.split("=", 1)
                kvs[k.strip()] = v.strip()
        return (int(kvs["X"]), int(kvs["Y"]), int(kvs["WIDTH"]), int(kvs["HEIGHT"]))
    except Exception:
        return None


class DictateIndicator(QWidget):
    """Frameless transparent always-on-top widget that visualizes recording state.

    Six visual styles:
      - "wave"   – Siri-style audio-waveform (audio-reactive amplitude)
      - "bars"   – red equalizer bars + timer (legacy default)
      - "pulse"  – breathing white dot
      - "dots"   – three-dot typing indicator
      - "ripple" – sonar concentric expanding rings
      - "blob"   – morphing white liquid blob

    Slots (all thread-safe via QueuedConnection):
      show_recording      – start showing the indicator
      hide_recording      – hide it
      set_status_text     – flash a status string ("Transcribing…" etc.)
      set_audio_level     – feed normalized RMS for the wave style's amplitude
    """

    show_recording = Signal()
    hide_recording = Signal()
    set_status_text = Signal(str)
    set_audio_level = Signal(float)

    STYLES = ("wave", "bars", "pulse", "dots", "ripple", "blob")

    NUM_BARS = 5
    BAR_W = 6
    BAR_GAP = 5
    BAR_MIN_H = 6
    BAR_MAX_H = 38
    PILL_PAD_H = 18
    PILL_PAD_V = 14
    TEXT_GAP = 14
    TEXT_WIDTH = 56
    PILL_RADIUS = 22

    def __init__(self, style: str = "wave") -> None:
        super().__init__()
        self.setWindowFlags(
            Qt.FramelessWindowHint
            | Qt.WindowStaysOnTopHint
            | Qt.Tool
            | Qt.WindowDoesNotAcceptFocus
        )
        self.setAttribute(Qt.WA_TranslucentBackground, True)
        self.setAttribute(Qt.WA_TransparentForMouseEvents, True)
        self.setAttribute(Qt.WA_ShowWithoutActivating, True)

        self._mode = "recording"
        self._status_text = ""
        self._t0 = 0.0
        self._anim_t = 0.0
        self._audio_level = 0.0
        self._audio_level_smoothed = 0.0
        self._font = QFont("Noto Sans", 13, QFont.DemiBold)
        self.style = style if style in self.STYLES else "wave"

        self._size_for_style()

        self.timer = QTimer(self)
        self.timer.timeout.connect(self._tick)

        self.show_recording.connect(self._on_show, Qt.QueuedConnection)
        self.hide_recording.connect(self._on_hide, Qt.QueuedConnection)
        self.set_status_text.connect(self._on_status, Qt.QueuedConnection)
        self.set_audio_level.connect(self._on_audio_level, Qt.QueuedConnection)

        self.hide()

    def _size_for_style(self) -> None:
        if self.style == "pulse":
            self.setFixedSize(64, 64)
        elif self.style == "dots":
            self.setFixedSize(96, 40)
        elif self.style == "wave":
            self.setFixedSize(180, 56)
        elif self.style == "ripple":
            self.setFixedSize(72, 72)
        elif self.style == "blob":
            self.setFixedSize(76, 76)
        else:
            bars_w = self.NUM_BARS * self.BAR_W + (self.NUM_BARS - 1) * self.BAR_GAP
            w = self.PILL_PAD_H * 2 + bars_w + self.TEXT_GAP + self.TEXT_WIDTH
            h = self.BAR_MAX_H + self.PILL_PAD_V * 2
            self.setFixedSize(w, h)

    def sizeHint(self) -> QSize:  # noqa: D401
        return self.size()

    # ---------- slots ----------

    @Slot()
    def _on_show(self) -> None:
        self._mode = "recording"
        self._t0 = time.monotonic()
        self._anim_t = 0.0
        self._audio_level = 0.0
        self._audio_level_smoothed = 0.0
        self.show()
        self.raise_()
        self.timer.start(16)  # ~60 fps
        threading.Thread(target=self._reposition_async, daemon=True, name="indicator-pos").start()

    @Slot()
    def _on_hide(self) -> None:
        self.timer.stop()
        self.hide()

    @Slot(str)
    def _on_status(self, text: str) -> None:
        self._mode = "status"
        self._status_text = text
        if not self.isVisible():
            self.show()
            self.raise_()
        if not self.timer.isActive():
            self.timer.start(16)
        self.update()
        threading.Thread(target=self._reposition_async, daemon=True, name="indicator-pos").start()

    @Slot(float)
    def _on_audio_level(self, level: float) -> None:
        self._audio_level = max(0.0, level)
        target = self._audio_level
        # Asymmetric EMA — rise fast, fall slowly
        if target > self._audio_level_smoothed:
            self._audio_level_smoothed = 0.55 * self._audio_level_smoothed + 0.45 * target
        else:
            self._audio_level_smoothed = 0.85 * self._audio_level_smoothed + 0.15 * target

    def _tick(self) -> None:
        self._anim_t = time.monotonic() - self._t0
        self.update()

    # ---------- painting ----------

    def paintEvent(self, _event) -> None:  # noqa: N802, D401
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)

        # Wave style is unique: no background pill — line floats free.
        if self.style != "wave":
            p.setBrush(QColor(10, 10, 10, 245))
            p.setPen(Qt.NoPen)
            radius = min(self.height() // 2, self.PILL_RADIUS)
            p.drawRoundedRect(self.rect(), radius, radius)

        if self._mode == "status":
            self._paint_status(p)
            return

        if self.style == "pulse":
            self._paint_pulse(p)
        elif self.style == "dots":
            self._paint_dots(p)
        elif self.style == "wave":
            self._paint_wave(p)
        elif self.style == "ripple":
            self._paint_ripple(p)
        elif self.style == "blob":
            self._paint_blob(p)
        else:
            p.setBrush(Qt.NoBrush)
            glow_pulse = 0.6 + 0.4 * math.sin(self._anim_t * 5.0)
            p.setPen(QColor(255, 60, 60, int(80 + 70 * glow_pulse)))
            p.drawRoundedRect(self.rect().adjusted(1, 1, -1, -1), self.PILL_RADIUS - 1, self.PILL_RADIUS - 1)
            self._paint_bars(p)

    # --- individual style painters ---

    def _paint_pulse(self, p: QPainter) -> None:
        cx = self.width() // 2
        cy = self.height() // 2
        ring_t = (self._anim_t * 0.625) % 1.0
        ring_radius = int(10 + 18 * ring_t)
        ring_alpha = int(120 * (1.0 - ring_t))
        if ring_alpha > 0:
            p.setBrush(Qt.NoBrush)
            pen = p.pen()
            pen.setColor(QColor(255, 255, 255, ring_alpha))
            pen.setWidth(2)
            p.setPen(pen)
            p.drawEllipse(QPoint(cx, cy), ring_radius, ring_radius)
        breath = 0.5 + 0.5 * math.sin(self._anim_t * 4.0)
        r = int(6 + 4 * breath)
        p.setPen(Qt.NoPen)
        p.setBrush(QColor(255, 255, 255, 230))
        p.drawEllipse(QPoint(cx, cy), r, r)

    def _paint_dots(self, p: QPainter) -> None:
        n = 3
        dot_r = 5
        gap = 16
        total_w = (n - 1) * gap
        cy = self.height() // 2
        start_x = (self.width() - total_w) // 2
        for i in range(n):
            phase = self._anim_t * 4.0 - i * 0.6
            level = 0.5 + 0.5 * math.sin(phase)
            alpha = int(80 + 175 * level)
            radius = int(dot_r + 1.5 * level)
            p.setPen(Qt.NoPen)
            p.setBrush(QColor(255, 255, 255, alpha))
            p.drawEllipse(QPoint(start_x + i * gap, cy), radius, radius)

    def _build_wave_path(self, time_offset: float, amp_mult: float) -> QPainterPath:
        x_start = 8
        x_end = self.width() - 8
        cy = self.height() / 2
        n = 56
        t_anim = self._anim_t + time_offset
        # Audio-reactive: silence ≈ 0.08, normal speech ≈ 1.0, loud ≈ 1.85
        level_scale = 0.08 + min(1.85, self._audio_level_smoothed * 13.0)

        pts: list[tuple[float, float]] = []
        for i in range(n):
            progress = i / (n - 1)
            x = x_start + (x_end - x_start) * progress
            envelope = 0.5 - 0.5 * math.cos(progress * 2 * math.pi)
            t = t_anim * 5.5 + progress * 7.0
            wave = (
                0.55 * math.sin(t)
                + 0.30 * math.sin(t * 2.1 + 1.4)
                + 0.18 * math.sin(t * 0.6 + 3.0)
                + 0.10 * math.sin(t * 3.7 + 2.0)
            )
            y = cy + 14.0 * amp_mult * envelope * wave * level_scale
            pts.append((x, y))

        path = QPainterPath()
        path.moveTo(*pts[0])
        for i in range(1, len(pts) - 1):
            x0, y0 = pts[i]
            x1, y1 = pts[i + 1]
            mid = ((x0 + x1) / 2, (y0 + y1) / 2)
            path.quadTo(x0, y0, *mid)
        path.lineTo(*pts[-1])
        return path

    def _paint_wave(self, p: QPainter) -> None:
        path = self._build_wave_path(time_offset=0.0, amp_mult=1.0)
        echo = self._build_wave_path(time_offset=-0.18, amp_mult=0.55)
        p.setBrush(Qt.NoBrush)
        # Outer wide soft glow
        outer = QPen(QColor(255, 40, 60, 35))
        outer.setWidthF(11.0)
        outer.setCapStyle(Qt.RoundCap)
        outer.setJoinStyle(Qt.RoundJoin)
        p.setPen(outer)
        p.drawPath(path)
        # Mid glow
        mid = QPen(QColor(255, 50, 60, 70))
        mid.setWidthF(7.0)
        mid.setCapStyle(Qt.RoundCap)
        mid.setJoinStyle(Qt.RoundJoin)
        p.setPen(mid)
        p.drawPath(path)
        # Echo wave (fainter, behind)
        ep = QPen(QColor(255, 90, 100, 90))
        ep.setWidthF(1.6)
        ep.setCapStyle(Qt.RoundCap)
        ep.setJoinStyle(Qt.RoundJoin)
        p.setPen(ep)
        p.drawPath(echo)
        # Crisp red gradient line on top
        grad = QLinearGradient(8, 0, self.width() - 8, 0)
        grad.setColorAt(0.0, QColor(220, 30, 50, 245))
        grad.setColorAt(0.5, QColor(255, 80, 90, 250))
        grad.setColorAt(1.0, QColor(220, 30, 50, 245))
        line = QPen(QBrush(grad), 2.6)
        line.setCapStyle(Qt.RoundCap)
        line.setJoinStyle(Qt.RoundJoin)
        p.setPen(line)
        p.drawPath(path)

    def _paint_ripple(self, p: QPainter) -> None:
        cx = self.width() // 2
        cy = self.height() // 2
        n_ripples = 3
        cycle = 1.6
        for i in range(n_ripples):
            phase = ((self._anim_t / cycle) + i / n_ripples) % 1.0
            r = int(6 + 28 * phase)
            alpha = int(180 * (1.0 - phase))
            if alpha <= 0:
                continue
            pen = QPen(QColor(255, 255, 255, alpha))
            pen.setWidth(2)
            p.setPen(pen)
            p.setBrush(Qt.NoBrush)
            p.drawEllipse(QPoint(cx, cy), r, r)
        p.setPen(Qt.NoPen)
        p.setBrush(QColor(255, 255, 255, 235))
        p.drawEllipse(QPoint(cx, cy), 4, 4)

    def _paint_blob(self, p: QPainter) -> None:
        cx = self.width() / 2
        cy = self.height() / 2
        base_r = 22.0
        n = 64
        path = QPainterPath()
        for i in range(n + 1):
            angle = 2 * math.pi * i / n
            deform = (
                3.0 * math.sin(self._anim_t * 2.2 + angle * 3.0)
                + 2.0 * math.sin(self._anim_t * 1.4 + angle * 5.0 + 1.1)
                + 1.2 * math.sin(self._anim_t * 3.1 + angle * 7.0 + 2.4)
            )
            r = base_r + deform
            x = cx + r * math.cos(angle)
            y = cy + r * math.sin(angle)
            if i == 0:
                path.moveTo(x, y)
            else:
                path.lineTo(x, y)
        path.closeSubpath()
        grad = QRadialGradient(QPointF(cx, cy), base_r + 6.0)
        grad.setColorAt(0.0, QColor(255, 255, 255, 240))
        grad.setColorAt(0.6, QColor(255, 255, 255, 180))
        grad.setColorAt(1.0, QColor(255, 255, 255, 50))
        p.setBrush(grad)
        p.setPen(Qt.NoPen)
        p.drawPath(path)

    def _paint_bars(self, p: QPainter) -> None:
        bars_w = self.NUM_BARS * self.BAR_W + (self.NUM_BARS - 1) * self.BAR_GAP
        bars_x = self.PILL_PAD_H
        center_y = self.height() // 2
        for i in range(self.NUM_BARS):
            phase = self._anim_t * 9.0 + i * 0.85
            base = 0.5 + 0.5 * math.sin(phase)
            envelope = 0.7 + 0.3 * math.sin(self._anim_t * 4.0 + i * 0.4)
            level = max(0.18, base * envelope)
            h = int(self.BAR_MIN_H + (self.BAR_MAX_H - self.BAR_MIN_H) * level)
            x = bars_x + i * (self.BAR_W + self.BAR_GAP)
            y = center_y - h // 2
            grad = QLinearGradient(0, y, 0, y + h)
            grad.setColorAt(0.0, QColor(255, 220, 90))
            grad.setColorAt(0.5, QColor(255, 130, 50))
            grad.setColorAt(1.0, QColor(230, 40, 60))
            p.setBrush(grad)
            p.setPen(Qt.NoPen)
            p.drawRoundedRect(QRect(x, y, self.BAR_W, h), 3, 3)
        p.setFont(self._font)
        p.setPen(QColor(255, 255, 255, 220))
        text_x = bars_x + bars_w + self.TEXT_GAP
        p.drawText(
            QRect(text_x, 0, self.width() - text_x - self.PILL_PAD_H, self.height()),
            int(Qt.AlignVCenter | Qt.AlignLeft),
            f"{self._anim_t:0.1f}s",
        )

    def _paint_status(self, p: QPainter) -> None:
        p.setFont(self._font)
        p.setPen(QColor(255, 255, 255, 230))
        p.drawText(self.rect(), int(Qt.AlignCenter), self._status_text or "")

    # ---------- positioning ----------

    def _reposition_async(self) -> None:
        """Fetch window/cursor position off the main thread, then apply on it."""
        win = _focused_window_geometry()
        pos = _cursor_pos() if win is None else None
        QTimer.singleShot(0, lambda: self._apply_position(win, pos))

    def _apply_position(self, win: tuple | None, pos: tuple | None) -> None:
        """Compute final screen-clamped position and move the widget (main thread only)."""
        if win is not None:
            wx, wy, ww, wh = win
            x = wx + (ww - self.width()) // 2
            y = wy + wh - self.height() - 24
            screen_obj = (
                QGuiApplication.screenAt(QPoint(wx + ww // 2, wy + wh // 2))
                or QGuiApplication.primaryScreen()
            )
        elif pos is not None:
            cx, cy = pos
            screen_obj = QGuiApplication.screenAt(QPoint(cx, cy)) or QGuiApplication.primaryScreen()
            x = cx - self.width() // 2
            y = cy + 28
        else:
            screen = QGuiApplication.primaryScreen().availableGeometry()
            self.move(
                screen.x() + (screen.width() - self.width()) // 2,
                screen.y() + screen.height() - self.height() - 120,
            )
            return
        screen = screen_obj.availableGeometry()
        x = max(screen.x() + 8, min(x, screen.x() + screen.width() - self.width() - 8))
        y = max(screen.y() + 8, min(y, screen.y() + screen.height() - self.height() - 8))
        self.move(x, y)
