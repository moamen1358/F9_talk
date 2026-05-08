#!/usr/bin/env bash
# build_deb.sh — Build a self-contained .deb for f9-talk.
#
# Usage:
#   ./build_deb.sh
#
# Output: f9-talk_0.1.0_all.deb  (in the project root)
#
# Install it with:
#   sudo dpkg -i f9-talk_0.1.0_all.deb
#   sudo apt-get install -f   # fix any missing system deps if needed
set -euo pipefail

VERSION="0.1.0"
PKG="f9-talk"
ARCH="all"
SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_ROOT="/tmp/${PKG}-deb-build"
DEB_DIR="${BUILD_ROOT}/${PKG}_${VERSION}_${ARCH}"
OUT_DEB="${SRC_DIR}/${PKG}_${VERSION}_${ARCH}.deb"

echo "── Preparing build tree ────────────────────────────────────────────────"
rm -rf "$BUILD_ROOT"
mkdir -p \
    "$DEB_DIR/DEBIAN" \
    "$DEB_DIR/opt/f9-talk" \
    "$DEB_DIR/usr/local/bin" \
    "$DEB_DIR/usr/share/applications" \
    "$DEB_DIR/usr/share/icons/hicolor/scalable/apps" \
    "$DEB_DIR/usr/share/icons/hicolor/48x48/apps" \
    "$DEB_DIR/usr/share/icons/hicolor/128x128/apps" \
    "$DEB_DIR/usr/share/icons/hicolor/512x512/apps" \
    "$DEB_DIR/etc/xdg/autostart"

echo "── Copying source ──────────────────────────────────────────────────────"
rsync -a \
    --exclude='.venv/' \
    --exclude='*.egg-info/' \
    --exclude='__pycache__/' \
    --exclude='.git/' \
    --exclude='.env' \
    --exclude='build_deb.sh' \
    --exclude='install.sh' \
    --exclude='*.deb' \
    --exclude='scripts/' \
    --exclude='docs/' \
    "$SRC_DIR/" "$DEB_DIR/opt/f9-talk/"

# ── Icon ─────────────────────────────────────────────────────────────────────
ASSETS_DIR="$SRC_DIR/f9_talk/assets"
# Scalable SVG (vector, perfect at all sizes)
cp "$ASSETS_DIR/f9-talk.svg" "$DEB_DIR/usr/share/icons/hicolor/scalable/apps/f9-talk.svg"
# Original 512x512 PNG (highest-res raster fallback)
cp "$ASSETS_DIR/f9-talk.png" "$DEB_DIR/usr/share/icons/hicolor/512x512/apps/f9-talk.png"
# Smaller raster sizes — use rsvg-convert for quality, fall back to copying the 512px PNG
for SIZE in 48 128; do
    if command -v rsvg-convert &>/dev/null; then
        rsvg-convert -w "$SIZE" -h "$SIZE" "$ASSETS_DIR/f9-talk.svg" \
            -o "$DEB_DIR/usr/share/icons/hicolor/${SIZE}x${SIZE}/apps/f9-talk.png"
    else
        cp "$ASSETS_DIR/f9-talk.png" \
           "$DEB_DIR/usr/share/icons/hicolor/${SIZE}x${SIZE}/apps/f9-talk.png"
    fi
done

# ── Wrapper script ────────────────────────────────────────────────────────────
cat > "$DEB_DIR/usr/local/bin/f9-talk" << 'WRAPPER'
#!/usr/bin/env bash
# Load user secrets if present
SECRETS="$HOME/.config/F9_talk/secrets.env"
if [[ -f "$SECRETS" ]]; then
    set -o allexport
    # shellcheck disable=SC1090
    source "$SECRETS"
    set +o allexport
fi
exec /opt/f9-talk/.venv/bin/python -m f9_talk "$@"
WRAPPER
chmod 755 "$DEB_DIR/usr/local/bin/f9-talk"

# ── App menu entry ────────────────────────────────────────────────────────────
cat > "$DEB_DIR/usr/share/applications/f9-talk.desktop" << 'DESKTOP'
[Desktop Entry]
Type=Application
Version=1.0
Name=F9 Talk
GenericName=Dictation
Comment=Hold F9 to speak; text appears at your cursor
Exec=f9-talk --backend cloud
Icon=f9-talk
Categories=Utility;Accessibility;
Keywords=dictation;speech;voice;stt;
StartupNotify=false
DESKTOP

