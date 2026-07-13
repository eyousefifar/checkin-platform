# Historical — superseded for active work by `GOAL-ADVISOR.md`.

# Goal: Implement PKSP Check-In Rust port (carefully)

## Goal kind

`code-change` — careful, milestone-gated rewrite of the **backend/edge stack** in Rust, guided by `rust-port-plans/` and behavioral parity with the working Python system. **Next.js (`apps/web`) stays.**

## Objective

Implement the full Rust edge runtime from **M0 through M6** as specified in:

| Priority | Doc |
|---|---|
| 1 | `rust-port-plans/14-implementation-roadmap.md` (milestone order + exits) |
| 2 | `rust-port-plans/00-README.md` (goals / non-goals / success criteria) |
| 3 | `rust-port-plans/02`–`03` (architecture + crate matrix) |
| 4 | `rust-port-plans/04`–`05` (data + frozen API/WS contracts) |
| 5 | `rust-port-plans/06`–`08` (core logic, models, worker/smart scene) |
| 6 | `rust-port-plans/09`–`11` (media, API state, frontend) |
| 7 | `rust-port-plans/12`–`13` (cutover + verification) |
| 8 | `rust-port-plans/15` (risks / decisions — do not reopen lightly) |
| 9 | Working Python under `apps/api/` + `plans/03`–`06` for **behavior** truth |
| 10 | `camera_issue_fix.md` for H.265 / WHEP constraints |

For **every milestone and every vertical slice** inside a milestone, run this loop until the milestone exit criteria are **honestly** green:

```
Thoughtful TDD
  → Implement minimum code to pass
  → Run tests (cargo test for touched crates)
  → Verify against milestone exit + contract parity
  → Refactor toward simpler, clearer Rust (same external behavior)
  → Optimize only with evidence (infer ms, FPS, lock contention)
  → Repeat until Perfect for that milestone
```

**Perfect** means: milestone exit checklist green, tests green, no dead dual implementations, no “we’ll wire it later” stubs left in the path you claim works, known limitations documented, and the Next.js app still works for that milestone’s scope.

### Careful means (non-negotiable discipline)

1. **One milestone at a time.** Do not start M{n+1} product work until M{n} Perfect gate.
2. **Contracts before cleverness.** HTTP/WS JSON matches `rust-port-plans/05` and `apps/web` — no drive-by field renames.
3. **Pure core first.** Algorithms in `pksp-core` with zero I/O; port Python tests before wiring RTSP/ONNX.
4. **Mock before models.** Default `MOCK_VISION=true` path must work end-to-end before real buffalo_l.
5. **Embedding compatibility is a hard gate.** Before reusing `data/pksp.db` embeddings with real ONNX, prove `cosine(python, rust) ≥ 0.99` on fixture images. If not, stop and fix alignment/normalize — do not “ship and re-enroll later” without documenting.
6. **Media last among parity paths, smart scene after parity.** M5 must not break M2–M4 behavior when smart scene is disabled.
7. **Do not rewrite MediaMTX.** Supervise it or use GStreamer WHEP; never reimplement the full media server.
8. **Do not rewrite Next.js.** Env-only integration until optional UI polish after M5.
9. **Do not delete Python** until M6 Perfect + rollback still documented. Keep dual-run possible.
10. **No FAISS, Postgres, Redis, cloud face APIs, PyO3-InsightFace.** Follow rejects in `rust-port-plans/03` and `15`.

---

## Universal engineering loop (mandatory each milestone)

### 1. Thoughtful TDD

Before product code for a unit of work:

1. Name the behavior in one sentence (from the relevant `rust-port-plans/*` section).
2. List failure modes and edge cases (empty gallery, cooldown, bad aspect, H.265 WHEP, etc.).
3. Write **failing** tests first:
   - Pure: embed, match, quality, track, vote, FSM, daily, zones, trajectory
   - DB: migrations, upsert cameras, blob roundtrip
   - API: status codes, auth 401, health shape, employee CRUD, daily JSON
   - WS: hello + detections shape (integration when router exists)
4. Prefer **fast tests without models** (mock engine / fixture vectors). Real ONNX tests: `#[ignore]` or feature `models` unless models are present.
5. No private biometrics in git; synthetic fixtures only.

### 2. Implement

- Smallest code that passes tests and satisfies the milestone slice.
- Crate boundaries from `rust-port-plans/02`:
  - `pksp-core` — pure
  - `pksp-db` — sqlx + migrations
  - `pksp-vision` — engine, gallery, worker, enroll helpers
  - `pksp-media` — MediaMTX/GStreamer façade
  - `pksp-api` — Axum routes, auth, hub, AppState
  - `pksp-cli` — `serve` / `migrate` / later `models`
