#!/usr/bin/env bash
# Download public sample faces, enroll them into a running pksp API, and
# build a looping RTSP slideshow video for recognition demos.
#
# Requires: curl, ffmpeg, a running stack (./scripts/dev-stack.sh start …)
# Data lands under data/demo-faces and data/demo-rtsp (gitignored via data/*).
#
# Usage:
#   ./scripts/dev-stack.sh start testsrc   # or any source first
#   ./scripts/seed_demo_faces.sh
#   SAMPLE=./data/demo-rtsp/demo_faces.mp4 ./scripts/dev-stack.sh start sample
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
API="${API_URL:-http://127.0.0.1:8000}"
PASS="${ADMIN_PASSWORD:-change-me}"
ONLINE="$ROOT/data/demo-faces/online"
RTSP_DIR="$ROOT/data/demo-rtsp"
BASE_EX="https://raw.githubusercontent.com/ageitgey/face_recognition/master/examples"

mkdir -p "$ONLINE" "$RTSP_DIR"

echo "Logging in to $API …"
TOKEN=$(curl -sf -X POST "$API/api/auth/login" -H 'Content-Type: application/json' \
  -d "{\"password\":\"$PASS\"}" | python3 -c "import sys,json; print(json.load(sys.stdin)['access_token'])")

download() {
  local url="$1" out="$2"
  if [[ -f "$out" && -s "$out" ]]; then
    echo "  keep $(basename "$out")"
    return 0
  fi
  echo "  get $(basename "$out")"
  curl -fsSL -o "$out" "$url"
}

echo "Downloading public sample faces…"
download "$BASE_EX/obama.jpg" "$ONLINE/obama.jpg"
download "$BASE_EX/obama2.jpg" "$ONLINE/obama2.jpg"
download "$BASE_EX/obama_small.jpg" "$ONLINE/obama_small.jpg"
download "$BASE_EX/biden.jpg" "$ONLINE/biden.jpg"
download "https://raw.githubusercontent.com/opencv/opencv/4.x/samples/data/lena.jpg" "$ONLINE/lena.jpg"

