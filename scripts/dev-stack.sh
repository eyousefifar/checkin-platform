#!/usr/bin/env bash
# Start / stop / status for a full local PKSP demo stack:
#   pksp serve (MediaMTX + vision + API) + FFmpeg mock RTSP + Next.js
#
# Usage:
#   ./scripts/dev-stack.sh start [webcam|chokepoint|testsrc|sample]
#   ./scripts/dev-stack.sh stop
#   ./scripts/dev-stack.sh status
#
# Env:
#   SAMPLE=/path/to.mp4   (with start sample)
#   ADMIN_PASSWORD / JWT_SECRET  (optional; defaults from .env or loopback demos)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOG_DIR="${LOG_DIR:-$ROOT/data/live-logs}"
PKSP_BIN="${PKSP_BIN:-$ROOT/apps/edge/target/release/pksp}"
MTX_BIN="${MTX_BIN:-$ROOT/apps/edge/bin/mediamtx}"
MTX_CFG="${MTX_CFG:-$ROOT/configs/mediamtx.yml}"
API_URL="${API_URL:-http://127.0.0.1:8000}"
WEB_URL="${WEB_URL:-http://localhost:3000}"
MTX_API="${MTX_API:-http://127.0.0.1:9997}"

mkdir -p "$LOG_DIR"

cmd="${1:-status}"
source_arg="${2:-}"

load_root_env() {
  if [[ -f "$ROOT/.env" ]]; then
    set -a
    # shellcheck disable=SC1091
    source "$ROOT/.env"
    set +a
  fi
}

pid_alive() {
  local f="$1"
  [[ -f "$f" ]] || return 1
  local p
  p="$(cat "$f" 2>/dev/null || true)"
  [[ -n "$p" ]] || return 1
  kill -0 "$p" 2>/dev/null
}

stop_pidfile() {
  local name="$1"
  local f="$LOG_DIR/${name}.pid"
  if pid_alive "$f"; then
    local p
    p="$(cat "$f")"
    echo "Stopping $name (pid $p)…"
    kill "$p" 2>/dev/null || true
    # Give graceful exit; then force
    for _ in 1 2 3 4 5; do
      kill -0 "$p" 2>/dev/null || break
      sleep 0.3
    done
    if kill -0 "$p" 2>/dev/null; then
      kill -9 "$p" 2>/dev/null || true
    fi
  fi
  rm -f "$f"
}

stop_all() {
  stop_pidfile "ffmpeg-rtsp"
  stop_pidfile "next"
  stop_pidfile "pksp"
  # Orphan cleanup by port (MediaMTX + API + Next + WebRTC)
  # pksp's supervised mediamtx can outlive the parent if killed hard.
  for port in 8000 3000 8554 8889 9997 1935 8888; do
    local p
    p="$(lsof -tiTCP:$port -sTCP:LISTEN 2>/dev/null || true)"
    if [[ -n "$p" ]]; then
      echo "Freeing port $port (pid $p)…"
      # shellcheck disable=SC2086
      kill $p 2>/dev/null || true
    fi
  done
  # Leftover publishers (name-only to avoid killing this shell)
  local fp
  for fp in $(pgrep -x ffmpeg 2>/dev/null || true); do
    # Only stop ffmpeg that looks like our RTSP publisher
    if ps -p "$fp" -o args= 2>/dev/null | grep -q 'rtsp://127.0.0.1:8554'; then
      echo "Stopping leftover ffmpeg publisher (pid $fp)…"
      kill "$fp" 2>/dev/null || true
    fi
  done
  sleep 0.5
  echo "Stack stopped."
}

status_one() {
  local name="$1"
  local f="$LOG_DIR/${name}.pid"
  if pid_alive "$f"; then
    echo "  $name: running (pid $(cat "$f"))"
  else
    echo "  $name: stopped"
  fi
}