- Config via env (**prefer identical names** to Python `.env` for drop-in).
- Typed `AppState` — **no process-global mutables** like Python’s hub/gallery/worker singletons.

### 3. Test

- `cargo test -p <crate>` then `cargo test` for default features.
- Capture command + exit code before claiming green.
- When frontend is in scope: point `NEXT_PUBLIC_API_URL` at Rust; smoke the pages manually.

### 4. Verify

Against **milestone exit criteria** in `rust-port-plans/14` + applicable checks in `rust-port-plans/13`:

- Automated first
- Manual/system when milestone includes them (health, WS, video, enroll walk)
- If reality forces a decision change: update `rust-port-plans/15` decision log — do not silently diverge

### 5. Refactor

With tests green:

- Delete dead code and premature abstractions
- Prefer clear module names over clever generics
- Collapse duplicate paths
- Keep public API/WS contracts stable
- Same external behavior after refactor (re-run tests)

### 6. Optimize

Only when measured or milestone-budgeted:

- Vision: FPS, det_size, latest-frame drop, inference semaphore
- API: `spawn_blocking` for ONNX/enroll; never block axum workers on heavy CPU
- Media: transcoder only when H.265; prefer camera H.264
- Prefer simplicity over micro-opts that obscure code

### 7. Repeat until Perfect

Milestone is done only when:

- [ ] All milestone tasks for that M complete
- [ ] Exit criteria checklist green (`14`)
- [ ] Relevant tests green
- [ ] Explicit refactor pass done (code simpler than first draft)
- [ ] Operator notes updated if next human needs new commands
- [ ] License / research-model disclaimer still true when real vision is live

Then commit a coherent milestone checkpoint (or atomic sub-commits per slice) and proceed.

---

## Target product shape (end state)

```
pksp serve   (Rust binary under apps/edge/)
├── control plane   Axum REST + WS + JWT + SQLite
├── vision plane    mock | ort buffalo_l, track/vote/FSM, optional smart scene
└── media plane     supervised MediaMTX (+ transcoder) → later optional GStreamer WHEP
         │
         ▼ same contracts
    apps/web (Next.js) — unchanged except env if needed
```

Workspace layout (required):

```
apps/edge/
  Cargo.toml
  crates/
    pksp-core/
    pksp-db/
    pksp-vision/
    pksp-media/
    pksp-api/
    pksp-cli/
  migrations/          # or under pksp-db
```

---

## Constraints (non-negotiable)

| Constraint | Value |
|---|---|
| Product goal | On-prem LAN check-in; CEO demo grade, not certified payroll |
| UI | **Keep** `apps/web` Next.js + BMW M design |
| API contracts | Frozen per `rust-port-plans/05` + `apps/web/src/lib/types.ts` |
| Match | Cosine on L2 embeddings — **not** FAISS |
| DB | SQLite; embeddings `float32` LE, dim 512, 2048-byte blobs |
| Models | buffalo_l ONNX via `ort` (+ mock); non-commercial banner remains |
| Gallery scale | ≪ 50 employees |
| Cameras | 1–2 RTSP; demo publisher allowed |
| Media | MediaMTX supervise first; **no** MediaMTX rewrite |
| Smart scene | Zones/trajectory/quality — **not** VLM on hot path |
| Python tree | Keep until M6 Perfect; rollback path required |
| Secrets | Env only; never log full RTSP URLs with passwords |

## Non-goals

- Rewrite Next.js / Electron / mobile
- Full pure-Rust WebRTC SFU / MediaMTX clone
- PyO3 wrapping InsightFace
- FAISS, Postgres, Redis, Kubernetes, cloud face APIs
- Production commercial model license purchase flow
- KYC-grade anti-spoof certification
- Feature creep beyond M0–M6 (YOLO person, dual AdaFace, multi-tenant SaaS, SSO)
- Deleting Python before rollback is documented and M6 is green
- “Big bang” rewrite of all crates in one untested PR

---

## Acceptance criteria (whole goal)

