# 01 — Architecture

## High-level view

```
┌──────────────────┐        RTSP         ┌────────────────┐
│ IP Camera IN     │────────────────────▶│                │
│ IP Camera OUT    │────────────────────▶│   MediaMTX     │── WebRTC ──▶ Browser (video)
└──────────────────┘                     │   (path proxy) │
                                         └───────┬────────┘
                                                 │ RTSP re-publish / path source
                                                 ▼
                                         ┌────────────────┐
                                         │ Vision Worker  │
                                         │ (in API proc)  │
                                         │ OpenCV + ONNX  │
                                         └───────┬────────┘
                    ┌────────────────────────────┼────────────────────────────┐
                    ▼                            ▼                            ▼
             ┌─────────────┐            ┌────────────────┐           ┌──────────────┐
             │ SQLite      │            │ WebSocket hub  │           │ REST API     │
             │ employees   │            │ detections     │           │ employees    │
             │ embeddings  │            │ attendance     │           │ attendance   │
             │ attendance  │            │ camera status  │           │ cameras      │
             └─────────────┘            └───────┬────────┘           └──────┬───────┘
                                                │                           │
                                                └───────────┬───────────────┘
                                                            ▼
                                                   ┌────────────────┐
                                                   │ Next.js Admin  │
                                                   │ Dashboard HUD  │
                                                   │ Enrollment     │
                                                   │ Daily results  │
                                                   └────────────────┘
```

## Component responsibilities

### MediaMTX

- Pull or accept RTSP from IP cameras (or FFmpeg test sources).
- Re-serve streams as **WebRTC** (primary) and optionally HLS fallback.
- Isolate cameras from multiple consumers (browser + vision worker).
- Config: `configs/mediamtx.yml`.

### Vision Worker (inside FastAPI process or sibling thread)

- Open RTSP with OpenCV (`rtsp_transport=tcp` preferred).
- Throttle to 3–8 FPS per camera under CPU budget.
- Run face pipeline (detect → quality → match → track → vote).
- Emit detection frames to WebSocket hub.
- Call attendance FSM on identity commits.
- Reload gallery when enrollment changes (in-process event or DB version stamp).

### API service (FastAPI)

- REST: employees, attendance, cameras, health, settings.
- WebSocket: `/ws/live` multiplexed event channel.
- Enrollment embedding extraction (reuse same model as live path).
- Session/auth for admin (LAN-grade MVP).

### Web (Next.js)

- Live dashboard: WebRTC player + Canvas HUD synced to WS events.
- Employee CRUD + multi-image upload.
- Attendance day view + CSV export.
- Apply BMW M design tokens from root `DESIGN.md`.

### SQLite

- Source of truth for employees, embeddings blobs, attendance events, camera config.
- File path: `data/pksp.db` (gitignored).

## Process model (MVP)

**Recommended for Apple Silicon demo:**

| Process | How it runs | Notes |
|---|---|---|
| MediaMTX | Docker Compose | Always containerized |
| API + Vision | Native Python 3.11+ venv on Mac | Avoid Docker ANE/CoreML friction |
| Web | `next dev` or `next start` on Mac | Proxy API via env `NEXT_PUBLIC_API_URL` |

**Optional full-compose** for non-Mac Linux hosts: MediaMTX + API containers; vision uses CPU ONNX only.

## Data flows

### A. Live detection frame

1. Vision reads frame at time `t`, size `(W, H)`.
2. Runs pipeline → list of faces with `bbox_norm` (0–1 coords), `track_id`, `employee_id?`, `name?`, `score?`, `status`.
3. Publishes:

```json
{
  "type": "detections",
  "camera_id": "cam_in",
  "ts": 1710000000.123,
  "frame_w": 1920,
  "frame_h": 1080,
  "faces": [
    {
      "track_id": 7,
      "bbox": [0.41, 0.22, 0.52, 0.48],
      "label": "Sara N.",
      "employee_id": "E1002",
      "score": 0.71,
      "quality_ok": true,
      "state": "tracking"
    }
  ]
}
```

4. Frontend draws boxes in Canvas using normalized bbox × video element size.

### B. Attendance commit

1. Vote succeeds → FSM evaluates IN/OUT/COOLDOWN.
2. Persist `attendance_events` row.
3. Broadcast:

```json
{
  "type": "attendance",
  "event_id": 42,
  "employee_id": "E1002",
  "name": "Sara N.",
  "kind": "check_in",
  "camera_id": "cam_in",
  "score": 0.74,
  "ts": 1710000001.0
}
```

### C. Enrollment

1. Admin uploads images via multipart REST.
2. API runs face extract offline (same model).
3. Stores images under `data/enroll/{employee_id}/`, mean embedding in DB.
4. Increments `gallery_version`; vision reloads gallery.

## Failure modes

| Failure | User-visible behavior | Recovery |
|---|---|---|
| Camera RTSP down | Tile shows OFFLINE; status WS event | Auto-reconnect OpenCV + MediaMTX path |
| MediaMTX down | No video; detections may still work if worker pulls cam direct | Restart compose service |
| Model load fail | API health = degraded; banner in UI | Fix ORT providers / model download |
| Unknown person | HUD “UNKNOWN”; optional unrecognized event | No false attendance if threshold strict |
| Gallery empty | Detections without names | Enroll employees |
| CPU overload | FPS drops; queue drops frames | Auto lower process rate |
| DB locked | Write retry; log error | Single writer; short transactions |

## Concurrency & threading

- **One** model session shared; serialize inference with a lock (CPU models are not free-threaded).
- Camera capture threads (or asyncio + thread pool) push latest frame into a 1-slot buffer (always process freshest).
- WebSocket broadcast via asyncio queue; do not block inference on slow clients.
- SQLAlchemy sessions per request/task; no shared session across threads.

## Trust boundaries

```
[Cameras / LAN] ── untrusted media ──▶ MediaMTX / Vision
[Admin browser on LAN] ── auth session ──▶ API
[API] owns biometrics; never expose raw embeddings publicly
```

MVP assumes **trusted LAN**. Do not port-forward to the internet without TLS + real auth + network review.

## Scaling notes (post-MVP)

| Growth | Change |
|---|---|
| More cameras | Dedicated vision workers per N cams; GPU |
| More employees | Still NumPy until ~10k; then FAISS IVF/HNSW |
| Multi-site | Postgres + edge workers + central report API |
| Production license | Swap buffalo_l → AuraFace or licensed pack |

## Directory layout (target)

```
pksp-checkin/
  DESIGN.md
  README.md
  docker-compose.yml
  configs/mediamtx.yml
  .env.example
  apps/
    api/
      pyproject.toml | requirements.txt
      app/
        main.py
        config.py
        db/
        routers/
        services/
          vision/
          attendance/
          gallery/
        schemas/
        ws/
    web/
      package.json
      src/app/...
  data/                 # gitignored runtime
  plans/                # this document set
  scripts/
    download_models.sh
    demo_rtsp.sh
    seed_demo.py
```