do_status() {
  echo "PKSP dev stack ($LOG_DIR)"
  status_one "pksp"
  status_one "ffmpeg-rtsp"
  status_one "next"
  echo
  if curl -sf "$API_URL/api/health" >/dev/null 2>&1; then
    echo "  health: OK  $API_URL/api/health"
    curl -sf "$API_URL/api/health" 2>/dev/null | head -c 500 || true
    echo
  else
    echo "  health: unreachable ($API_URL/api/health)"
  fi
  if curl -sf "$MTX_API/v3/paths/list" >/dev/null 2>&1; then
    echo "  mediamtx paths:"
    curl -sf "$MTX_API/v3/paths/list" 2>/dev/null | head -c 800 || true
    echo
  else
    echo "  mediamtx API: unreachable ($MTX_API)"
  fi
  code="$(curl -s -o /dev/null -w '%{http_code}' "$WEB_URL" 2>/dev/null || echo 000)"
  echo "  web: HTTP $code  $WEB_URL"
}

pick_source() {
  local want="${1:-}"
  if [[ -n "$want" ]]; then
    echo "$want"
    return
  fi
  if [[ -n "${SOURCE:-}" ]]; then
    echo "$SOURCE"
    return
  fi
  # Prefer webcam when avfoundation lists a camera; else chokepoint; else testsrc
  if ffmpeg -f avfoundation -list_devices true -i "" 2>&1 | grep -q "MacBook\|Camera\|FaceTime\|AVFoundation video devices"; then
    # Presence of devices list ≠ permission; still try webcam first at start time.
    echo "webcam"
    return
  fi
  if find "$ROOT/data/benchmarks/chokepoint" -name '*.pgm' 2>/dev/null | head -1 | grep -q .; then
    echo "chokepoint"
    return
  fi
  echo "testsrc"
}

wait_http() {
  local url="$1"
  local label="$2"
  local tries="${3:-40}"
  local i
  for i in $(seq 1 "$tries"); do
    if curl -sf "$url" >/dev/null 2>&1; then
      echo "  $label ready ($url)"
      return 0
    fi
    sleep 0.5
  done
  echo "  WARNING: $label not ready after ${tries} tries ($url)" >&2
  return 1
}

