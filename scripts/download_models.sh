#!/usr/bin/env bash
# Pre-warm InsightFace buffalo_l model cache for demo day.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/apps/api"

if [[ ! -d .venv ]]; then
  echo "Create venv first: python3.11 -m venv apps/api/.venv && pip install -r requirements.txt"
  exit 1
fi

# shellcheck disable=SC1091
source .venv/bin/activate

python - <<'PY'
import os
print("Downloading / verifying InsightFace buffalo_l …")
try:
    from insightface.app import FaceAnalysis
    app = FaceAnalysis(name="buffalo_l", providers=["CPUExecutionProvider"])
    app.prepare(ctx_id=-1, det_size=(640, 640))
    print("OK: buffalo_l ready")
except Exception as e:
    print("WARN: model download failed:", e)
    print("Demo can still run with MOCK_VISION=true")
    raise SystemExit(1)
PY
