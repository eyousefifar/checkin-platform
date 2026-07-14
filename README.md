# PKSP Check-In

On-prem employee check-in using RTSP cameras and face recognition.

**Runtime:** Rust (`apps/edge` → `pksp serve`). Next.js provides the operator UI.

> **Research / CEO demo MVP** — not a production payroll system.  
> Face models (`buffalo_l`) may be **non-commercial** unless separately licensed.

## What it is

1. **Live dashboard** — camera wall + WebSocket face HUD (aerospace ops shell)
2. **Employee enrollment** — model-guided webcam capture (or manual upload) → mean L2 embedding
3. **Daily attendance** — check-in / check-out FSM, event snapshots, CSV export

## Stack

| Layer | Choice |
|---|---|
| Edge API | Rust, Axum, Tokio, sqlx |
| Vision | Rust + ONNX Runtime (`buffalo_l`) |
| Match | Flat cosine |
| Web | Next.js 16 + Tailwind (`DESIGN.md` aerospace ops) |
| Video | MediaMTX (RTSP → WebRTC) |
| DB | SQLite |

**Not used:** FAISS, Postgres, cloud face APIs.

## Repo layout

```
apps/edge/         Rust edge runtime
apps/web/          Next.js admin UI
configs/           MediaMTX and zone configuration
data/              SQLite + enroll + event snapshots (gitignored)
scripts/           demo RTSP and model download helpers
plans/             archived design specs
DESIGN.md          aerospace ops design system
```

## Quickstart (Mac, LAN)

### 1. Build the edge runtime

```bash
./scripts/download_models.sh
cd apps/edge
cargo build --release
```

### 2. Full local demo (recommended)

One command starts MediaMTX (via `pksp`), a mock RTSP camera, and the Next UI.
Vision and the browser both use path **`cam_in`** so HUD boxes sit on the video you see.

```bash
chmod +x scripts/*.sh
./scripts/dev-stack.sh start webcam      # best: your face via Mac camera
# ./scripts/dev-stack.sh start chokepoint # offline face crops (detection only)
# ./scripts/dev-stack.sh start testsrc    # color bars, no faces
# SAMPLE=/path/to/lobby.mp4 ./scripts/dev-stack.sh start sample
./scripts/dev-stack.sh status
./scripts/dev-stack.sh stop
```

Open **http://localhost:3000** (login password defaults to `change-me` for loopback).
Enroll under **Configure** (guided capture), then show the same face to the mock RTSP source to test recognition.

Manual pieces (if you prefer separate terminals):

```bash
# Mock RTSP only (MediaMTX must already be up):
SOURCE=webcam ./scripts/demo_rtsp.sh
# SOURCE=chokepoint|testsrc|sample  SAMPLE=/path/to.mp4
```

For a real IP camera, set `CAM_IN_RTSP` and `CAM_IN_WEBRTC_PATH` to the **same** MediaMTX path in a private `.env`; never commit camera credentials.

### 3. Run the Rust API alone

```bash
export DATA_DIR=./data
export DATABASE_URL='sqlite:///./data/pksp-rust.db?mode=rwc'
export APP_TIMEZONE=Asia/Tehran
export CAM_IN_RTSP=rtsp://127.0.0.1:8554/cam_in
export CAM_IN_WEBRTC_PATH=cam_in
export ADMIN_PASSWORD=<set-a-strong-password>
export JWT_SECRET=<generate-at-least-32-bytes>
./apps/edge/target/release/pksp serve
```

Health: `curl http://localhost:8000/api/health`

The server fails closed if the buffalo_l models or an enabled camera RTSP URL are unavailable.

### 4. Run the web UI alone

```bash
cd apps/web
cp .env.local.example .env.local 2>/dev/null || true
npm install
npm run dev
```

Open **http://localhost:3000**.

## Tests

```bash
# Rust
cd apps/edge
cargo test --locked
cargo check --all-features --locked

# Web
cd ../web
npm run lint
npm run typecheck
npm test -- --run
npm run build
```

## Known limits

- **CPU FPS** may drop below target on dual cameras; prefer one camera for a CEO demo.
- **`buffalo_l` weights** are research/non-commercial unless licensed.
- **PAD / anti-spoof** is not KYC-certified.
- Trusted LAN only — no public-internet hardening.
- Event frame snapshots are best-effort after commit; no automatic retention/pruning yet.
- Snapshot JPEGs follow the same trusted-LAN visibility model as live video (no JWT).

## Design docs

- [GOAL-ADVISOR.md](./GOAL-ADVISOR.md) — active-stack completion and retirement record
- [apps/edge/README.md](./apps/edge/README.md) — edge runtime and operations
- [DESIGN.md](./DESIGN.md) — aerospace ops design system

## License note

InsightFace **code** is MIT. Pretrained **buffalo_l** packs are typically **non-commercial**. Production go-live needs a commercial license or a commercial-safe model swap.
