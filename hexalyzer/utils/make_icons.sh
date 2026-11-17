#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ASSETS_DIR="$SCRIPT_DIR/../assets"

# --- Prep icons for MacOS bundle (app finder icon)

mkdir -p "$ASSETS_DIR/icon.iconset"

sips -z 16 16     "$ASSETS_DIR/icon_1024x1024_mac.png" --out "$ASSETS_DIR/icon.iconset/icon_16x16.png"
sips -z 32 32     "$ASSETS_DIR/icon_1024x1024_mac.png" --out "$ASSETS_DIR/icon.iconset/icon_16x16@2x.png"
sips -z 128 128   "$ASSETS_DIR/icon_1024x1024_mac.png" --out "$ASSETS_DIR/icon.iconset/icon_128x128.png"
sips -z 256 256   "$ASSETS_DIR/icon_1024x1024_mac.png" --out "$ASSETS_DIR/icon.iconset/icon_128x128@2x.png"
sips -z 256 256   "$ASSETS_DIR/icon_1024x1024_mac.png" --out "$ASSETS_DIR/icon.iconset/icon_256x256.png"
sips -z 512 512   "$ASSETS_DIR/icon_1024x1024_mac.png" --out "$ASSETS_DIR/icon.iconset/icon_256x256@2x.png"
sips -z 512 512   "$ASSETS_DIR/icon_1024x1024_mac.png" --out "$ASSETS_DIR/icon.iconset/icon_512x512.png"
sips -z 1024 1024 "$ASSETS_DIR/icon_1024x1024_mac.png" --out "$ASSETS_DIR/icon.iconset/icon_512x512@2x.png"

iconutil -c icns "$ASSETS_DIR/icon.iconset"

# --- Prep icon for Windows executable and installer
# (the window / dock icon is loaded at runtime from the 128x128 PNGs)

magick "$ASSETS_DIR/icon_1024x1024_win.png" -define icon:auto-resize=256,128,64,48,32,16 "$ASSETS_DIR/icon.ico"
