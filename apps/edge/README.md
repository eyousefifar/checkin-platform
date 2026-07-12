# PKSP Edge (Rust)

On-prem check-in edge runtime: REST + WebSocket + mock/real vision + supervised media.

## Build

```bash
cd apps/edge
cargo build --release
```

Binary: `target/release/pksp` (or `target/debug/pksp`).

## Run

Monorepo root `.env` often sets Python-style:

```env
DATABASE_URL=sqlite:///./data/pksp.db
DATA_DIR=./data
```

Rust **correctly resolves** `sqlite:///./data/...` to a **relative** path (`./data/pksp.db`), not `/./data` (which used to hit a read-only root filesystem).

### Recommended (explicit, isolated from Python DB)

From `apps/edge` or repo root:

```bash
export DATA_DIR=../../data/rust-edge          # from apps/edge; or ./data/rust-edge from repo root
export DATABASE_URL="sqlite:///${DATA_DIR}/pksp-rust.db?mode=rwc"
# If DATA_DIR is relative, prefer the three-slash relative form:
#   export DATABASE_URL="sqlite:///./data/rust-edge/pksp-rust.db"
export ADMIN_PASSWORD=<set-a-strong-password>
export JWT_SECRET=<generate-at-least-32-bytes>
export MOCK_VISION=true
export BIND_ADDR=127.0.0.1:8000
export CAM_IN_WEBRTC_PATH=demo
# optional: MEDIAMTX_BIN=mediamtx MEDIAMTX_CONFIG=../../configs/mediamtx.yml

./target/debug/pksp serve
# or from repo root:
#   ./apps/edge/target/debug/pksp serve
```

**Important:** if monorepo `.env` already defines `DATABASE_URL`, exporting only `DATA_DIR` does **not** change the DB file — set `DATABASE_URL` as well (or unset it to use `$DATA_DIR/pksp-rust.db`).

Health: `curl http://127.0.0.1:8000/api/health`

Point Next.js at the Rust API:

```bash
export NEXT_PUBLIC_API_URL=http://localhost:8000
export NEXT_PUBLIC_WS_URL=ws://localhost:8000/api/ws/live
export NEXT_PUBLIC_WEBRTC_BASE=http://localhost:8889
```

## Media (bundled — no separate servers)

`pksp serve` **supervises** media children so you do not run MediaMTX/GStreamer by hand:

| Binary | Location | Role |
|---|---|---|
| **MediaMTX** | `apps/edge/bin/mediamtx` | RTSP/RTMP/HLS/**WHEP** for the browser |
| **ffmpeg** | `apps/edge/bin/ffmpeg` | Optional H.265→H.264 publish into MediaMTX |

```bash
./apps/edge/scripts/download-binaries.sh   # refresh vendored bins
```

Resolution order: `MEDIAMTX_BIN` / `FFMPEG_BIN` env → `apps/edge/bin/` → PATH.

**Codec policy:** prefer `CAM_IN_H264_RTSP` (or H.264 camera path). If `CAM_IN_RTSP` is H.265/`stream1`, supervised ffmpeg publishes `cam_in_h264` and health returns that browser-safe path. `FORCE_TRANSCODE=true` forces transcoder.

**GStreamer is not required.** See `docs/media-rust-bindings.md`, `docs/deploy.md`.

## Vision pipeline

```
capture (synthetic | ffmpeg RTSP)
  → quality → match → IoU track → vote
  → smart scene (zones + walk-by)
  → attendance FSM + cooldown
  → WS detections / attendance
```

| Mode | How |
|---|---|
| Mock (default) | `MOCK_VISION=true` — synthetic frames + intensity embeddings |
| Real ONNX | `MOCK_VISION=false`, models in `$DATA_DIR/models/buffalo_l/`, build with `--features pksp-vision/ort` |
| Smart scene | `ENABLE_SMART_SCENE=true` (default), zones in `configs/zones.{camera_id}.json` |

```bash
./scripts/download_models.sh   # copies det_10g.onnx + w600k_r50.onnx into data/models/buffalo_l/
```

**Re-enroll:** if Rust embeddings are not cosine-compatible with a prior Python gallery, re-enroll all staff under Rust. Do not mix mock and real embeddings for production punches.

## Smart scene

- **Active** zone: identity vote may commit attendance  
- **Approach**: HUD `approaching`; no punch  
- **Ignore**: no vote (posters / frame edge)  
- **Walk-by**: lateral trajectory without active dwell → `walkby`, no punch  

Disable with `ENABLE_SMART_SCENE=false` for pure quality→vote→FSM parity.

## Docs

- `docs/deploy.md` — LAN runbook, systemd, backup, rollback  
- `docs/benches.md` — performance notes  
- `docs/media-rust-bindings.md` — media stack options  

## Rollback to Python

1. Stop `pksp serve` (Ctrl-C — graceful: vision + media stop).
2. Start Python API: `cd apps/api && uvicorn app.main:app --port 8000`
3. Start MediaMTX via `docker compose up -d mediamtx` as before.
4. Keep the same `apps/web` env pointing at `:8000`.

## Known limits

- buffalo_l may be non-commercial unless licensed  
- Anti-spoof not certified  
- Real SCRFD decode is best-effort; validate embeddings before relying on old gallery  
- Dual RTSP (vision + media) can load the camera — prefer lower-res vision stream when possible

Python tree under `apps/api/` is intentionally retained.

## Models / ONNX

Default `MOCK_VISION=true` needs no weights.  
For real buffalo_l, place ONNX under `$DATA_DIR/models/buffalo_l/` and set `MOCK_VISION=false`.  
If cosine parity with Python enrollments is not proven ≥0.99, **re-enroll** all staff under the Rust engine.

## Tests

```bash
cd apps/edge
cargo test
```
