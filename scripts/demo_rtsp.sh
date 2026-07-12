#!/usr/bin/env bash
# Publish a demo RTSP stream into MediaMTX path "demo".
# Requires: docker compose mediamtx up, ffmpeg installed.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MTX_URL="${MTX_URL:-rtsp://127.0.0.1:8554/demo}"
SAMPLE="${SAMPLE:-}"

if [[ -n "$SAMPLE" && -f "$SAMPLE" ]]; then
  echo "Looping sample video → $MTX_URL"
  exec ffmpeg -re -stream_loop -1 -i "$SAMPLE" \
    -c:v libx264 -preset ultrafast -tune zerolatency -pix_fmt yuv420p \
    -an -f rtsp -rtsp_transport tcp "$MTX_URL"
fi

echo "No SAMPLE mp4 set — publishing testsrc pattern → $MTX_URL"
echo "Tip: SAMPLE=/path/to/lobby.mp4 $0"
exec ffmpeg -re -f lavfi -i "testsrc=size=1280x720:rate=15" \
  -f lavfi -i "sine=frequency=1000:sample_rate=44100" \
  -c:v libx264 -preset ultrafast -tune zerolatency -pix_fmt yuv420p \
  -c:a aac -shortest \
  -f rtsp -rtsp_transport tcp "$MTX_URL"