1. **Milestones M0→M6 complete** per `rust-port-plans/14` exit criteria.
2. **Workspace** exists at `apps/edge/` with crates as in architecture doc; `cargo build` and default `cargo test` green.
3. **API parity:** health, auth login JWT, employees CRUD + multipart enroll, attendance daily/events/CSV, cameras, `WS /api/ws/live` message types per `05` — Next.js works with ≤ env changes.
4. **Vision parity:** quality → match → IoU track → vote → attendance FSM + cooldown; mock mode always; real ONNX mode when models present.
5. **Embedding gate:** real engine cosine ≥ 0.99 vs Python on fixtures **or** explicit re-enroll procedure documented and executed — never silent wrong IDs.
6. **Media:** `pksp serve` owns/supervises browser-safe video path (H.264 WHEP); no dependence on hand-maintained `/tmp` transcoder scripts; H.265 handled by supervised transcoder or documented H.264 camera config.
7. **Data:** sqlx migrations; camera **upsert** from env (fixes Python seed-only-if-empty); existing `data/enroll/` layout supported.
8. **Smart scene (M5):** walk-by does not punch; active-zone legitimate stop does; feature flag to disable returns pure parity behavior.
9. **State hygiene:** single typed AppState; broadcast hub; no Python-style global mutables; latest-frame drop under load (no unbounded queues).
10. **Honesty:** README (or `apps/edge/README.md`) lists known limits (CPU FPS, model license, PAD not certified, media child binary if still used).
11. **Rollback:** documented path back to Python API + prior MediaMTX in &lt; 10 minutes.
12. **Quality bar:** each milestone ends with a refactor pass; no abandoned dual implementations left “for later.”

---

## Milestone checklist (execute in order)

### M0 — Workspace scaffold

**Plan refs:** `14` M0, `02`, `03`

- [ ] **TDD:** trivial unit or build-only gate — workspace compiles; optional `GET /health` later in M2
- [ ] Create `apps/edge/` Cargo workspace + crate stubs (`pksp-core` … `pksp-cli`)
- [ ] Shared `workspace.dependencies` per crate matrix (pin versions carefully; prefer current stable)
- [ ] `pksp-cli` binary builds (`--help` / placeholder `serve` ok)
- [ ] Short `apps/edge/README.md`: how to `cargo build`
- [ ] **Verify:** `cargo build` exit 0
- [ ] **Refactor:** clean workspace, no unused deps
- [ ] **Perfect gate:** M0 exits in `14`

### M1 — Pure core (`pksp-core`)

**Plan refs:** `06`, `13`, Python pure modules + tests

- [ ] **TDD first:** port cases from `test_embed`, `test_match`, `test_quality_track_vote`, `test_fsm`, `test_daily`
- [ ] Implement: embed pack/unpack/mean, cosine match + margin, quality gate, IoU track, vote, FSM, daily aggregate
- [ ] Stub or minimal zones/trajectory APIs (full smart behavior can wait for M5, but types ok)
- [ ] **Constraint:** `pksp-core` has **no** sqlx, ort, axum, gstreamer
- [ ] **Verify:** `cargo test -p pksp-core` exit 0
- [ ] **Refactor:** clear modules, no mega-file
- [ ] **Perfect gate:** M1 exits

### M2 — DB + API + mock vision + WS (frontend theater)

**Plan refs:** `04`, `05`, `10`, `11`, Python routers + mock worker

- [ ] **TDD first:** migrations on temp DB; camera upsert; JWT login; health JSON includes `cameras[].webrtc_path`; match via mock gallery
- [ ] sqlx migrations matching schema; **upsert cameras from env**
- [ ] Axum routes: health, auth, employees, attendance, cameras
- [ ] LiveHub via `tokio::sync::broadcast`; `WS /api/ws/live` hello/detections/attendance/metrics
- [ ] Mock FaceEngine + synthetic (or enroll-cycle) capture + process loop
- [ ] Gallery load; enroll multipart; attendance commit path
- [ ] Env names aligned with Python where possible; `MOCK_VISION=true` default
- [ ] **Verify:** Next.js login → dashboard WS HUD → employees → attendance (media video may still be external MediaMTX)
- [ ] **Verify:** `cargo test` default features green
- [ ] **Refactor:** AppState only; no globals
- [ ] **Perfect gate:** M2 exits

**Careful stop:** Do not start real ONNX until M2 Perfect. Theater path must be solid.

### M3 — Real ONNX engine + live capture

**Plan refs:** `07`, `12`, Python `engine.py` / capture

