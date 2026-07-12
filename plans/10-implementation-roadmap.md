# 10 — Implementation Roadmap

## Delivery philosophy

Ship a **CEO-impressive vertical slice** early (dark live HUD even with mock detections), then replace mocks with real vision, then harden attendance truth.

## Phase A — Skeleton & theater (Day 1)

**Outcome:** Beautiful shell runs; fake live data proves UX.

### Tasks

1. Repo layout `apps/api`, `apps/web`, `configs/`, `data/`, `scripts/`, `plans/`
2. Root `DESIGN.md` present (done via getdesign)
3. Next.js app with AppShell, routes, BMW M tokens in Tailwind
4. FastAPI health + mock WebSocket broadcaster (synthetic faces)
5. Docker Compose MediaMTX + demo path
6. Camera tiles with placeholder video or demo WebRTC
7. License banner + README quickstart

### Exit criteria

- [ ] CEO can open localhost and feel “this is a product”
- [ ] Event ticker animates with mock check-ins

## Phase B — Enrollment & gallery (Day 1–2)

**Outcome:** Employees and embeddings real.

### Tasks

1. SQLite models + create_all
2. Employee CRUD API + UI
3. Multi-image upload + storage
4. InsightFace load + enroll embedding compute
5. GalleryService in-memory matrix
6. Unit tests: pack/unpack, cosine match on fixtures

### Exit criteria

- [ ] Enroll 2 people with photos
- [ ] Gallery size reflects active employees with embeddings

## Phase C — Live vision (Day 2–3)

**Outcome:** Real boxes and names on live RTSP/demo video.

### Tasks

1. Camera config seed from env
2. RTSP capture loops + frame throttle
3. Quality gate + match + IoU track + voting
4. Real `detections` WS messages
5. Canvas HUD alignment
6. Camera online/offline status
7. Performance tuning on target Mac

### Exit criteria

- [ ] Known person labeled within ~2s on demo video or webcam/RTSP
- [ ] Unknown stays unknown at strict threshold

## Phase D — Attendance (Day 3)

**Outcome:** Daily sheet is true for walk-through.

### Tasks

1. Attendance FSM + cooldown
2. Persist events + local_date
3. Daily aggregate API + UI table
4. CSV export
5. Dashboard metrics from real events
6. Optional anti-spoof flag path

### Exit criteria

- [ ] Walk in → row first_in
- [ ] Walk out → last_out
- [ ] Rapid reappearance does not double-punch

## Phase E — Polish & demo pack (Day 3–4)

**Outcome:** Rehearsed, resilient, impressive.

### Tasks

1. Empty/error/reconnect states
2. Sci-fi HUD micro-details (brackets, mono clock)
3. `scripts/demo_rtsp.sh`, `download_models.sh`, `seed_demo.py`
4. Threshold calibration notes in README
5. 5-minute CEO script dry run
6. Backup `data/` instructions

### Exit criteria

- [ ] Full script passes twice without crash
- [ ] Second operator can start stack from README alone

## Work breakdown by area

| Area | Owner skill | Depends on |
|---|---|---|
| UI shell | Frontend | DESIGN.md |
| API/DB | Backend | Schema plan |
| Vision | CV/Python | Models download |
| MediaMTX | Infra | Camera URLs |
| Attendance | Backend | Vision commits |
| Docs | Anyone | All phases |

## CEO demo script (5 minutes)

1. **Cold open (30s)** — Full-screen dashboard; mention on-prem, no cloud, live cameras.
2. **Live detect (90s)** — Colleague walks into frame; name locks; score visible; ticker event.
3. **Enrollment (90s)** — Add a new person with 5 photos; show usable count; return to live; recognize them.
4. **Attendance truth (60s)** — Open daily table; show first_in; export CSV.
5. **Integrity (30s)** — Mention multi-frame voting, cooldown, non-commercial model banner, production license path.
6. **Close** — Q&A on rollout, privacy, hardware upgrade (GPU).

## Parallelization tips

- Frontend can build entire UI against mock WS while vision is in progress.
- MediaMTX + sample video unblocks UI without real cameras.
- Threshold calibration needs real faces — schedule 30 minutes with 2–3 staff.

## Definition of done (MVP)

- Plans implemented in code for Phases A–E
- README documents start/stop and camera env
- No secrets in git
- Demo script verified on target Mac
- Known limitations listed (CPU FPS, license, PAD)

## Post-MVP backlog (ordered)

1. Commercial-safe recognizer (AuraFace / license)
2. Postgres + backup automation
3. TLS + real auth
4. Manual review UI for gray-zone matches
5. GPU multi-camera
6. Optional person tracker for crowded lobby
7. HR export integrations
