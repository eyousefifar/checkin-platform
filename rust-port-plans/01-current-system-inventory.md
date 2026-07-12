# 01 — Current System Inventory

**Purpose:** Complete map of the working Python/Next/MediaMTX system so the Rust port does not miss surface area.

## 1. Runtime processes (as of 2026-07-12)

| Process | Port(s) | Role | Start method |
|---|---|---|---|
| Next.js | `3000` | Admin UI, WHEP client, Canvas HUD | `apps/web` → `npm run dev` |
| FastAPI / Uvicorn | `8000` | REST, WS, enrollment, attendance, vision worker | `uvicorn app.main:app` |
| MediaMTX | `8554` RTSP, `1935` RTMP, `8888` HLS, `8889` WebRTC, `9997` API | Path proxy RTSP→WebRTC/HLS | Docker compose or standalone binary |
| GStreamer transcoder | (internal → RTMP 1935) | H.265→H.264 publish to `cam_in_h264` | Ad-hoc script (camera fix) |

Vision and API share **one Python process**. Media is external.

## 2. Repository layout

```
apps/api/app/           FastAPI + vision + attendance
apps/api/tests/         pytest suite
apps/web/src/           Next.js 15 App Router + Tailwind
configs/mediamtx.yml    Media paths
data/                   SQLite + enroll images (gitignored)
scripts/                demo_rtsp, download_models, seed_demo
plans/                  Original product design
camera_issue_fix.md   H.265 / WHEP incident report
DESIGN.md               BMW M tokens (frontend)
```

## 3. Backend module inventory

### 3.1 Entry & config

| File | Responsibility | Rust target |
|---|---|---|
| `app/main.py` | FastAPI app, lifespan: DB init, gallery load, start vision worker, hub.bind_loop | `pksp-cli` + `pksp-api` |
| `app/config.py` | pydantic-settings: auth, cameras, vision thresholds, ONNX providers, capture backend | `pksp-core` or `pksp-api` Settings |
| `app/auth.py` | JWT HS256 create/verify; Bearer dependency | `pksp-api` auth |

### 3.2 Database

| File | Responsibility | Rust target |
|---|---|---|
| `app/db/models.py` | SQLAlchemy models: employees, images, embeddings, cameras, attendance_events, app_meta | `pksp-db` |
| `app/db/session.py` | engine, SessionLocal, `init_db`, `seed_cameras` (only if empty) | `pksp-db` |

### 3.3 Routers

| File | Prefix / endpoints | Auth |
|---|---|---|
| `routers/health.py` | `GET /api/health` | Public |
| `routers/auth.py` | `POST /api/auth/login` | Public |
| `routers/employees.py` | CRUD + images upload + recompute embedding | Bearer |
| `routers/attendance.py` | daily, events, CSV | Bearer |
| `routers/cameras.py` | list, patch | Bearer |
| `routers/ws.py` | `WS /api/ws/live` | token query optional |

### 3.4 WebSocket hub

| File | Responsibility | Notes |
|---|---|---|
| `app/ws/hub.py` | `LiveHub`: connect/disconnect, broadcast, `broadcast_nowait` via `run_coroutine_threadsafe` | Thread→async bridge is fragile; Rust should use channels |

### 3.5 Vision

| File | Responsibility | Pure? |
|---|---|---|
| `services/vision/engine.py` | `FaceEngine` protocol; `MockFaceEngine`; `InsightFaceEngine` (buffalo_l, det+rec only) | I/O + model |
| `services/vision/embed.py` | L2 normalize, pack/unpack float32 LE, mean embedding | **Pure** |
| `services/vision/match.py` | Cosine top1 + margin → identity / UNKNOWN / AMBIGUOUS | **Pure** |
| `services/vision/quality.py` | det_score, min face px, aspect | **Pure** |
| `services/vision/track.py` | IoU multi-object tracker, vote history deque | **Pure** |
| `services/vision/vote.py` | Multi-frame majority vote commit | **Pure** |
| `services/vision/worker.py` | Latest-frame buffer, RTSP/OpenCV or FFmpeg VAAPI capture, process loop, adaptive FPS, infer → track → vote → commit | I/O heavy |

