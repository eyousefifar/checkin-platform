# PKSP Check-In

On-prem employee check-in using RTSP cameras and face recognition.

**Runtime:** Rust (`apps/edge` → `pksp serve`). Next.js provides the operator UI.

> **Research / CEO demo MVP** — not a production payroll system.  
> Face models (`buffalo_l`) may be **non-commercial** unless separately licensed.

## What it is

1. **Live dashboard** — camera tiles + WebSocket face HUD (BMW M sci-fi shell)
2. **Employee enrollment** — multi-photo gallery → mean L2 embedding
3. **Daily attendance** — check-in / check-out FSM + CSV export

## Stack

| Layer | Choice |
|---|---|
| Edge API | Rust, Axum, Tokio, sqlx |
| Vision | Rust + ONNX Runtime (`buffalo_l`) |
| Match | Flat cosine |
| Web | Next.js 16 + Tailwind (`DESIGN.md` BMW M) |
| Video | MediaMTX (RTSP → WebRTC) |
| DB | SQLite |

**Not used:** FAISS, Postgres, cloud face APIs.

## Repo layout

```
apps/edge/         Rust edge runtime
apps/web/          Next.js admin UI
configs/           MediaMTX and zone configuration
data/              SQLite + enroll images (gitignored)
scripts/           demo RTSP and model download helpers
plans/             archived design specs
DESIGN.md          BMW M tokens
```

## Quickstart (Mac, LAN)

### 1. Build the edge runtime

```bash
./scripts/download_models.sh
cd apps/edge
cargo build --release
```

### 2. Start MediaMTX and an optional demo stream

```bash
cd ../..
docker compose up -d mediamtx
chmod +x scripts/*.sh
./scripts/demo_rtsp.sh # needs ffmpeg; or SAMPLE=/path/to/lobby.mp4 ./scripts/demo_rtsp.sh
```

For an IP camera, set `CAM_IN_RTSP` and `CAM_IN_WEBRTC_PATH` in a private `.env`; never commit camera credentials.

### 3. Run the Rust API

```bash
export DATA_DIR=./data
export DATABASE_URL='sqlite:///./data/pksp-rust.db?mode=rwc'
export APP_TIMEZONE=Asia/Tehran
export CAM_IN_RTSP=rtsp://127.0.0.1:8554/cam_in
export ADMIN_PASSWORD=<set-a-strong-password>
export JWT_SECRET=<generate-at-least-32-bytes>
./apps/edge/target/release/pksp serve
```

Health: `curl http://localhost:8000/api/health`

The server fails closed if the buffalo_l models or an enabled camera RTSP URL are unavailable.

### 4. Run the web UI

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

## Design docs

- [GOAL-ADVISOR.md](./GOAL-ADVISOR.md) — active-stack completion and retirement record
- [apps/edge/README.md](./apps/edge/README.md) — edge runtime and operations
- [DESIGN.md](./DESIGN.md) — BMW M design system

## License note

InsightFace **code** is MIT. Pretrained **buffalo_l** packs are typically **non-commercial**. Production go-live needs a commercial license or a commercial-safe model swap.
