#!/usr/bin/env bash
# Build the standalone .app and a styled .dmg for Claude Usage Monitor.
#
# The fancy installer dmg (custom coral background + arrow, icon layout, and the
# "Claude Usage Monitor Installer" window title) is produced with `create-dmg`,
# NOT Tauri's dmg bundler — Tauri can't set a separate volume/window title. So
# THIS script is the source of truth for the release dmg.
#
# Requires: create-dmg (`brew install create-dmg`). Runs the AppleScript/Finder
# step, so run it in a GUI session and approve the one-time automation prompt.
set -euo pipefail
cd "$(dirname "$0")/.."

APP_NAME="Claude Usage Monitor"
VERSION="$(grep -m1 '"version"' src-tauri/tauri.conf.json | sed -E 's/.*"([0-9.]+)".*/\1/')"
ARCH="$(uname -m)"; [ "$ARCH" = "x86_64" ] && ARCH="x64"

command -v create-dmg >/dev/null || { echo "create-dmg not found — run: brew install create-dmg"; exit 1; }
export PATH="$HOME/.cargo/bin:$PATH"

echo "==> Building app bundle..."
bun run tauri build --bundles app

APP="src-tauri/target/release/bundle/macos/${APP_NAME}.app"
[ -d "$APP" ] || { echo "app not found at $APP"; exit 1; }

OUTDIR="src-tauri/target/release/bundle/dmg"
OUT="${OUTDIR}/Claude-Usage-Monitor-${VERSION}-${ARCH}.dmg"
STAGE="$(mktemp -d)"
mkdir -p "$OUTDIR"; rm -f "$OUT"
cp -R "$APP" "$STAGE/"
hdiutil detach "/Volumes/${APP_NAME} Installer" >/dev/null 2>&1 || true

echo "==> Building dmg..."
create-dmg \
  --volname "${APP_NAME} Installer" \
  --background "src-tauri/icons/dmg-background.png" \
  --window-pos 200 120 \
  --window-size 600 420 \
  --icon-size 120 \
  --icon "${APP_NAME}.app" 150 185 \
  --hide-extension "${APP_NAME}.app" \
  --app-drop-link 450 185 \
  "$OUT" "$STAGE"

rm -rf "$STAGE"
echo "==> Done: $OUT"
