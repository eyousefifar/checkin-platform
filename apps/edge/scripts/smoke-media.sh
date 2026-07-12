#!/usr/bin/env bash
# Deterministic MediaMTX publication smoke with generated test video (no camera).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MTX_BIN="${MEDIAMTX_BIN:-$ROOT/bin/mediamtx}"
FFMPEG_BIN="${FFMPEG_BIN:-$ROOT/bin/ffmpeg}"
if [[ ! -x "$MTX_BIN" ]]; then
  if command -v mediamtx >/dev/null 2>&1; then
    MTX_BIN="$(command -v mediamtx)"
  else
    echo "mediamtx not found at $ROOT/bin/mediamtx — run download-binaries.sh" >&2
    exit 1
  fi
fi
if [[ ! -x "$FFMPEG_BIN" ]]; then
  if command -v ffmpeg >/dev/null 2>&1; then
    FFMPEG_BIN="$(command -v ffmpeg)"
  else
    echo "ffmpeg not found" >&2
    exit 1
  fi
fi

port_free() {
  local proto="$1" port="$2"
  if command -v lsof >/dev/null 2>&1; then
    if [[ "$proto" == "udp" ]]; then
      ! lsof -nP -iUDP:"$port" -sUDP:Idle >/dev/null 2>&1 && \
        ! lsof -nP -iUDP:"$port" >/dev/null 2>&1
    else
      ! lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1
    fi
    return $?
  fi
  if command -v ss >/dev/null 2>&1; then
    if [[ "$proto" == "udp" ]]; then
      ! ss -uln | grep -qE "[:.]${port}\\s"
    else
      ! ss -tln | grep -qE "[:.]${port}\\s"
    fi
    return $?
  fi
  # Best effort: try binding via python
  python3 - "$proto" "$port" <<'PY'
import socket, sys
proto, port = sys.argv[1], int(sys.argv[2])
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM if proto=="udp" else socket.SOCK_STREAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
try:
    s.bind(("127.0.0.1", port))
except OSError:
    sys.exit(1)
finally:
    s.close()
sys.exit(0)
PY
}

pick_ports() {
  local tries=0
  while (( tries < 40 )); do
    tries=$((tries + 1))
    RTSP_PORT=$((20000 + RANDOM % 20000))
    RTMP_PORT=$((20000 + RANDOM % 20000))
    WEBRTC_HTTP=$((20000 + RANDOM % 20000))
    API_PORT=$((20000 + RANDOM % 20000))
    WEBRTC_UDP=$((20000 + RANDOM % 20000))
    HLS_PORT=$((20000 + RANDOM % 20000))
    # unique
    local all=("$RTSP_PORT" "$RTMP_PORT" "$WEBRTC_HTTP" "$API_PORT" "$WEBRTC_UDP" "$HLS_PORT")
    local uniq
    uniq="$(printf '%s\n' "${all[@]}" | sort -u | wc -l | tr -d ' ')"
    [[ "$uniq" == "6" ]] || continue
    port_free tcp "$RTSP_PORT" || continue
    port_free tcp "$RTMP_PORT" || continue
    port_free tcp "$WEBRTC_HTTP" || continue
    port_free tcp "$API_PORT" || continue
    port_free tcp "$HLS_PORT" || continue
    port_free udp "$WEBRTC_UDP" || continue
    return 0
  done
  echo "could not find free temporary ports" >&2
  return 1
}

pick_ports

WORKDIR="$(mktemp -d)"
MTX_PID=""
FF_PID=""
cleanup() {
  if [[ -n "${FF_PID}" ]] && kill -0 "$FF_PID" 2>/dev/null; then
    kill "$FF_PID" 2>/dev/null || true
    wait "$FF_PID" 2>/dev/null || true
  fi
  if [[ -n "${MTX_PID}" ]] && kill -0 "$MTX_PID" 2>/dev/null; then
    kill "$MTX_PID" 2>/dev/null || true
    wait "$MTX_PID" 2>/dev/null || true
  fi
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

PATH_NAME="cam_in_h264"
CFG="$WORKDIR/mediamtx.yml"
cat >"$CFG" <<EOF
logLevel: warn
api: yes
apiAddress: 127.0.0.1:${API_PORT}
rtsp: yes
rtspAddress: 127.0.0.1:${RTSP_PORT}
rtspTransports: [tcp]
rtmp: yes
rtmpAddress: 127.0.0.1:${RTMP_PORT}
hls: yes
hlsAddress: 127.0.0.1:${HLS_PORT}
webrtc: yes
webrtcAddress: 127.0.0.1:${WEBRTC_HTTP}
webrtcLocalUDPAddress: 127.0.0.1:${WEBRTC_UDP}
webrtcAdditionalHosts: [127.0.0.1]
srt: no
playback: no
metrics: no
pprof: no
paths:
  ${PATH_NAME}:
    source: publisher
EOF

"$MTX_BIN" "$CFG" >"$WORKDIR/mtx.log" 2>&1 &
MTX_PID=$!

# Wait for API
for _ in $(seq 1 50); do
  if curl -fsS "http://127.0.0.1:${API_PORT}/v3/paths/get/${PATH_NAME}" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$MTX_PID" 2>/dev/null; then
    echo "MediaMTX exited early" >&2
    cat "$WORKDIR/mtx.log" >&2 || true
    exit 1
  fi
  sleep 0.1
done

# Publish lavfi testsrc H.264 → RTMP
"$FFMPEG_BIN" -hide_banner -loglevel error \
  -re -f lavfi -i "testsrc=size=320x240:rate=15" \
  -an -c:v libx264 -preset ultrafast -tune zerolatency -t 30 \
  -f flv "rtmp://127.0.0.1:${RTMP_PORT}/${PATH_NAME}" \
  >"$WORKDIR/ff.log" 2>&1 &
FF_PID=$!

READY=0
for _ in $(seq 1 80); do
  RESP="$(curl -fsS "http://127.0.0.1:${API_PORT}/v3/paths/get/${PATH_NAME}" 2>/dev/null || true)"
  if echo "$RESP" | grep -q '"ready":true' && echo "$RESP" | grep -q '"source":{'; then
    READY=1
    break
  fi
  # also accept source non-null without nested brace edge cases
  if echo "$RESP" | grep -q '"ready":true' && ! echo "$RESP" | grep -q '"source":null'; then
    READY=1
    break
  fi
  sleep 0.25
done

if [[ "$READY" != "1" ]]; then
  echo "path never became ready" >&2
  echo "api: $(curl -sS "http://127.0.0.1:${API_PORT}/v3/paths/get/${PATH_NAME}" || true)" >&2
  cat "$WORKDIR/mtx.log" >&2 || true
  cat "$WORKDIR/ff.log" >&2 || true
  exit 1
fi

echo "OK smoke-media path=${PATH_NAME} api=127.0.0.1:${API_PORT}"
# cleanup trap stops only our PIDs and removes WORKDIR
