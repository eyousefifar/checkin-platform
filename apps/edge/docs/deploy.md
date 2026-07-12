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

# 3. Env (owner-only secrets; never put camera passwords in RTSP user-info in git)
umask 077
export DATABASE_URL="sqlite:///./data/pksp-rust.db?mode=rwc"
export DATA_DIR=./data
chmod 700 "$DATA_DIR" 2>/dev/null || true
export MOCK_VISION=true          # theater
# export MOCK_VISION=false       # real faces when models + ort ready
export ENABLE_SMART_SCENE=true
export ZONE_CONFIG_DIR=./configs
export BIND_ADDR=127.0.0.1:8000  # non-loopback requires explicit ADMIN_PASSWORD + JWT_SECRET (≥32)
export CAM_IN_WEBRTC_PATH=demo   # or cam_in_h264 after transcoder
# ADMIN_PASSWORD and JWT_SECRET must come from a private EnvironmentFile / .env (0600)

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
# Prefer a credential-free H.264 path (set only in private .env):
# export CAM_IN_H264_RTSP=   # no user-info in tracked docs
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

Use a dedicated service user/group. Keep the environment file outside the
working tree and mode `0600`. `UMask=0077` ensures SQLite WAL/SHM and enrollment
files stay owner-only (do not rely on ad-hoc chmod helpers for every file).

```ini
[Unit]
Description=PKSP Check-In Edge
After=network.target

[Service]
Type=simple
User=pksp
Group=pksp
WorkingDirectory=/opt/pksp
EnvironmentFile=-/etc/pksp/pksp.env
UMask=0077
ExecStart=/opt/pksp/apps/edge/target/release/pksp serve
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
```

### One-time permission repair (operator-run, service stopped)

```bash
# stop the service first
sudo systemctl stop pksp
sudo chown -R pksp:pksp /opt/pksp/data
sudo chmod 700 /opt/pksp/data /opt/pksp/data/enroll
sudo find /opt/pksp/data -type f \( -name '*.db' -o -name '*.db-wal' -o -name '*.db-shm' -o -path '*/enroll/*' \) -exec chmod 600 {} \;
sudo chmod 600 /etc/pksp/pksp.env
sudo systemctl start pksp
# verify process umask 0077 and no group/other bits under DATA_DIR
```

Direct-run (non-systemd) operators should begin the shell session with `umask 077`
before creating `DATA_DIR` or the database. On non-Unix hosts, rely on OS ACLs /
disk encryption instead of inventing a portability layer.

## Re-enroll (embedding space change)

If Rust ONNX embeddings are not cosine-compatible with Python InsightFace (≥0.99), **re-enroll all employees** under Rust. Do not mix mock and real embeddings in one gallery for production punches.

## Real-model verification (operator-owned fixtures)

Do **not** commit face images or embeddings. Locally:

```bash
# Directory contains only private fixtures + manifest.json
# { "images": [ { "file": "a.jpg", "faces": 1, "expect_embedding": true }, ... ] }
export PKSP_VISION_FIXTURE_DIR=/path/to/private/fixtures
cd apps/edge
cargo test -p pksp-vision --features ort --locked real_model -- --ignored
```

Blank-frame smoke (models present, no fixtures):  
`cargo test -p pksp-vision --features ort --locked real_model_blank_frame`

## Known limits

- buffalo_l weights may be non-commercial
- Anti-spoof not certified
- CPU FPS limited; adaptive FPS optional (`VISION_ADAPTIVE=true`)
- Dual RTSP pull (vision + media) on high-res streams — prefer lower-res vision substream when available
