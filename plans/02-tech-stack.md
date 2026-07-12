# 02 — Tech Stack

## Final MVP stack

| Layer | Technology | Role |
|---|---|---|
| Web UI | **Next.js 15** (App Router) + TypeScript + Tailwind CSS | Admin dashboard, enrollment, attendance |
| Design | **BMW M `DESIGN.md`** ([getdesign.md/bmw-m](https://getdesign.md/bmw-m/design-md)) | Colors, type, spacing, sci-fi restraint |
| API | **FastAPI** + Uvicorn | REST + WebSockets |
| ORM | **SQLAlchemy 2.x** (or SQLModel) | SQLite access |
| DB | **SQLite** | Zero-ops on-prem demo |
| Vision | **InsightFace** `buffalo_l` + **ONNX Runtime** | Detect + align + 512-d embeddings |
| Match | **NumPy** cosine similarity | Gallery search for N&lt;50 |
| Anti-spoof (optional) | **MiniFASNet** (Silent-Face-Anti-Spoofing) | Gate on commit candidates only |
| Camera pull | **OpenCV** (`opencv-python-headless`) | RTSP frames for AI |
| Browser video | **MediaMTX** (bluenviron) | RTSP → WebRTC |
| Orchestration | **Docker Compose** (MediaMTX) + native Python/Node on Mac | LAN demo |
| Package (API) | `requirements.txt` or `uv` / pip-tools | Reproducible venv |
| Package (Web) | npm / pnpm | Standard Next toolchain |

## Version guidance (pin at implement time)

| Package | Guidance |
|---|---|
| Python | 3.11 or 3.12 |
| Node | 20 LTS or 22 LTS |
| insightface | latest stable 0.7.x line that installs cleanly on Mac |
| onnxruntime | CPU default; evaluate CoreML EP on Apple Silicon |
| fastapi | ≥ 0.115 |
| next | 15.x |
| MediaMTX | latest stable Docker image `bluenviron/mediamtx` |

Exact pins live in lockfiles when code is scaffolded.

## License matrix (critical)

| Component | License | Commercial use |
|---|---|---|
| InsightFace **code** | MIT | Yes |
| InsightFace **buffalo_l weights** | Non-commercial research unless separately licensed | **No** without license |
| ONNX Runtime | MIT | Yes |
| OpenCV | Apache-2.0 | Yes |
| FAISS (if ever added) | MIT | Yes |
| MiniFASNet / Silent-Face-Anti-Spoofing | Apache-2.0 | Generally yes (verify upstream README) |
| MediaMTX | MIT | Yes |
| Next.js / React | MIT | Yes |
| fal **AuraFace** weights | Apache-2.0 | Designed for commercial |
| OpenCV Zoo YuNet + SFace | Check model cards; Zoo Apache-friendly | Prefer for commercial-safe fallback |

### Demo policy

- MVP uses **buffalo_l** for CEO accuracy/speed.
- UI shows persistent disclaimer: *Face models are research/non-commercial unless licensed.*
- Production go-live requires one of:
  1. InsightFace commercial model license, or
  2. Swap recognizer to **AuraFace** (InsightFace-compatible ONNX), or
  3. Fallback stack **YuNet + SFace**.

## Why each major choice

### Next.js + Tailwind

- Fast path to a polished multi-page admin app.
- Easy WebSocket client + component structure for HUD.
- App Router layouts fit nav shell (Dashboard / Employees / Attendance).

### FastAPI

- Async WebSockets + sync OpenCV/ONNX in thread pool is a known pattern.
- Python is non-negotiable for InsightFace ecosystem.
- OpenAPI free for admin tooling.

### InsightFace buffalo_l (demo)

- One call path: detection, landmarks, recognition embedding.
- Internally uses efficient detector family (SCRFD-class) + ResNet50 ArcFace-style embedder.
- Widely used; fastest “whole pipeline works” path for a local demo.

### NumPy match (not FAISS)

- For ≤50 identities, flat cosine is trivial and correct.
- FAISS adds install pain (especially on Apple Silicon) with no accuracy gain.
- Upgrade trigger: gallery size or QPS that profiling shows as hot (unlikely).

### MediaMTX (not raw MJPEG from Python)

- Browsers cannot natively play RTSP.
- Re-encoding annotated video in Python wastes CPU and looks worse.
- MediaMTX is the maintained open media proxy (RTSP/WebRTC/HLS).

### SQLite (not Postgres)

- Single Mac demo, single writer, no DBA.
- Schema stays portable; switch dialect later if needed.

### MiniFASNet optional

- Apache-2.0 silent anti-spoof; good narrative for “not a printed photo.”
- **Not** security certification. Run only on candidates to save CPU.

## Rejected alternatives (and why)

| Alternative | Why rejected for this MVP |
|---|---|
| **FAISS** | Overkill for &lt;50 vectors; extra native dep |
| **pgvector / Postgres** | Unnecessary ops for demo scale |
| **Standalone SCRFD + separate ArcFace** | More glue; buffalo_l already bundles det+rec |
| **CVLFace AdaFace dual pass** | CPU budget killer; Phase 2 research only |
| **YOLO person track first** | Entrance 1–2 people; faces suffice |
| **MTCNN / old Haar** | Outdated accuracy/speed |
| **DeepFace mega-wrapper** | Heavier abstraction; less control over pipeline gates |
| **Cloud Face APIs** | Privacy, cost, offline demo fails |
| **Streaming annotated MP4 from OpenCV** | High CPU, high latency, poor UX vs WebRTC+Canvas |
| **Electron desktop** | Web on LAN is enough for CEO laptop/TV |
| **Kubernetes** | Absurd for 1 Mac demo |
| **Redis** | No multi-instance need; asyncio hub is enough |
| **Full RBAC / SSO** | Post-demo; single admin secret for LAN |

## Commercial upgrade paths

### Path A — License buffalo pack

Contact InsightFace commercial licensing for `buffalo_l` / related packs. Minimal code change.

### Path B — AuraFace

- Apache-2.0 model designed for commercial use.
- Works with InsightFace-style pipelines (ONNX embedding).
- May need accuracy re-thresholding vs buffalo_l.

### Path C — YuNet + SFace

- OpenCV Zoo; lighter, more defensible licensing story.
- Lower accuracy on hard angles/blur — accept for compliance-first orgs.

## Frontend design stack detail

- **Source of truth:** root `DESIGN.md` (BMW M analysis).
- **Fonts:** map BMW Type Next → available web substitute if licensed font unavailable (e.g. Inter / system geometric + uppercase tracking). Prefer closest open alternative and keep UPPERCASE display style.
- **Accent:** M tricolor stripe `#0066b1` → `#1c69d4` → `#e22718` used sparingly (header stripe, active indicators).
- **Surfaces:** pure black canvas `#000`, cards `#1a1a1a`, elevated `#262626`.
- **Telemetry:** mono font for scores/timestamps (Geist Mono or `ui-monospace`).

## Backend library detail

```
fastapi
uvicorn[standard]
sqlalchemy
pydantic / pydantic-settings
python-multipart
opencv-python-headless
numpy
insightface
onnxruntime          # or platform-specific EP package after testing (OpenVINOExecutionProvider supported)
Pillow
aiofiles
httpx                # optional health checks
```

Optional later: `onnxruntime` CoreML provider experiments on Mac.

## Environment variables (conceptual)

See also [08-infra-and-deploy](./08-infra-and-deploy.md).

| Variable | Purpose |
|---|---|
| `DATABASE_URL` | `sqlite+aiosqlite:///./data/pksp.db` or sync URL |
| `ADMIN_PASSWORD` | MVP gate |
| `CAM_IN_RTSP` | Source URL for entrance cam |
| `CAM_OUT_RTSP` | Source URL for exit cam |
| `MEDIAMTX_WEBRTC_URL` | Browser base for WebRTC |
| `INSIGHTFACE_MODEL` | default `buffalo_l` |
| `VISION_TARGET_FPS` | e.g. `5` |
| `MATCH_THRESHOLD` | e.g. `0.45` (cosine; calibrate) |
| `MATCH_MARGIN` | e.g. `0.08` top1−top2 |
| `COOLDOWN_SECONDS` | e.g. `90` |
| `ENABLE_ANTISPOOF` | `true/false` |

## Dependency install risks (Mac)

| Risk | Mitigation |
|---|---|
| insightface build needs tools | Prefer wheels; install Xcode CLT |
| onnxruntime CoreML regressions | Start with CPU EP; measure |
| OpenCV RTSP (FFmpeg backend) | Install via pip wheel; use `rtsp_transport=tcp` |
| Model auto-download first run | `scripts/download_models.sh` pre-warm for demo day |
