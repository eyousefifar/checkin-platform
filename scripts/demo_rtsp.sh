#!/usr/bin/env bash
# Publish a mock camera into MediaMTX over RTSP (H.264 / TCP).
#
# Sources (SOURCE=…):
#   webcam      macOS AVFoundation camera (best for live face match testing)
#   chokepoint  loop research face crops under data/benchmarks/chokepoint (detection demo)
#   sample      loop SAMPLE=/path/to.mp4 (or any video file)
#   testsrc     color bars (media smoke only — no faces)
#
# Default path is cam_in so vision (CAM_IN_RTSP) and browser (CAM_IN_WEBRTC_PATH)
# can share one stream. Override with MTX_URL.
#
# Requires: ffmpeg; MediaMTX already listening (pksp serve or docker compose).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MTX_URL="${MTX_URL:-rtsp://127.0.0.1:8554/cam_in}"
SOURCE="${SOURCE:-}"
SAMPLE="${SAMPLE:-}"
WEBCAM_DEVICE="${WEBCAM_DEVICE:-0}"
CHOKEPOINT_DIR="${CHOKEPOINT_DIR:-}"

# Infer SOURCE from SAMPLE for backward compatibility when SOURCE unset.
if [[ -z "$SOURCE" ]]; then
  if [[ -n "$SAMPLE" && -f "$SAMPLE" ]]; then
    SOURCE=sample
  else
    SOURCE=testsrc
  fi
fi

pick_chokepoint_dir() {
  if [[ -n "$CHOKEPOINT_DIR" && -d "$CHOKEPOINT_DIR" ]]; then
    echo "$CHOKEPOINT_DIR"
    return 0
  fi
  local candidates=(
    "$ROOT/data/benchmarks/chokepoint/P1E_S1_C1/0013"
    "$ROOT/data/benchmarks/chokepoint/P1E_S1_C1/0006"
    "$ROOT/data/benchmarks/chokepoint/P1E_S1_C1/0001"
    "$ROOT/data/benchmarks/chokepoint/P1E_S3_C1/0003"
  )
  local d
  for d in "${candidates[@]}"; do
    if compgen -G "$d/*.pgm" >/dev/null 2>&1; then
      echo "$d"
      return 0
    fi
  done
  # First directory that contains any .pgm
  d="$(find "$ROOT/data/benchmarks/chokepoint" -type f -name '*.pgm' 2>/dev/null | head -1 | xargs -I{} dirname {} 2>/dev/null || true)"
  if [[ -n "$d" && -d "$d" ]]; then
    echo "$d"
    return 0
  fi
  return 1
}

encode_rtsp() {
  # Shared H.264 publisher flags for low-latency LAN demos.
  exec ffmpeg -hide_banner -loglevel warning -nostdin "$@" \
    -an \
    -c:v libx264 -preset ultrafast -tune zerolatency -pix_fmt yuv420p \
    -g 30 -bf 0 \
    -f rtsp -rtsp_transport tcp "$MTX_URL"
}

case "$SOURCE" in
  webcam)
    echo "Publishing macOS webcam (device $WEBCAM_DEVICE) → $MTX_URL"
    # video_size / framerate are soft constraints; camera may negotiate lower.
    encode_rtsp -f avfoundation -framerate 15 -video_size 1280x720 \
      -i "${WEBCAM_DEVICE}:none" \
      -vf "format=yuv420p"
    ;;
  chokepoint)
    dir="$(pick_chokepoint_dir)" || {
      echo "No Chokepoint .pgm frames found under data/benchmarks/chokepoint" >&2
      exit 1
    }
    echo "Looping Chokepoint face sequence $dir → $MTX_URL"
    # Crops are ~96×96 greyscale. Place a ~240px face on a 1280×720 canvas so
    # SCRFD sees a realistic face size in a normal RTSP frame (not a full-bleed
    # pixelated square).
    encode_rtsp -re -stream_loop -1 -framerate 10 -pattern_type glob \
      -i "$dir/*.pgm" \
      -vf "format=yuv420p,scale=240:240:flags=bicubic,pad=1280:720:(ow-iw)/2:(oh-ih)/2:color=0x404040"
    ;;
  sample)
    if [[ -z "$SAMPLE" || ! -f "$SAMPLE" ]]; then
      echo "SOURCE=sample requires SAMPLE=/path/to/video" >&2
      exit 1
    fi
    echo "Looping sample video $SAMPLE → $MTX_URL"
    encode_rtsp -re -stream_loop -1 -i "$SAMPLE" \
      -vf "format=yuv420p"
    ;;
  testsrc)
    echo "Publishing testsrc pattern → $MTX_URL (no faces — media smoke only)"
    echo "Tip: SOURCE=webcam or SOURCE=chokepoint for face detection"
    encode_rtsp -re -f lavfi -i "testsrc=size=1280x720:rate=15" \
      -vf "format=yuv420p"
    ;;
  *)
    echo "Unknown SOURCE='$SOURCE' (expected webcam|chokepoint|sample|testsrc)" >&2
    exit 1
    ;;
esac