### 3.6 Gallery, enroll, attendance

| File | Responsibility |
|---|---|
| `services/gallery/service.py` | In-memory matrix (N,512); load from DB; version via `app_meta`; match |
| `services/enroll.py` | Decode upload, extract emb, write `data/enroll/{id}/`, mean, bump gallery |
| `services/attendance/fsm.py` | Direction + cooldown + min_dwell → commit/skip |
| `services/attendance/daily.py` | Aggregate first_in/last_out/status |
| `services/attendance/service.py` | Persist event, broadcast attendance WS |

### 3.7 Tests (must port or re-specify)

| Test file | Covers |
|---|---|
| `test_embed.py` | pack/unpack/mean |
| `test_match.py` | threshold, margin, UNKNOWN/AMBIGUOUS |
| `test_quality_track_vote.py` | quality gate, IoU, vote |
| `test_fsm.py` | in/out/bidirectional, cooldown, dwell |
| `test_daily.py` | aggregate statuses |
| `test_attendance_api.py` | HTTP attendance |
| `test_enroll_api.py` | upload path |
| `test_health_ws.py` | health + WS shapes |
| `test_worker_pipeline.py` | end-to-end pipeline with mock |
| `test_forbidden_stack.py` | no FAISS/Postgres/cloud deps |

## 4. Frontend inventory

### Pages

| Route | File | Backend deps |
|---|---|---|
| `/` | `app/page.tsx` | health (webrtc_path), WS live, metrics |
| `/login` | `app/login/page.tsx` | `POST /api/auth/login` |
| `/employees` | `app/employees/page.tsx` | list employees |
| `/employees/new` | `app/employees/new/page.tsx` | create |
| `/employees/[id]` | detail + image upload | get, patch, images, recompute |
| `/attendance` | daily table + CSV | daily + CSV export |

### Key libs/components

| Path | Role |
|---|---|
| `lib/api.ts` | `API_URL`, Bearer fetch, `wsUrl()` with token |
| `lib/types.ts` | FaceDet, DetectionsMsg, AttendanceMsg, MetricsMsg, Employee, DailyRow |
| `lib/whep.ts` | RTCPeerConnection + POST SDP to MediaMTX WHEP |
| `hooks/useLiveWs.ts` | WS client, fan-in messages |
| `components/CameraTile.tsx` | WHEP + HLS fallback + FaceHudCanvas overlay |
| `components/FaceHudCanvas.tsx` | normalized bboxes → canvas |
| `components/EventTicker.tsx` | attendance events |
| `DESIGN.md` tokens | BMW M styling |

**Important:** Video pixels do **not** come from the API. API only supplies HUD via WS. Video is MediaMTX WHEP at `NEXT_PUBLIC_WEBRTC_BASE` (default `http://localhost:8889`).

## 5. Data flows

### A. Live detection frame

```
RTSP camera
  → (vision) OpenCV / ffmpeg VAAPI → LatestFrameBuffer
  → throttle 5 FPS
  → FaceEngine.get → quality → gallery.match → assign_tracks → evaluate_vote
  → optional commit_identity → SQLite + WS attendance
  → WS detections (normalized bbox) every processed frame
  → Canvas HUD
```

### B. Browser video (parallel)

```
RTSP camera (often H.265)
  → MediaMTX path (cam_in) and/or GStreamer → H.264 → path cam_in_h264
  → WHEP POST /{path}/whep
  → <video> element
```

### C. Enrollment

```
Multipart upload → decode BGR → FaceEngine → quality
  → write enroll/{id}/{uuid}.ext
  → mean L2 of usable embeddings → employee_embeddings BLOB
  → gallery_version++ → reload matrix
```

### D. Attendance commit

```
VoteCommit → on_identity_commit(direction, cooldown, dwell)
  → INSERT attendance_events
  → WS type=attendance
```