- [ ] **TDD first:** mock still green; ort load tests ignored without models; fixture cosine gate when models present
- [ ] Spike decision (≤2 days): custom ort SCRFD+ArcFace **or** `face_id` — record in `15` if changed
- [ ] Load buffalo_l ONNX (det + rec only; **no genderage**)
- [ ] Alignment + normalize; **hard gate:** cosine ≥ 0.99 vs Python on same image(s)
- [ ] Shared engine for enroll + live
- [ ] RTSP/frame capture backend (GStreamer appsink **or** retina+ffmpeg — pick per platform, document)
- [ ] `MOCK_VISION=false` path documented
- [ ] EP: CPU default; OpenVINO/CoreML optional features only if tested
- [ ] **Verify:** real enroll + recognition on demo stream or door cam sample
- [ ] **Verify:** if cosine gate fails → **do not** mark Perfect; fix or document mandatory re-enroll
- [ ] **Refactor:** single FaceEngine trait; no duplicated preprocess
- [ ] **Perfect gate:** M3 exits

**Careful stop:** Never mark M3 Perfect with “looks ok on HUD” without the cosine gate or an explicit re-enroll migration note.

### M4 — Media plane ownership

**Plan refs:** `09`, `camera_issue_fix.md`

- [ ] **TDD / checks:** media supervisor restarts child; health `webrtc_path` always browser-safe
- [ ] Supervise MediaMTX (child or documented external still ok only if supervised path is primary)
- [ ] H.265 → H.264 transcoder as owned, auto-restarting pipeline (GStreamer), **not** ad-hoc `/tmp` scripts
- [ ] Prefer native H.264 camera URL when configured (no unnecessary transcode)
- [ ] Frontend WHEP works after `pksp serve` (+ system plugins/binary) without manual steps
- [ ] HLS fallback still possible (ports 8888/8889 parity preferred)
- [ ] **Verify:** kill transcoder process → supervisor recovers; video returns
- [ ] **Refactor:** media behind clean trait (`MediaPlane` / handles)
- [ ] **Perfect gate:** M4 exits

### M5 — Smart scene

**Plan refs:** `08`, zones/trajectory in `06`

- [ ] **TDD first:** zone point-in-polygon; walk-by vs approach trajectories; commit eligibility rules
- [ ] Zone config per camera (JSON file first)
- [ ] Trajectory walk-by suppression; active-zone required for punch
- [ ] Optional pose/blur quality extensions
- [ ] Bidirectional motion hint when direction = bidirectional
- [ ] Feature flag `ENABLE_SMART_SCENE` (or equivalent): **off** = pure M2–M4 parity
- [ ] Additive HUD `state` values only (frontend may ignore)
- [ ] **Verify:** walk-by no check_in; door stop check_in; cooldown still holds
- [ ] **Refactor:** policy pure in core; worker only orchestrates
- [ ] **Perfect gate:** M5 exits

### M6 — Hardening, benches, cutover

**Plan refs:** `12`, `13`, `15`, `00` success criteria

- [ ] Benchmarks: match, infer, process loop notes recorded (markdown or bench output)
- [ ] Graceful shutdown (no SQLite corruption)
- [ ] Tracing levels usable for ops
- [ ] Deploy notes (how to run on LAN; systemd sketch optional)
- [ ] Backup/restore + rollback to Python drilled once
- [ ] Goal-level verification plan items all run
- [ ] Optional spike only: GStreamer WHEP without MediaMTX — **do not block Perfect** if unstable
- [ ] Update root or edge README: Rust primary path; Python rollback; known limits
- [ ] **Refactor:** final pass — delete dead code paths
- [ ] **Perfect gate:** full acceptance criteria 1–12

---

## Verification plan (goal-level)

Run and capture evidence (e.g. `artifacts/verify-rust/`, gitignored) as milestones complete:

1. **Build:** `cd apps/edge && cargo build` → exit 0  
2. **Unit/default tests:** `cargo test` → exit 0  
3. **Core:** `cargo test -p pksp-core` → exit 0  
4. **Health:** `GET /api/health` includes `status`, `vision_ready`, `gallery_size`, `cameras[].webrtc_path`  
5. **Auth:** login → token → authorized employees list  
6. **WS:** connect `/api/ws/live?token=…` → `hello` then `detections` under mock  
7. **Enroll smoke:** create employee + images → `embedding_ready`  
8. **Attendance smoke:** commit path → daily row + CSV headers  
9. **Frontend smoke:** `/`, `/employees`, `/attendance` against Rust API; license banner visible  
10. **Real vision (M3+):** recognition on demo/door within ~2s with vote when enrolled  
11. **Media (M4+):** WHEP video without manual transcoder script  
12. **Smart scene (M5+):** walk-by vs door-stop scenarios  
13. **Embedding gate (M3+):** cosine report or re-enroll log  
14. **Rollback drill (M6):** restore Python path once  
15. **Dress rehearsal:** CEO 5-minute script on Rust-only stack twice (or document honest blockers)

