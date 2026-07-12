#!/usr/bin/env bash
# Download MediaMTX (+ optional ffmpeg) into apps/edge/bin for pksp serve.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/bin"
mkdir -p "$BIN"

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64) ARCH_TAG=amd64 ;;
  arm64|aarch64) ARCH_TAG=arm64 ;;
  *) echo "unsupported arch: $ARCH"; exit 1 ;;
esac

MTX_VER="${MEDIAMTX_VERSION:-v1.11.3}"
case "$OS" in
  darwin) MTX_OS=darwin ;;
  linux) MTX_OS=linux ;;
  *) echo "unsupported os: $OS"; exit 1 ;;
esac

MTX_URL="https://github.com/bluenviron/mediamtx/releases/download/${MTX_VER}/mediamtx_${MTX_VER}_${MTX_OS}_${ARCH_TAG}.tar.gz"
echo "→ MediaMTX ${MTX_VER} (${MTX_OS}/${ARCH_TAG})"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
curl -fsSL -o "$TMP/mediamtx.tgz" "$MTX_URL"
tar -xzf "$TMP/mediamtx.tgz" -C "$TMP"
cp "$TMP/mediamtx" "$BIN/mediamtx"
chmod +x "$BIN/mediamtx"
"$BIN/mediamtx" --help >/dev/null
echo "  installed $BIN/mediamtx"

# ffmpeg: prefer copying a working local binary (static bundling is OS-specific)
if command -v ffmpeg >/dev/null 2>&1; then
  REAL="$(python3 -c "import os; print(os.path.realpath('$(command -v ffmpeg)'))")"
  cp -f "$REAL" "$BIN/ffmpeg"
  chmod +x "$BIN/ffmpeg"
  echo "  installed $BIN/ffmpeg (from $REAL)"
  "$BIN/ffmpeg" -version | head -1
else
  echo "  WARN: ffmpeg not on PATH — install via brew/apt or place a static binary at $BIN/ffmpeg"
  echo "        Transcoding H.265→H.264 will be unavailable until then; H.264 sources still work."
fi

echo "Done. pksp serve will auto-discover binaries under bin/."