start_pksp() {
  if [[ ! -x "$PKSP_BIN" ]]; then
    echo "Building release pksp…"
    (cd "$ROOT/apps/edge" && cargo build --release -p pksp-cli)
  fi
  if [[ ! -x "$MTX_BIN" ]]; then
    echo "Downloading MediaMTX binary…"
    "$ROOT/apps/edge/scripts/download-binaries.sh"
  fi
  if [[ ! -f "$ROOT/data/models/buffalo_l/det_10g.onnx" ]]; then
    echo "Downloading face models…"
    (cd "$ROOT" && ./scripts/download_models.sh)
  fi

  # Align vision RTSP + browser WHEP on the same path.
  # Force cam_in even if monorepo .env still points at the old "demo" path —
  # ${VAR:-default} would keep the wrong .env value and break the HUD overlay.
  export DATA_DIR="${DATA_DIR:-$ROOT/data}"
  export DATABASE_URL="${DEV_DATABASE_URL:-sqlite:///./data/pksp-live.db?mode=rwc}"
  export BIND_ADDR="${BIND_ADDR:-127.0.0.1:8000}"
  export APP_TIMEZONE="${APP_TIMEZONE:-Asia/Tehran}"
  export CAM_IN_RTSP="rtsp://127.0.0.1:8554/cam_in"
  export CAM_IN_WEBRTC_PATH="cam_in"
  export MEDIA_SOURCE_MODE="external"
  export MEDIAMTX_BIN="${MEDIAMTX_BIN:-$MTX_BIN}"
  export MEDIAMTX_CONFIG="${MEDIAMTX_CONFIG:-$MTX_CFG}"
  export MEDIAMTX_API_ADDR="127.0.0.1:9997"
  export ZONE_CONFIG_DIR="${ZONE_CONFIG_DIR:-$ROOT/configs}"
  # Off for demos so faces anywhere in frame are eligible (zones still work if set true).
  export ENABLE_SMART_SCENE="${ENABLE_SMART_SCENE:-false}"
  export VISION_ENABLED="true"
  export CORS_ORIGINS="${CORS_ORIGINS:-http://localhost:3000}"
  export ADMIN_PASSWORD="${ADMIN_PASSWORD:-change-me}"
  # Loopback accepts demo secrets; keep ≥32 chars for consistency.
  export JWT_SECRET="${JWT_SECRET:-dev-jwt-secret-change-me-32b!!}"
  if [[ ${#JWT_SECRET} -lt 32 ]]; then
    export JWT_SECRET="dev-jwt-secret-change-me-32b!!"
  fi

  # Absolute-ish DATA_DIR when using monorepo root cwd
  mkdir -p "$DATA_DIR"

  echo "Starting pksp serve…"
  (
    cd "$ROOT"
    # Force overrides after dotenv inside binary by exporting in this process.
    nohup env \
      DATA_DIR="$DATA_DIR" \
      DATABASE_URL="$DATABASE_URL" \
      BIND_ADDR="$BIND_ADDR" \
      APP_TIMEZONE="$APP_TIMEZONE" \
      CAM_IN_RTSP="$CAM_IN_RTSP" \
      CAM_IN_WEBRTC_PATH="$CAM_IN_WEBRTC_PATH" \
      MEDIA_SOURCE_MODE="$MEDIA_SOURCE_MODE" \
      MEDIAMTX_BIN="$MEDIAMTX_BIN" \
      MEDIAMTX_CONFIG="$MEDIAMTX_CONFIG" \
      MEDIAMTX_API_ADDR="$MEDIAMTX_API_ADDR" \
      ZONE_CONFIG_DIR="$ZONE_CONFIG_DIR" \
      ENABLE_SMART_SCENE="$ENABLE_SMART_SCENE" \
      VISION_ENABLED="$VISION_ENABLED" \
      CORS_ORIGINS="$CORS_ORIGINS" \
      ADMIN_PASSWORD="$ADMIN_PASSWORD" \
      JWT_SECRET="$JWT_SECRET" \
      "$PKSP_BIN" serve \
      >"$LOG_DIR/pksp.log" 2>&1 &
    echo $! >"$LOG_DIR/pksp.pid"
  )
  wait_http "$API_URL/api/health" "pksp health" 60 || {
    echo "---- pksp.log (tail) ----" >&2
    tail -40 "$LOG_DIR/pksp.log" >&2 || true
    return 1
  }
}

start_ffmpeg() {
  local src="$1"
  export SOURCE="$src"
  export MTX_URL="${MTX_URL:-rtsp://127.0.0.1:8554/cam_in}"
  # SAMPLE may already be set for sample mode

  echo "Starting FFmpeg mock RTSP (SOURCE=$src)…"
  (
    cd "$ROOT"
    nohup env SOURCE="$SOURCE" SAMPLE="${SAMPLE:-}" MTX_URL="$MTX_URL" \
      "$ROOT/scripts/demo_rtsp.sh" \
      >"$LOG_DIR/ffmpeg-rtsp.log" 2>&1 &
    echo $! >"$LOG_DIR/ffmpeg-rtsp.pid"
  )
  sleep 1.5
  if ! pid_alive "$LOG_DIR/ffmpeg-rtsp.pid"; then
    echo "FFmpeg publisher exited early. Log:" >&2
    tail -30 "$LOG_DIR/ffmpeg-rtsp.log" >&2 || true
    if [[ "$src" == "webcam" ]]; then
      echo "Webcam failed (permission?). Falling back to chokepoint…" >&2
      start_ffmpeg chokepoint
      return
    fi
    if [[ "$src" == "chokepoint" ]]; then
      echo "Chokepoint failed. Falling back to testsrc…" >&2
      start_ffmpeg testsrc
      return
    fi
    return 1
  fi

  # Wait for MediaMTX to see a ready source on cam_in
  local i ready=0
  for i in $(seq 1 30); do
    if curl -sf "$MTX_API/v3/paths/get/cam_in" 2>/dev/null | grep -q '"ready"[[:space:]]*:[[:space:]]*true'; then
      ready=1
      break
    fi
    # older API shape
    if curl -sf "$MTX_API/v3/paths/list" 2>/dev/null | grep -q 'cam_in'; then
      if curl -sf "$MTX_API/v3/paths/list" 2>/dev/null | grep -q '"ready"'; then
        ready=1
        break
      fi
    fi
    sleep 0.5
  done
  if [[ "$ready" -eq 1 ]]; then
    echo "  MediaMTX path cam_in is ready"
  else
    echo "  WARNING: cam_in not reported ready yet — check $LOG_DIR/ffmpeg-rtsp.log"
    tail -15 "$LOG_DIR/ffmpeg-rtsp.log" || true
  fi
}

start_next() {
  echo "Starting Next.js…"
  (
    cd "$ROOT/apps/web"
    nohup env \
      NEXT_PUBLIC_API_URL="${NEXT_PUBLIC_API_URL:-http://localhost:8000}" \
      NEXT_PUBLIC_WS_URL="${NEXT_PUBLIC_WS_URL:-ws://localhost:8000/api/ws/live}" \
      NEXT_PUBLIC_WEBRTC_BASE="${NEXT_PUBLIC_WEBRTC_BASE:-http://localhost:8889}" \
      npm run dev \
      >"$LOG_DIR/next.log" 2>&1 &
    echo $! >"$LOG_DIR/next.pid"
  )
  wait_http "$WEB_URL" "Next.js" 60 || {
    echo "---- next.log (tail) ----" >&2
    tail -40 "$LOG_DIR/next.log" >&2 || true
    return 1
  }
}

print_howto() {
  local src="$1"
  cat <<EOF

════════════════════════════════════════════════════════════
  PKSP local demo is up
════════════════════════════════════════════════════════════
  UI:        $WEB_URL
  API:       $API_URL/api/health
  Login:     password from ADMIN_PASSWORD (default: change-me)
  RTSP mock: SOURCE=$src → rtsp://127.0.0.1:8554/cam_in
  WHEP path: cam_in  (aligned with vision)
  Logs:      $LOG_DIR/

  Face testing checklist
  1) Open $WEB_URL  → MONITOR should show live video
  2) Detection: face boxes / HUD on the camera tile (WS)
  3) Recognition:
       - Go to CONFIGURE → Add employee
       - Use guided webcam capture (5 poses) or upload photos
       - Return to MONITOR and show the same face to the RTSP source
         (if SOURCE=webcam, look at the MacBook camera)
  4) Events: click a live event for match-reveal / snapshot

  SOURCE notes
  - webcam:     best full test (your face)
  - chokepoint: offline face-detection loop (research crops)
  - testsrc:    no faces — media plumbing only
  - sample:     SAMPLE=/path/to.mp4 ./scripts/dev-stack.sh start sample

  Stop:  ./scripts/dev-stack.sh stop
  Status:./scripts/dev-stack.sh status
════════════════════════════════════════════════════════════
EOF
}

do_start() {
  load_root_env
  local src
  src="$(pick_source "$source_arg")"

  # Clean slate so CAM_IN_WEBRTC_PATH / RTSP always match this run.
  stop_all
  start_pksp
  start_ffmpeg "$src"
  start_next

  # Detect actual publisher if fallback happened
  if grep -q "Chokepoint\|chokepoint" "$LOG_DIR/ffmpeg-rtsp.log" 2>/dev/null; then
    src="chokepoint"
  elif grep -q "testsrc" "$LOG_DIR/ffmpeg-rtsp.log" 2>/dev/null; then
    src="testsrc"
  elif grep -q "webcam" "$LOG_DIR/ffmpeg-rtsp.log" 2>/dev/null; then
    src="webcam"
  fi

  print_howto "$src"
  do_status
}

case "$cmd" in
  start) do_start ;;
  stop) stop_all ;;
  status) do_status ;;
  restart)
    stop_all
    shift || true
    source_arg="${1:-}"
    do_start
    ;;
  *)
    echo "Usage: $0 {start|stop|status|restart} [webcam|chokepoint|testsrc|sample]" >&2
    exit 1
    ;;
esac
