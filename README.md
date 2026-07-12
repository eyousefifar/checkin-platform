# PKSP Check-In

On-prem employee check-in using RTSP cameras and face recognition.

> **Research / CEO demo MVP** — not a production payroll system.  
> Face models (`buffalo_l`) may be **non-commercial** unless separately licensed.

## What it is

1. **Live dashboard** — camera tiles + WebSocket face HUD (BMW M sci-fi shell)
2. **Employee enrollment** — multi-photo gallery → mean L2 embedding
3. **Daily attendance** — check-in / check-out FSM + CSV export

## Stack

| Layer | Choice |
|---|---|
| Web | Next.js 15 + Tailwind (`DESIGN.md` BMW M) |
| API | FastAPI + Uvicorn |
| Vision | InsightFace `buffalo_l` + ONNX Runtime (or mock for theater) |
| Match | **NumPy cosine** (not FAISS) |
| Video | MediaMTX (RTSP → WebRTC) |
| DB | SQLite |

**Not used:** FAISS, Postgres, cloud face APIs.

## Repo layout

```
apps/api/          FastAPI + vision + attendance
apps/web/          Next.js admin UI
configs/           MediaMTX
data/              SQLite + enroll images (gitignored)
scripts/           demo_rtsp, download_models, seed_demo
plans/             design specs
DESIGN.md          BMW M tokens
```

## Quickstart (Mac, LAN)

### 1. MediaMTX

```bash
docker compose up -d mediamtx
```

### 2. Demo RTSP (optional, no real cameras)

```bash
chmod +x scripts/*.sh
# needs ffmpeg
./scripts/demo_rtsp.sh
# or SAMPLE=/path/to/lobby.mp4 ./scripts/demo_rtsp.sh
```

### 3. API (Python 3.11 recommended)

```bash
cd apps/api
python3.11 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
# optional real model: MOCK_VISION=false and ./scripts/download_models.sh
export MOCK_VISION=true   # default theater + mock enroll embeddings
export ADMIN_PASSWORD=change-me
uvicorn app.main:app --host 0.0.0.0 --port 8000 --reload
```

Health: `curl http://localhost:8000/api/health`

### 4. Web

```bash
cd apps/web
cp .env.local.example .env.local 2>/dev/null || true
npm install
npm run dev
```

Open **http://localhost:3000**

### 5. Admin

1. Visit `/login` — password from `ADMIN_PASSWORD` (default `change-me`)
2. **Employees** → add person → upload face images → embedding ready
3. **Dashboard** → live mock/real detections + event ticker
4. **Attendance** → daily table → Export CSV

## CEO 5-minute path

1. Cold open dashboard (mock HUD if `MOCK_VISION=true`)
2. Enroll a colleague with 5 door-angle photos (`MOCK_VISION=false` + real model for true faces)
3. Walk past camera / demo stream → name locks on HUD (~2s with vote)
4. Open Attendance → first_in → export CSV
5. Note multi-frame voting, cooldown, non-commercial banner

## Config (thresholds)

See `.env.example`. Important knobs:

| Variable | Default | Notes |
|---|---|---|
| `MATCH_THRESHOLD` | 0.45 | Cosine accept floor — calibrate on site |
| `MATCH_MARGIN` | 0.08 | top1−top2; prefer fewer false accepts |
| `COOLDOWN_SECONDS` | 90 | Same employee + camera double-punch guard |
| `MIN_DWELL_SECONDS` | 30 | Bidirectional IN→OUT dwell |
| `VISION_TARGET_FPS` | 5 | CPU budget |
| `DET_SIZE` | 640 | Lower if overloaded |
| `MOCK_VISION` | true | Synthetic engine for offline demo |

## Tests

```bash
cd apps/api && source .venv/bin/activate && pytest -q
cd apps/web && npx tsc --noEmit
```

Coverage includes: embedding pack/unpack, cosine match + margin, quality gate, IoU track, vote commit, attendance FSM + cooldown, daily aggregate, enroll API, WS shapes.

## Known limits

- **CPU FPS** may drop below target on dual cams; prefer 1 cam for CEO demo
- **`buffalo_l` weights** are research/non-commercial unless licensed — UI banner always on
- **PAD / anti-spoof** optional and **not** KYC-certified
- **Mock vision** mode proves UX without models; set `MOCK_VISION=false` after `scripts/download_models.sh`
- Profile / backlight / tiny faces will miss — quality gate rejects weak evidence
- Trusted LAN only — no public internet hardening

## Design docs

- [GOAL.md](./GOAL.md) — phase TDD loop
- [plans/](./plans/README.md) — architecture through verification
- [DESIGN.md](./DESIGN.md) — BMW M design system

## License note

InsightFace **code** is MIT. Pretrained **buffalo_l** packs are typically **non-commercial**. Production go-live needs a commercial license or a commercial-safe model swap (AuraFace / YuNet+SFace).
