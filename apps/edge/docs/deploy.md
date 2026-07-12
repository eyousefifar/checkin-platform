# Deploy notes (LAN edge)

## Quick start

```bash
# 1. Binaries
./apps/edge/scripts/download-binaries.sh

# 2. Models (real vision)
export DATA_DIR=./data
./scripts/download_models.sh
# Build with: cargo build -p pksp-cli --release --features ort
# Or: cd apps/edge && cargo build --release -p pksp-cli
# Note: enable ort feature on pksp-vision via: cargo build -p pksp-cli --features pksp-vision/ort

# 3. Env
export DATABASE_URL="sqlite:///./data/pksp-rust.db?mode=rwc"
export DATA_DIR=./data
export MOCK_VISION=true          # theater
# export MOCK_VISION=false       # real faces when models + ort ready
export ENABLE_SMART_SCENE=true
export ZONE_CONFIG_DIR=./configs
export BIND_ADDR=0.0.0.0:8000
export CAM_IN_WEBRTC_PATH=demo   # or cam_in_h264 after transcoder

# 4. Run
./apps/edge/target/release/pksp serve
```

Frontend:

```bash
export NEXT_PUBLIC_API_URL=http://<host>:8000
export NEXT_PUBLIC_WS_URL=ws://<host>:8000/api/ws/live
export NEXT_PUBLIC_WEBRTC_BASE=http://<host>:8889
```

## Camera codec

Prefer **H.264** substream for browser WHEP. If only H.265 `stream1` is available, set `CAM_IN_RTSP` to that URL and let `pksp serve` run the supervised ffmpeg transcoder → `cam_in_h264`. Or set:

```bash
export CAM_IN_H264_RTSP=rtsp://user:pass@cam/stream2   # skips transcoder
export FORCE_TRANSCODE=true                            # force transcoder
```

## Smart scene zones

Edit `configs/zones.cam_in.json` polygons (normalized 0–1). Disable with `ENABLE_SMART_SCENE=false`.

## Backup / restore

```bash
# backup
cp data/pksp-rust.db data/pksp-rust.db.bak
tar czf enroll-backup.tgz data/enroll

# restore
cp data/pksp-rust.db.bak data/pksp-rust.db
tar xzf enroll-backup.tgz
```

## Rollback to Python (< 10 min)

1. Stop `pksp serve` (Ctrl-C).
2. `docker compose up -d mediamtx`
3. `cd apps/api && source .venv/bin/activate && uvicorn app.main:app --host 0.0.0.0 --port 8000`
4. Keep Next.js env pointing at `:8000`.

## systemd sketch

```ini
[Unit]
Description=PKSP Check-In Edge
After=network.target

[Service]
Type=simple
WorkingDirectory=/opt/pksp
EnvironmentFile=/opt/pksp/.env
ExecStart=/opt/pksp/apps/edge/target/release/pksp serve
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
```

## Re-enroll (embedding space change)

If Rust ONNX embeddings are not cosine-compatible with Python InsightFace (≥0.99), **re-enroll all employees** under Rust. Do not mix mock and real embeddings in one gallery for production punches.

## Known limits

- buffalo_l weights may be non-commercial
- Anti-spoof not certified
- CPU FPS limited; adaptive FPS optional (`VISION_ADAPTIVE=true`)
- Dual RTSP pull (vision + media) on high-res streams — prefer lower-res vision substream when available
