#!/usr/bin/env bash
set -e

# Build directory
APP_DIR="build/AppDir"
OUTPUT="VarLens-x86_64.AppImage"

echo "==> Compiling VarLens in release mode..."
cargo build --release

echo "==> Preparing AppDir layout..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/usr/bin"
mkdir -p "$APP_DIR/usr/share/applications"
mkdir -p "$APP_DIR/usr/share/icons/hicolor/scalable/apps"
mkdir -p "$APP_DIR/usr/share/metainfo"

# Copy binary
cp target/release/varlens "$APP_DIR/usr/bin/"

# Copy desktop and icon assets
cp data/io.github.PinedaMB.VarLens.desktop "$APP_DIR/usr/share/applications/"
cp data/icons/hicolor/scalable/apps/io.github.PinedaMB.VarLens.svg "$APP_DIR/usr/share/icons/hicolor/scalable/apps/"
cp data/io.github.PinedaMB.VarLens.metainfo.xml "$APP_DIR/usr/share/metainfo/"

# Root level AppDir links/files required by AppImage standard
cp data/io.github.PinedaMB.VarLens.desktop "$APP_DIR/"
cp data/icons/hicolor/scalable/apps/io.github.PinedaMB.VarLens.svg "$APP_DIR/io.github.PinedaMB.VarLens.svg"
cp data/icons/hicolor/scalable/apps/io.github.PinedaMB.VarLens.svg "$APP_DIR/.DirIcon"

# AppRun launcher
cat << 'EOF' > "$APP_DIR/AppRun"
#!/bin/sh
SELF=$(readlink -f "$0")
HERE=${SELF%/*}
export PATH="${HERE}/usr/bin:${PATH}"
export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS}"
exec "${HERE}/usr/bin/varlens" "$@"
EOF

chmod +x "$APP_DIR/AppRun"

echo "==> Building AppImage..."
if ! command -v appimagetool >/dev/null 2>&1; then
    echo "Notice: 'appimagetool' not found in PATH."
    echo "Downloading appimagetool..."
    wget -q https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage -O appimagetool
    chmod +x appimagetool
    ARCH=x86_64 ./appimagetool "$APP_DIR" "$OUTPUT"
    rm -f appimagetool
else
    ARCH=x86_64 appimagetool "$APP_DIR" "$OUTPUT"
fi

echo "==> VarLens AppImage successfully generated: $OUTPUT"
