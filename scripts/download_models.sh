#!/usr/bin/env bash
# Download / export buffalo_l ONNX weights for Rust.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA_DIR="${DATA_DIR:-$ROOT/data}"
DEST="$DATA_DIR/models/buffalo_l"
mkdir -p "$DEST"

echo "Target: $DEST"

copy_if_present() {
  local src="$1"
  if [[ -f "$src/det_10g.onnx" && -f "$src/w600k_r50.onnx" ]]; then
    cp -f "$src/det_10g.onnx" "$DEST/"
    cp -f "$src/w600k_r50.onnx" "$DEST/"
    echo "OK: copied det_10g.onnx + w600k_r50.onnx from $src"
    return 0
  fi
  return 1
}

# 1) Prefer an existing model cache if present.
for d in \
  "$HOME/.insightface/models/buffalo_l" \
  "$HOME/.insightface/models/buffalo_l/buffalo_l"
do
  if copy_if_present "$d"; then
    ls -la "$DEST"
    exit 0
  fi
done

# Nested search
if [[ -d "$HOME/.insightface/models" ]]; then
  found="$(find "$HOME/.insightface/models" -name 'det_10g.onnx' 2>/dev/null | head -1 || true)"
  if [[ -n "$found" ]]; then
    copy_if_present "$(dirname "$found")" && ls -la "$DEST" && exit 0
  fi
fi

# 2) Download the official buffalo_l release archive.
command -v curl >/dev/null || { echo "curl is required" >&2; exit 1; }
command -v unzip >/dev/null || { echo "unzip is required" >&2; exit 1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fL --retry 3 -o "$tmp/buffalo_l.zip" \
  https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_l.zip
unzip -q "$tmp/buffalo_l.zip" -d "$tmp"
copy_if_present "$tmp/buffalo_l" || { echo "buffalo_l archive is missing required models" >&2; exit 1; }

ls -la "$DEST"
echo "Rust: cargo build -p pksp-cli --features pksp-vision/ort  (from apps/edge)"
