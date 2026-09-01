#!/usr/bin/env bash
# uninstall.sh — remove Alphacode installed by install.sh
set -euo pipefail

PREFIX="${ALPHACODE_PREFIX:-$HOME/.local}"
BIN_DIR="${ALPHACODE_BIN_DIR:-$PREFIX/bin}"
TARGET="$BIN_DIR/alphacode"

if [ ! -e "$TARGET" ] && [ ! -L "$TARGET" ]; then
  echo "No alphacode binary at $TARGET — nothing to do." >&2
  exit 0
fi

rm -f "$TARGET"
echo "Removed $TARGET"

# Also remove config + sessions if requested.
if [ "${ALPHACODE_PURGE:-0}" = "1" ]; then
  if [ -d "$HOME/.config/alphacode" ]; then
    rm -rf "$HOME/.config/alphacode"
    echo "Removed ~/.config/alphacode"
  fi
  if [ -d "$HOME/.local/share/alphacode" ]; then
    rm -rf "$HOME/.local/share/alphacode"
    echo "Removed ~/.local/share/alphacode"
  fi
fi