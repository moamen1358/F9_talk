#!/usr/bin/env bash
# install.sh — wire f9-talk into the desktop so it auto-starts on login.
#
# Run once after cloning / moving the project:
#   chmod +x install.sh && ./install.sh
#
# Safe to re-run: existing secrets.env is never overwritten.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV_PYTHON="$SCRIPT_DIR/.venv/bin/python"
VENV_BIN="$SCRIPT_DIR/.venv/bin/f9-talk"
CONFIG_DIR="$HOME/.config/F9_talk"
SECRETS_FILE="$CONFIG_DIR/secrets.env"
AUTOSTART_FILE="$HOME/.config/autostart/f9-talk.desktop"
APP_FILE="$HOME/.local/share/applications/f9-talk.desktop"

# ── 1. Sanity check ──────────────────────────────────────────────────────────
if [[ ! -f "$VENV_PYTHON" ]]; then
    echo "ERROR: .venv not found at $SCRIPT_DIR/.venv"
    echo "Create it first:"
    echo "  cd $SCRIPT_DIR"
    echo "  python3 -m venv .venv && .venv/bin/pip install -e ."
    exit 1
fi

# ── 2. Config / secrets ──────────────────────────────────────────────────────
mkdir -p "$CONFIG_DIR"
chmod 700 "$CONFIG_DIR"

if [[ ! -f "$SECRETS_FILE" ]]; then
    cp "$SCRIPT_DIR/.env.example" "$SECRETS_FILE"
    chmod 600 "$SECRETS_FILE"
    echo "Created $SECRETS_FILE"
    echo ""
    echo "  ACTION REQUIRED: open that file and paste your Deepgram API key."
    echo "  (Get one free at https://console.deepgram.com/signup)"
    echo ""
else
    echo "Secrets file already exists — skipping: $SECRETS_FILE"
fi

# ── 3. Icon ──────────────────────────────────────────────────────────────────
ICON_SRC="$SCRIPT_DIR/f9_talk/assets"
mkdir -p "$HOME/.local/share/icons/hicolor/scalable/apps" \
         "$HOME/.local/share/icons/hicolor/512x512/apps"
cp "$ICON_SRC/f9-talk.svg" "$HOME/.local/share/icons/hicolor/scalable/apps/f9-talk.svg"
cp "$ICON_SRC/f9-talk.png" "$HOME/.local/share/icons/hicolor/512x512/apps/f9-talk.png"
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" &>/dev/null || true
echo "Icon installed"

# ── 4. App menu entry (.desktop) ─────────────────────────────────────────────
mkdir -p "$(dirname "$APP_FILE")"
cat > "$APP_FILE" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=F9 Talk
GenericName=Dictation
Comment=Hold F9 to speak; text appears at your cursor
Exec=$VENV_BIN --backend cloud
Icon=f9-talk
Categories=Utility;Accessibility;
Keywords=dictation;speech;voice;stt;
StartupNotify=false
NoDisplay=false
EOF
echo "App launcher created: $APP_FILE"

# ── 4. Autostart entry ───────────────────────────────────────────────────────
mkdir -p "$(dirname "$AUTOSTART_FILE")"
cat > "$AUTOSTART_FILE" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=F9 Talk
Comment=Hold-to-talk dictation — starts automatically on login
Exec=$VENV_BIN --backend cloud
Icon=f9-talk
X-GNOME-Autostart-enabled=true
X-GNOME-Autostart-Delay=5
Hidden=false
NoDisplay=false
EOF
echo "Autostart entry created: $AUTOSTART_FILE"

# ── 5. Done ──────────────────────────────────────────────────────────────────
echo ""
echo "Done. F9 Talk will launch automatically on your next login."
echo ""
echo "To start it right now:"
echo "  $VENV_BIN --backend cloud"
echo ""
echo "To change the hotkey or backend, edit $AUTOSTART_FILE and update the Exec= line."
echo "Options: --backend cloud|local|both  --local-hotkey f9  --target ar"