## 6. Configuration surface (env)

From `app/config.py` / README:

| Variable | Default | Used for |
|---|---|---|
| `ADMIN_PASSWORD` | change-me | login |
| `JWT_SECRET` | dev secret | tokens |
| `JWT_TTL_HOURS` | 12 | tokens |
| `DATABASE_URL` | sqlite data/pksp.db | DB |
| `DATA_DIR` | data/ | enroll + db |
| `APP_TIMEZONE` | UTC | local_date |
| `CORS_ORIGINS` | http://localhost:3000 | CORS |
| `CAM_IN_RTSP` / `CAM_OUT_RTSP` | demo | capture |
| `CAM_IN_WEBRTC_PATH` | demo | frontend path |
| `IP_CAMERA_USER/PASS`, `CAM_IP`, `CAM_HIGH_QUALITY_PATH` | real cam | effective RTSP |
| `MATCH_THRESHOLD` | 0.45 | accept |
| `MATCH_MARGIN` | 0.08 | top1−top2 |
| `VISION_TARGET_FPS` | 5 | throttle |
| `DET_SIZE` | 640 | model |
| `MOCK_VISION` | true | synthetic |
| `ONNX_PROVIDERS` | CPUExecutionProvider | EP list |
| `CAPTURE_BACKEND` | auto | opencv vs vaapi |
| `COOLDOWN_SECONDS` | 90 | FSM |
| `MIN_DWELL_SECONDS` | 30 | bidirectional |
| `MIN_FACE_PX` / `MIN_DET_SCORE` | 60 / 0.5 | quality |
| `VOTE_WINDOW` / `VOTE_MIN_HITS` | 5 / 3 | voting |

## 7. Known defects & ops debt (fix in Rust)

| Issue | Evidence | Fix in Rust port |
|---|---|---|
| `seed_cameras` only runs if table empty | `session.py` early return | **Upsert** RTSP + webrtc_path from env on boot (or explicit migrate command) |
| H.265 not WebRTC-compatible | `camera_issue_fix.md` | Codec adapter as first-class media feature |
| Dual pull of camera | MediaMTX + OpenCV both RTSP | Single ingest fan-out where possible |
| Process-global hub/gallery/worker | module singletons | Typed `AppState` with `Arc` |
| Session per frame on commit | `worker._infer_frame` | Dedicated DB task / pool |
| Ad-hoc transcoder in `/tmp` | camera fix | Owned by `pksp-media` |
| Frontend default path mismatch | hardcoded cam_in vs demo | health-driven path already partially fixed; keep contract |

## 8. External binaries / models

| Artifact | Source | License note |
|---|---|---|
| MediaMTX | bluenviron Docker / binary | MIT |
| GStreamer + x264enc | system | LGPL stack |
| FFmpeg (VAAPI path) | system | LGPL/GPL depending build |
| buffalo_l ONNX | InsightFace download | **Non-commercial weights** unless licensed |
| OpenCV | pip | Apache-2.0 |

## 9. What is *not* in the current app

- Anti-spoof enabled (`ENABLE_ANTISPOOF` default false; MiniFASNet not integrated in worker)
- Zone maps / trajectory / person detector
- Multi-node / Redis / HA
- Alembic migrations (create_all only)
- OpenAPI-driven client generation

## 10. Acceptance criteria for this inventory

- [x] All process types listed
- [x] All `apps/api/app` modules mapped
- [x] Frontend routes and video path documented
- [x] Data flows A–D documented
- [x] Known defects captured for architecture docs

## 11. Source map summary (Python → Rust crate)

| Python | Rust crate |
|---|---|
| pure vision + attendance math | `pksp-core` |
| models + session + seed | `pksp-db` |
| engine + worker + gallery + enroll vision | `pksp-vision` |
| MediaMTX/GStreamer role | `pksp-media` |
| routers + auth + hub + main | `pksp-api` + `pksp-cli` |
| apps/web | **keep** (see `11-frontend-integration.md`) |
