# 08 — Infrastructure & Deploy

## Target environment

| Item | Value |
|---|---|
| Host | Mac (Apple Silicon or Intel) on company LAN |
| Cameras | 1–2 IP cameras with RTSP |
| Network | Cameras + Mac same LAN; no public ingress required |
| Users | Admin browser on same LAN |

## Runtime topology

```
[IP Cameras]
     | RTSP
     v
[MediaMTX :8554 RTSP | :8889 WebRTC | :8888 HLS]
     |                    \
     | RTSP read            \ WebRTC
     v                       v
[FastAPI Vision+API :8000]  [Browser Next :3000]
     |
     v
[SQLite data/pksp.db]
```

## Docker Compose (MVP)

Compose **at least** MediaMTX. API/web may run native on Mac for ONNX performance.

```yaml
# docker-compose.yml (conceptual)
services:
  mediamtx:
    image: bluenviron/mediamtx:latest
    ports:
      - "8554:8554"   # RTSP
      - "1935:1935"   # RTMP optional
      - "8888:8888"   # HLS
      - "8889:8889"   # WebRTC
      - "9997:9997"   # API optional
    volumes:
      - ./configs/mediamtx.yml:/mediamtx.yml
    network_mode: host   # often easiest for LAN cams on Linux; on Mac use port maps
```

**Mac note:** Docker Desktop networking differs; prefer published ports over `network_mode: host`. Cameras must be reachable from the container (host IP, not `localhost` of camera if mis-routed).

## MediaMTX config sketch

```yaml
# configs/mediamtx.yml (illustrative — align with current MediaMTX schema)
paths:
  cam_in:
    source: rtsp://user:pass@192.168.1.10:554/stream1
    sourceOnDemand: no
  cam_out:
    source: rtsp://user:pass@192.168.1.11:554/stream1
    sourceOnDemand: no
  # Demo without cameras:
  demo:
    source: publisher  # fed by ffmpeg script
```

Vision worker can read either:

- Direct camera RTSP, or
- `rtsp://127.0.0.1:8554/cam_in` via MediaMTX (preferred: single pull, reconnect centralization)

**Prefer MediaMTX as the only RTSP client to the camera** when camera connection limits are low.

## Demo without real cameras

`scripts/demo_rtsp.sh`:

```bash
# Loop a sample MP4 into MediaMTX path "demo"
ffmpeg -re -stream_loop -1 -i samples/lobby.mp4 \
  -c copy -f rtsp rtsp://127.0.0.1:8554/demo
```

Or MediaMTX + FFmpeg testsrc.

## Environment files

### `.env.example`

```bash
# API
ADMIN_PASSWORD=change-me
APP_TIMEZONE=UTC
DATABASE_URL=sqlite:///./data/pksp.db
CORS_ORIGINS=http://localhost:3000

# Cameras (also seed DB)
CAM_IN_RTSP=rtsp://127.0.0.1:8554/cam_in
CAM_OUT_RTSP=rtsp://127.0.0.1:8554/cam_out
CAM_IN_DIRECTION=in
CAM_OUT_DIRECTION=out

# Vision
INSIGHTFACE_MODEL=buffalo_l
VISION_TARGET_FPS=5
MATCH_THRESHOLD=0.45
MATCH_MARGIN=0.08
COOLDOWN_SECONDS=90
ENABLE_ANTISPOOF=false
DET_SIZE=640

# Web
NEXT_PUBLIC_API_URL=http://localhost:8000
NEXT_PUBLIC_WS_URL=ws://localhost:8000/api/ws/live
NEXT_PUBLIC_WEBRTC_BASE=http://localhost:8889
```

Never commit real camera passwords.

## Native process start (demo day)

```bash
# terminal 1
docker compose up mediamtx

# terminal 2
cd apps/api && source .venv/bin/activate
uvicorn app.main:app --host 0.0.0.0 --port 8000

# terminal 3
cd apps/web && npm run dev
```

Optional process manager: `honcho` / `overmind` Procfile.

## Apple Silicon notes

| Topic | Guidance |
|---|---|
| ONNX Runtime | Start with default CPU EP; measure FPS |
| CoreML EP | Optional experiment; can help or hurt depending on model splits |
| Docker for vision | Often slower / harder; keep vision native |
| Rosetta | Prefer arm64 Python wheels |
| Memory | buffalo_l ~hundreds of MB; fine on modern Mac |

## Linux on-prem server (later)

- Same compose + systemd units for api/web
- Nginx reverse proxy with basic auth/TLS if multi-user LAN
- NVIDIA: switch providers to CUDA EP when GPU appears

## Storage & backup

| Path | Backup? |
|---|---|
| `data/pksp.db` | Yes — attendance truth |
| `data/enroll/` | Yes — re-enrollment cost |
| Model cache | No — re-download |
| Logs | Optional |

Daily copy to encrypted USB / NAS for demo continuity.

## Networking checklist

- [ ] Mac can `ffprobe` camera RTSP
- [ ] MediaMTX paths show READY
- [ ] Browser on another laptop can open `http://<mac-ip>:3000`
- [ ] Firewall allows 3000, 8000, 8889 (and 8554 if needed)
- [ ] Cameras not exposed to WAN

## Observability (MVP)

- Structured logs: camera reconnects, commits, errors
- `/api/health` + dashboard metrics
- No Prometheus required for CEO demo

## Secrets

- Admin password in env
- Camera credentials in env / MediaMTX config with file perms `600`
- JWT secret separate from admin password

## Resource estimates

| Component | CPU | RAM |
|---|---|---|
| MediaMTX | low | ~100MB |
| Vision 2 cams @5fps buffalo_l CPU | high (1–4 cores) | 1–2GB+ |
| Next.js | low | ~200MB |
| SQLite | negligible | negligible |

If Mac fans scream: lower FPS, one camera, smaller det_size.