---

## Implementation approach (how to work carefully)

1. Work **one milestone at a time**; Perfect gate before next.  
2. Inside a milestone, slice by **testable vertical** (e.g. FSM before RTSP; health before enroll).  
3. Prefer **pure functions** at the core so TDD is cheap without cameras or models.  
4. Keep ONNX behind `FaceEngine` trait; mock is first-class, not an afterthought.  
5. Prefer **identical env var names** to Python for cutover simplicity.  
6. Prefer **identical JSON field names** to Python for frontend zero-change.  
7. Commit when a slice is green; message references milestone (`m1: match margin tests + impl`).  
8. Report progress after each Perfect gate (milestone id, tests, verify, refactor summary).  
9. If blocked 3+ times on the same issue (e.g. GStreamer plugins, ort EP):  
   - Document blocker in notes  
   - Continue with the fallback allowed by plans (MediaMTX child, mock vision, CPU EP)  
   - **Do not** mark the blocked milestone Perfect until the claimed path works or scope is explicitly reduced in `15`  
10. When unsure, re-read the specific `rust-port-plans` doc — do not invent architecture.

### Suggested first session (start here)

```
1. Read rust-port-plans/00, 02, 03, 14, 15 (skim 05, 06)
2. Execute M0 only until Perfect
3. Execute M1 only until Perfect (largest pure value, lowest risk)
4. Stop and report before M2
```

Do **not** jump to ONNX or media on day one.

---

## Task checklist (execution order)

- [ ] Read `GOAL-RUST.md` (this file) + `rust-port-plans/00`–`03`, `14`, `15`  
- [ ] Skim Python pure modules + tests that M1 will port  
- [ ] M0 loop → Perfect  
- [ ] M1 loop → Perfect  
- [ ] M2 loop → Perfect + frontend theater against Rust  
- [ ] M3 loop → Perfect + embedding gate evidence  
- [ ] M4 loop → Perfect + video without `/tmp` scripts  
- [ ] M5 loop → Perfect + walk-by proof  
- [ ] M6 loop → Perfect + goal-level verification 1–15  
- [ ] Mark goal completed only when acceptance criteria 1–12 hold  

---

## Risks (keep visible while coding)

| Risk | Mitigation (mandatory) |
|---|---|
| Embedding space mismatch → wrong IDs | Cosine ≥ 0.99 gate; no silent DB reuse |
| Scope creep (MediaMTX rewrite, YOLO, VLM) | Reject; see non-goals + `15` |
| GStreamer / MediaMTX install pain | MediaMTX child first; document plugins; feature flags |
| H.265 WHEP 400 | Owned transcoder or camera H.264; never ignore |
| TDD skipped under time pressure | **Forbidden** for core logic and contracts |
| Milestone bundling (M2+M3+M4 in one PR) | Forbidden; Perfect gates exist for a reason |
| Deleting Python too early | Keep until M6 + rollback drill |
| Dual vision workers double attendance | Only one vision commit path live during dual-run |
| Camera seed stale webrtc_path | Upsert from env (M2 requirement) |
| Premature smart scene breaks demo | Feature flag off = parity |

---

## Progress reporting

After each milestone Perfect gate, report:

- Milestone id + one-line outcome  
- Tests run + result  
- Verify checks done  
- Refactor summary (what got simpler)  
- Embedding/media notes if applicable  
- Remaining milestones  

On full completion: how to run CEO demo on Rust-only, residual known limitations, and where Python rollback lives.

---

## How to start this goal (agent / human)

```text
Implement the PKSP Rust port carefully per GOAL-RUST.md: milestones M0→M6 with
thoughtful TDD → implement → test → verify → refactor → optimize → Perfect gate
each milestone. Source of truth: rust-port-plans/ (especially 14, 05, 06, 07, 09).
Keep apps/web. No MediaMTX rewrite, no FAISS/Postgres/cloud faces, no PyO3 InsightFace.
Start with M0 then M1 only until Perfect. Prefer identical API contracts and env names
to the Python system. Hard gate on embedding cosine ≥ 0.99 before reusing gallery DB
with real ONNX.
```

Or paste this file’s **Objective** + **Acceptance criteria** + **Careful means** into a goal runner.

---

## Definition of done (one line)

A second operator can run **`pksp serve`** (plus documented media deps), keep using the existing Next.js app, enroll faces, see live HUD recognition, browser video without hand-rolled transcoder scripts, and export today’s attendance CSV — with green `cargo test`, frozen API contracts, optional smart walk-by rejection, documented model license limits, and a tested rollback to Python.
