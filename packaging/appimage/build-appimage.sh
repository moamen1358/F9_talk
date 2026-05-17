#!/usr/bin/env bash
# Build an AppImage for f9-talk.
# Usage: ./packaging/appimage/build-appimage.sh [path-to-binary]
#
# If no binary path is given, it builds one with `cargo build --release`.
# The resulting .AppImage is placed in the repo root.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BINARY="${1:-$REPO_ROOT/target/release/f9-talk}"

if [ ! -f "$BINARY" ]; then
    echo "Binary not found at $BINARY — building..."
    cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml"
    BINARY="$REPO_ROOT/target/release/f9-talk"
fi

# Create AppDir structure
APPDIR="$REPO_ROOT/AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/scalable/apps"
mkdir -p "$APPDIR/usr/share/icons/hicolor/512x512/apps"

# Copy binary
cp "$BINARY" "$APPDIR/usr/bin/f9-talk"
chmod 755 "$APPDIR/usr/bin/f9-talk"

# Copy desktop file
cp "$REPO_ROOT/packaging/debian/applications/f9-talk.desktop" \
   "$APPDIR/usr/share/applications/f9-talk.desktop"

# Copy icons
cp "$REPO_ROOT/assets/f9-talk.svg" \
   "$APPDIR/usr/share/icons/hicolor/scalable/apps/f9-talk.svg"
cp "$REPO_ROOT/assets/f9-talk.png" \
   "$APPDIR/usr/share/icons/hicolor/512x512/apps/f9-talk.png"

# AppImage requires these at the root of AppDir
ln -sf usr/share/applications/f9-talk.desktop "$APPDIR/f9-talk.desktop"
ln -sf usr/share/icons/hicolor/scalable/apps/f9-talk.svg "$APPDIR/f9-talk.svg"
ln -sf usr/share/icons/hicolor/scalable/apps/f9-talk.svg "$APPDIR/.DirIcon"

# Create AppRun script
cat > "$APPDIR/AppRun" << 'EOF'
#!/bin/bash
SELF="$(readlink -f "$0")"
HERE="${SELF%/*}"
export PATH="${HERE}/usr/bin:${PATH}"
exec "${HERE}/usr/bin/f9-talk" "$@"
EOF
chmod 755 "$APPDIR/AppRun"

# Download appimagetool if not present
if ! command -v appimagetool &> /dev/null; then
    echo "Downloading appimagetool..."
    curl -sSL "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage" \
        -o /tmp/appimagetool
    chmod +x /tmp/appimagetool
    APPIMAGETOOL="/tmp/appimagetool"
else
    APPIMAGETOOL="appimagetool"
fi

# Build the AppImage
VERSION=$(grep '^version' "$REPO_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
export VERSION
ARCH=x86_64 "$APPIMAGETOOL" --no-appstream "$APPDIR" \
    "$REPO_ROOT/f9-talk-${VERSION}-x86_64.AppImage"

# Clean up
rm -rf "$APPDIR"

echo ""
echo "✓ AppImage created: f9-talk-${VERSION}-x86_64.AppImage"
echo "  Run it: chmod +x f9-talk-${VERSION}-x86_64.AppImage && ./f9-talk-${VERSION}-x86_64.AppImage"