# ── System-wide autostart (all users; each can disable per-user) ──────────────
cat > "$DEB_DIR/etc/xdg/autostart/f9-talk.desktop" << 'AUTOSTART'
[Desktop Entry]
Type=Application
Version=1.0
Name=F9 Talk
Comment=Hold-to-talk dictation — auto-starts on login
Exec=f9-talk --backend cloud
Icon=f9-talk
X-GNOME-Autostart-enabled=true
X-GNOME-Autostart-Delay=5
Hidden=false
NoDisplay=false
AUTOSTART

# ── DEBIAN/control ────────────────────────────────────────────────────────────
cat > "$DEB_DIR/DEBIAN/control" << CONTROL
Package: f9-talk
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Depends: python3 (>= 3.10), python3-venv, python3-pip, libportaudio2, libsndfile1, libxcb-cursor0, libxkbcommon-x11-0, libdbus-1-3, xdotool
Maintainer: Moamen Ghareeb <info@whiteguard.co.uk>
Homepage: https://github.com/moamen1358/F9_talk
Description: Hold-to-talk system-wide dictation for Linux
 Hold F9 (or any key) to speak — text appears at your cursor.
 Supports Deepgram cloud STT and optional local Whisper (GPU).
 Audio-reactive overlay indicator. Optional translation.
CONTROL

# ── DEBIAN/postinst ───────────────────────────────────────────────────────────
cat > "$DEB_DIR/DEBIAN/postinst" << 'POSTINST'
#!/usr/bin/env bash
set -e

INSTALL_DIR="/opt/f9-talk"
VENV_DIR="$INSTALL_DIR/.venv"

echo "f9-talk: creating Python virtual environment..."
python3 -m venv "$VENV_DIR"
"$VENV_DIR/bin/pip" install --quiet --upgrade pip
"$VENV_DIR/bin/pip" install --quiet "$INSTALL_DIR"

# Set up secrets file for the user who ran sudo (if any)
TARGET_USER="${SUDO_USER:-$USER}"
TARGET_HOME=$(getent passwd "$TARGET_USER" | cut -d: -f6)
CONFIG_DIR="$TARGET_HOME/.config/F9_talk"
SECRETS_FILE="$CONFIG_DIR/secrets.env"

if [[ -n "$TARGET_HOME" && ! -f "$SECRETS_FILE" ]]; then
    mkdir -p "$CONFIG_DIR"
    chmod 700 "$CONFIG_DIR"
    cp "$INSTALL_DIR/.env.example" "$SECRETS_FILE"
    chmod 600 "$SECRETS_FILE"
    chown -R "$TARGET_USER:" "$CONFIG_DIR"
    echo "f9-talk: created $SECRETS_FILE"
    echo "  ➜  Add your Deepgram API key there before using the cloud backend."
fi

# Refresh icon cache so the icon appears immediately
if command -v gtk-update-icon-cache &>/dev/null; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor &>/dev/null || true
fi

echo "f9-talk: installed successfully."
POSTINST
chmod 755 "$DEB_DIR/DEBIAN/postinst"

# ── DEBIAN/prerm ──────────────────────────────────────────────────────────────
cat > "$DEB_DIR/DEBIAN/prerm" << 'PRERM'
#!/usr/bin/env bash
set -e
echo "f9-talk: removing installation..."
rm -rf /opt/f9-talk
rm -f /usr/local/bin/f9-talk
PRERM
chmod 755 "$DEB_DIR/DEBIAN/prerm"

# ── Build ─────────────────────────────────────────────────────────────────────
echo "── Building .deb ───────────────────────────────────────────────────────"
dpkg-deb --build --root-owner-group "$DEB_DIR" "$OUT_DEB"

echo ""
echo "Done: $OUT_DEB"
echo ""
echo "Install with:"
echo "  sudo dpkg -i $OUT_DEB"
echo "  sudo apt-get install -f   # only needed if deps are missing"
