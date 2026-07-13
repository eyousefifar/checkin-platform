# PKSP Edge (Rust)

On-prem check-in edge runtime: REST + WebSocket + mock/real vision + supervised media.

## Build

```bash
cd apps/edge
cargo build --release
```

Binary: `target/release/pksp` (or `target/debug/pksp`).

## Run

Monorepo root `.env` may use a relative SQLite URL:

```env
DATABASE_URL=sqlite:///./data/pksp.db
DATA_DIR=./data
```

Rust **correctly resolves** `sqlite:///./data/...` to a **relative** path (`./data/pksp.db`), not `/./data` (which used to hit a read-only root filesystem).

### Recommended (explicit, isolated database)

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

**Publication policy** is explicit via `MEDIA_SOURCE_MODE` (default `external`):

| Mode | Input | Behavior |
|---|---|---|
| `external` | — | MediaMTX only; demo/operator publishes to `CAM_IN_WEBRTC_PATH` |
| `copy` | `CAM_IN_H264_RTSP` | Stream-copy H.264 → `MEDIA_PUBLISH_PATH` (`cam_in_h264`) |
| `transcode` | `CAM_IN_RTSP` | Encode H.264 → `MEDIA_PUBLISH_PATH` |

Health sets `media.publication` to `ready` only when the MediaMTX API reports a live publisher on the publish path. Source RTSP URLs are never logged or returned in API status.

Verify: `./apps/edge/scripts/smoke-media.sh` (generated test video).

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

**Re-enroll:** do not mix mock and real embeddings for production punches.

## Smart scene

- **Active** zone: identity vote may commit attendance  
- **Approach**: HUD `approaching`; no punch  
- **Ignore**: no vote (posters / frame edge)  
- **Walk-by**: lateral trajectory without active dwell → `walkby`, no punch  

Disable with `ENABLE_SMART_SCENE=false` for pure quality→vote→FSM parity.

## Docs

- `docs/deploy.md` — LAN runbook, systemd, backup, recovery
- `docs/benches.md` — performance notes  
- `docs/media-rust-bindings.md` — media stack options  

## Recovery

Stop `pksp serve`, restore the prior Rust binary and SQLite backup, then restart the service. Keep the same `apps/web` env pointing at `:8000`.

## Known limits

- buffalo_l may be non-commercial unless licensed  
- Anti-spoof not certified  
- Real SCRFD decode is best-effort; validate embeddings before relying on old gallery  
- Dual RTSP (vision + media) can load the camera — prefer lower-res vision stream when possible

## Models / ONNX

Default `MOCK_VISION=true` needs no weights.  
For real buffalo_l, place ONNX under `$DATA_DIR/models/buffalo_l/` and set `MOCK_VISION=false`.  
Re-enroll employees when changing model or embedding settings.

## Tests

```bash
cd apps/edge
cargo test
```
