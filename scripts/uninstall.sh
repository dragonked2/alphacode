#!/usr/bin/env bash
# uninstall.sh — remove Alphacode installed by install.sh
set -euo pipefail

# --- Determine paths (mirrors install.sh and launcher_dir() in Rust) ---
PREFIX="${ALPHACODE_PREFIX:-$HOME/.local}"
BIN_DIR="${ALPHACODE_BIN_DIR:-$PREFIX/bin}"
TARGET="$BIN_DIR/alphacode"

# Custom install dir overrides everything.
if [ -n "${ALPHACODE_INSTALL_DIR:-}" ]; then
  BIN_DIR="$ALPHACODE_INSTALL_DIR"
  TARGET="$BIN_DIR/alphacode"
fi

# ALPHACODE_HOME sandboxed layout.
if [ -n "${ALPHACODE_HOME:-}" ]; then
  BIN_DIR="$ALPHACODE_HOME/bin"
  TARGET="$BIN_DIR/alphacode"
fi

HOME_DIR="${ALPHACODE_HOME:-$HOME/.alphacode}"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/alphacode"

# --- Remove launcher binary ---
found_any=false
for candidate in \
  "$TARGET" \
  "$HOME/.local/bin/alphacode" \
  "$HOME/.local/bin/alphacode.exe" \
; do
  if [ -e "$candidate" ] || [ -L "$candidate" ]; then
    rm -f "$candidate"
    echo "Removed $candidate"
    found_any=true
  fi
done

if [ "$found_any" = false ]; then
  echo "No alphacode binary found — nothing to do." >&2
  exit 0
fi

# --- Remove data directory (~/.alphacode) ---
if [ -d "$HOME_DIR" ]; then
  rm -rf "$HOME_DIR"
  echo "Removed $HOME_DIR"
fi

# --- Remove config directory (~/.config/alphacode) ---
if [ -d "$CONFIG_DIR" ]; then
  rm -rf "$CONFIG_DIR"
  echo "Removed $CONFIG_DIR"
fi

# --- macOS hotkey LaunchAgent ---
PLIST="$HOME/Library/LaunchAgents/com.alphacode.hotkey.plist"
if [ -f "$PLIST" ]; then
  launchctl unload "$PLIST" 2>/dev/null || true
  rm -f "$PLIST"
  echo "Removed $PLIST"
fi

echo ""
echo "Alphacode has been uninstalled."
