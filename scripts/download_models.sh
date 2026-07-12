#!/usr/bin/env bash
# Download / export buffalo_l ONNX weights for Rust (and warm Python InsightFace cache).
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

# 1) Prefer InsightFace cache if already present
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

# 2) Python InsightFace download then copy
if [[ ! -d "$ROOT/apps/api/.venv" ]]; then
  echo "Create venv first: python3.11 -m venv apps/api/.venv && pip install -r apps/api/requirements.txt"
  echo "Or place det_10g.onnx + w600k_r50.onnx in $DEST"
  exit 1
fi

# shellcheck disable=SC1091
source "$ROOT/apps/api/.venv/bin/activate"
export DEST
python - <<'PY'
import os, shutil
from pathlib import Path

dest = Path(os.environ["DEST"])
dest.mkdir(parents=True, exist_ok=True)
print("Downloading / verifying InsightFace buffalo_l …")
try:
    from insightface.app import FaceAnalysis
    providers = os.environ.get("ONNX_PROVIDERS", "CPUExecutionProvider").split(",")
    providers = [p.strip() for p in providers if p.strip()] or ["CPUExecutionProvider"]
    app = FaceAnalysis(name="buffalo_l", providers=providers)
    app.prepare(ctx_id=-1, det_size=(640, 640))
    print("OK: buffalo_l ready (providers=%s)" % providers)
except Exception as e:
    print("WARN: InsightFace download failed:", e)
    raise SystemExit(1)

home = Path.home() / ".insightface" / "models"
found = list(home.rglob("det_10g.onnx"))
if not found:
    print("WARN: det_10g.onnx not found under", home)
    raise SystemExit(1)
src_dir = found[0].parent
for name in ("det_10g.onnx", "w600k_r50.onnx"):
    p = src_dir / name
    if p.is_file():
        shutil.copy2(p, dest / name)
        print("copied", p, "->", dest / name)
    else:
        alt = list(src_dir.rglob(name))
        if alt:
            shutil.copy2(alt[0], dest / name)
            print("copied", alt[0], "->", dest / name)
        else:
            print("MISSING", name)
            raise SystemExit(1)
print("OK:", list(dest.iterdir()))
PY

ls -la "$DEST"
echo "Rust: cargo build -p pksp-cli --features pksp-vision/ort  (from apps/edge)"