make_variants() {
  local srcdir="$1" outdir="$2" maxn="${3:-8}"
  mkdir -p "$outdir"
  find "$outdir" -type f -delete 2>/dev/null || true
  local n=0 src
  for src in "$srcdir"/*; do
    [[ -f "$src" ]] || continue
    for scale in 1.0 0.97 1.03; do
      for bright in 0 0.03 -0.03; do
        n=$((n + 1))
        ffmpeg -y -hide_banner -loglevel error -i "$src" \
          -vf "scale=iw*${scale}:ih*${scale},eq=brightness=${bright},format=yuv420p" \
          "$outdir/v${n}.jpg"
        if [[ $n -ge $maxn ]]; then
          echo "  $outdir: $n variants"
          return 0
        fi
      done
    done
  done
  echo "  $outdir: $n variants"
}

echo "Building enrollment variants…"
mkdir -p "$ROOT/data/demo-faces/obama_src" "$ROOT/data/demo-faces/biden_src" "$ROOT/data/demo-faces/lena_src"
cp -f "$ONLINE/obama.jpg" "$ONLINE/obama2.jpg" "$ONLINE/obama_small.jpg" "$ROOT/data/demo-faces/obama_src/"
cp -f "$ONLINE/biden.jpg" "$ROOT/data/demo-faces/biden_src/"
cp -f "$ONLINE/lena.jpg" "$ROOT/data/demo-faces/lena_src/"
make_variants "$ROOT/data/demo-faces/obama_src" "$ROOT/data/demo-faces/obama_enroll" 8
make_variants "$ROOT/data/demo-faces/biden_src" "$ROOT/data/demo-faces/biden_enroll" 8
make_variants "$ROOT/data/demo-faces/lena_src" "$ROOT/data/demo-faces/lena_enroll" 8

enroll() {
  local code="$1" name="$2" dir="$3"
  echo "Enrolling $code ($name)…"
  local resp id
  resp=$(curl -s -X POST "$API/api/employees" -H "Authorization: Bearer $TOKEN" \
    -H 'Content-Type: application/json' \
    -d "{\"employee_code\":\"$code\",\"full_name\":\"$name\",\"department\":\"Demo Faces\"}")
  id=$(echo "$resp" | python3 -c "import sys,json
try:
  print(json.load(sys.stdin).get('id') or '')
except Exception:
  print('')" 2>/dev/null || true)
  if [[ -z "$id" ]]; then
    id=$(curl -sf "$API/api/employees" -H "Authorization: Bearer $TOKEN" | python3 -c "
import sys,json
for e in json.load(sys.stdin):
  if e.get('employee_code')=='$code':
    print(e['id']); break
")
  fi
  [[ -n "$id" ]] || { echo "failed to create/find $code: $resp" >&2; return 1; }
  local args=()
  local f
  for f in "$dir"/v*.jpg; do
    args+=(-F "files=@${f};type=image/jpeg")
  done
  curl -sf -X POST "$API/api/employees/$id/images" -H "Authorization: Bearer $TOKEN" \
    "${args[@]}" | python3 -c "
import sys,json
d=json.load(sys.stdin)
print(f\"  id=$id ready={d.get('embedding_ready')} usable={d.get('usable')} used={d.get('num_images_used')} rejected={len(d.get('rejected') or [])}\")
"
}

enroll "DEMO-OBAMA" "Barack Obama (demo)" "$ROOT/data/demo-faces/obama_enroll"
enroll "DEMO-BIDEN" "Joe Biden (demo)" "$ROOT/data/demo-faces/biden_enroll"
enroll "DEMO-LENA" "Lena (demo)" "$ROOT/data/demo-faces/lena_enroll"

echo "Building RTSP slideshow video…"
i=0
for src in \
  "$ONLINE/obama.jpg" \
  "$ONLINE/obama2.jpg" \
  "$ONLINE/biden.jpg" \
  "$ONLINE/lena.jpg" \
  "$ONLINE/obama_small.jpg" \
  "$ONLINE/biden.jpg" \
  "$ONLINE/lena.jpg" \
  "$ONLINE/obama.jpg"
do
  i=$((i + 1))
  ffmpeg -y -hide_banner -loglevel error -i "$src" \
    -vf "scale=w=min(iw*720/ih\,960):h=min(720\,ih*960/iw),pad=1280:720:(ow-iw)/2:(oh-ih)/2:color=0x1a1a1e,format=yuv420p" \
    -frames:v 1 "$RTSP_DIR/frame_${i}.jpg"
done

rm -f "$RTSP_DIR/list.txt" "$RTSP_DIR/demo_faces.mp4"
for f in "$RTSP_DIR"/frame_*.jpg; do
  echo "file '$(basename "$f")'" >>"$RTSP_DIR/list.txt"
  echo "duration 2.5" >>"$RTSP_DIR/list.txt"
done
echo "file 'frame_${i}.jpg'" >>"$RTSP_DIR/list.txt"

(
  cd "$RTSP_DIR"
  ffmpeg -y -hide_banner -loglevel error -f concat -safe 0 -i list.txt \
    -vf "fps=15,format=yuv420p" -c:v libx264 -preset ultrafast -tune zerolatency -pix_fmt yuv420p \
    demo_faces.mp4
)

echo
echo "Gallery:"
curl -sf "$API/api/health" | python3 -c "import sys,json; print('  gallery_size=', json.load(sys.stdin).get('gallery_size'))"
curl -sf "$API/api/employees" -H "Authorization: Bearer $TOKEN" | python3 -c "
import sys,json
for e in json.load(sys.stdin):
  if str(e.get('employee_code','')).startswith('DEMO'):
    print(f\"  {e['employee_code']}: ready={e.get('embedding_ready')} usable={e.get('usable_images')}\")
"

cat <<EOF

Demo media ready:
  Video:  $RTSP_DIR/demo_faces.mp4
  Faces:  $ONLINE/

Start (or switch) the mock camera to this video:

  SAMPLE=$RTSP_DIR/demo_faces.mp4 ./scripts/dev-stack.sh start sample

Then open http://localhost:3000 — Monitor should label:
  DEMO-OBAMA, DEMO-BIDEN, DEMO-LENA as the slideshow cycles.
EOF
