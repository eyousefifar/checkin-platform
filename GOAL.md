# Historical — superseded for active work by `GOAL-ADVISOR.md`.

# Goal: Implement PKSP Check-In MVP end-to-end (plans 00→11)

## Goal kind

`code-change` — greenfield implementation of the approved plan set under `plans/`.

## Objective

Implement the full PKSP Check-In platform from **Phase A through Phase E** as specified in `plans/00-overview.md` … `plans/11-verification.md` and `plans/10-implementation-roadmap.md`.

For **every phase and every vertical slice** inside a phase, run this loop until the phase exit criteria are honestly green:

```
Thoughtful TDD
  → Implement minimum code to pass
  → Run tests
  → Verify against plan acceptance + manual/system checks
  → Refactor toward simpler, clearer code (same behavior)
  → Optimize only with evidence (CPU FPS, latency, readability)
  → Repeat until Perfect for that phase
```

**Perfect** means: plan exit criteria met, tests green, no dead abstractions, no copy-paste residue, CEO demo path works for that phase’s scope, known limitations documented (not hidden).

## Source of truth (read in order)

| Priority | Doc |
|---|---|
| 1 | `plans/10-implementation-roadmap.md` (phase order) |
| 2 | `plans/01-architecture.md`, `02-tech-stack.md` |
| 3 | `plans/03`–`06` (vision, attendance, data, API) |
| 4 | `plans/07-frontend-ui.md` + root `DESIGN.md` (BMW M) |
| 5 | `plans/08-infra-and-deploy.md`, `09-security-privacy.md` |
| 6 | `plans/11-verification.md` (definition of done checks) |
| 7 | `plans/00-overview.md` (success criteria / non-goals) |

Do **not** invent FAISS, dual AdaFace, person-YOLO, Postgres, or cloud face APIs. Follow first-principles stack in the plans.

---

## Universal engineering loop (mandatory each phase)

### 1. Thoughtful TDD

Before writing product code for a unit of work:

1. Name the behavior in one sentence (from the plan).
2. List failure modes and edge cases from the plan.
3. Write **failing** tests first (unit and/or API):
   - Pure logic: matching, quality gate, FSM, cooldown, embedding pack/unpack
   - API: status codes, validation, daily aggregate
   - Prefer fast tests without InsightFace when possible (mock model / fixture vectors)
4. Only then implement.

**Thoughtful** means: test behavior and contracts, not private implementation trivia; avoid brittle snapshot spam; use fixtures under `tests/fixtures/` (no real private biometrics in git).

### 2. Implement

- Smallest code that passes the new tests and satisfies the plan slice.
- Match architecture: MediaMTX video ≠ vision path; WebSocket HUD; SQLite; NumPy cosine.
- Config via env (thresholds, RTSP, FPS) — no magic constants buried without a settings module.

### 3. Test

- Run the new tests + full relevant suite for that package.
- API: `pytest`
- Web: unit/component tests where valuable; typecheck (`tsc`)
- Do not claim green without capturing command + exit code.

### 4. Verify

Against **phase exit criteria** in roadmap + applicable rows in `plans/11-verification.md`:

- Automated gates first
- Then manual/system checks (health, demo RTSP, UI smoke) when the phase includes them
- Update `plans/` only if reality forces a documented decision (prefer code to match plans)

### 5. Refactor

With tests green:

- Delete dead code and premature abstractions
- Rename for clarity
- Collapse duplicate paths
- Keep public API/WS contracts stable
- **Same external behavior** after refactor (re-run tests)

### 6. Optimize

Only when measured or plan-budgeted:

- Vision: FPS, det_size, frame drop policy, lock contention
- API: avoid blocking event loop (thread pool for OpenCV/ONNX)
- UI: no unnecessary re-renders on WS flood
- Prefer simplicity over micro-opts that obscure code

### 7. Repeat until Perfect

Phase is done only when:

- [ ] All phase tasks complete
- [ ] Exit criteria checklist green
- [ ] Tests green
- [ ] Refactor pass done (code simpler than first draft)
- [ ] README / scripts updated for anything the next human needs
- [ ] License / research-model disclaimer present when vision is live

Then commit a coherent phase checkpoint (or atomic sub-commits per slice) and proceed to the next phase.

---

## Constraints (non-negotiable)

| Constraint | Value |
|---|---|
| Goal of product | CEO demo / local technical MVP |
| Compute | CPU / Apple Silicon first |
| Cameras | 1–2 RTSP (demo RTSP via FFmpeg/MediaMTX allowed) |
| Scale | &lt; 50 employees |
| Deploy | On-prem LAN |
| Match | NumPy cosine — **not** FAISS |
| DB | SQLite |
| Models | `buffalo_l` for demo + non-commercial banner |
| Design | BMW M `DESIGN.md` |

## Non-goals

- Production commercial model licensing purchase flow
- Kubernetes, Redis, pgvector, cloud face APIs
- Mobile app, multi-tenant SaaS, SSO
- KYC-grade liveness certification
- Pushing to remote / deploy to public internet
- Feature creep beyond plans A–E

---

## Acceptance criteria (whole goal)

1. **Phases A→E complete** per `plans/10-implementation-roadmap.md` exit criteria.
2. **Repo layout** matches architecture: `apps/api`, `apps/web`, `configs/`, `data/`, `scripts/`, `plans/`, root `DESIGN.md`, `.env.example`, `docker-compose.yml`.
3. **API:** FastAPI with health, auth (MVP), employees CRUD + image enroll, attendance daily/events/CSV, WebSocket `/api/ws/live` contracts per `plans/06`.
4. **Vision:** InsightFace pipeline with quality gate, cosine match + margin, IoU track, temporal vote, attendance FSM + cooldown per `plans/03`–`04`; CPU-friendly FPS throttle.
5. **Web:** Next.js dashboard (WebRTC + Canvas HUD), employees UI, attendance UI, BMW M styling, license banner per `plans/07`.
6. **Infra:** MediaMTX config + demo RTSP script; native API/web start path documented for Mac.
7. **Tests:** Meaningful pytest coverage for match, FSM, enrollment logic, attendance aggregate; suite green in CI-local command.
8. **Verification:** `plans/11-verification.md` system checklist largely passable with demo video (no office required for core path).
9. **Quality bar:** After each phase, a refactor pass leaves code simpler; no abandoned dual implementations.
10. **Honesty:** README lists known limits (CPU FPS, model license, PAD not certified).

---

## Phase checklist (execute in order)

### Phase A — Skeleton & theater

**Plan refs:** roadmap Phase A, arch, frontend shell, infra MediaMTX

- [ ] **TDD:** health endpoint returns structured JSON; WS mock emits typed `hello` / `detections` / `attendance` shapes
- [ ] Scaffold monorepo, compose MediaMTX, FastAPI app, Next.js shell + DESIGN tokens
- [ ] Mock WS broadcaster + dashboard HUD consuming mocks
- [ ] License banner + `.env.example` + README quickstart
- [ ] **Verify:** open UI, see sci-fi shell + live mock ticker
- [ ] **Refactor/Optimize:** thin app structure, no god files
- [ ] **Perfect gate:** Phase A exit criteria

### Phase B — Enrollment & gallery

**Plan refs:** data model, API employees, vision enroll path

- [ ] **TDD first:** embedding pack/unpack; mean L2 vector; reject no-face; gallery top1/margin
- [ ] SQLite models + employee CRUD + multipart images
- [ ] InsightFace (or test double in unit tests) for enroll compute
- [ ] GalleryService load/reload on version bump
- [ ] Employees UI list/create/upload/recompute
- [ ] **Verify:** enroll 2 fixture identities; gallery_size ≥ 2
- [ ] **Refactor/Optimize:** single enroll pipeline function, clear reject reasons
- [ ] **Perfect gate:** Phase B exit criteria + pytest green

### Phase C — Live vision

**Plan refs:** vision pipeline, API WS detections, frontend CameraTile

- [ ] **TDD first:** quality gate; tracker IoU assign; vote commit rule (pure functions)
- [ ] RTSP/latest-frame capture + throttle + inference lock
- [ ] Real detections over WS; camera online status
- [ ] Canvas HUD alignment with normalized bboxes
- [ ] Demo RTSP script path works without real IP cams
- [ ] **Verify:** labeled recognition on demo/sample stream within ~2s when enrolled
- [ ] **Refactor/Optimize:** drop dead branches; tune FPS/det_size with measurement notes
- [ ] **Perfect gate:** Phase C exit criteria

### Phase D — Attendance

**Plan refs:** attendance logic, daily API/UI, CSV

- [ ] **TDD first:** FSM in/out/bidirectional; cooldown; daily aggregate statuses
- [ ] Persist events + `local_date`; broadcast `attendance` WS
- [ ] Daily table UI + filters + CSV export
- [ ] Dashboard metrics from real events
- [ ] **Verify:** walk-in / walk-out / no double-punch under cooldown
- [ ] **Refactor/Optimize:** one FSM module, no duplicated date logic
- [ ] **Perfect gate:** Phase D exit criteria

### Phase E — Polish & demo pack

**Plan refs:** roadmap E, verification, security disclaimers

- [ ] Empty/error/reconnect states; offline camera tile
- [ ] Sci-fi HUD micro-details (tasteful, per UI plan)
- [ ] `scripts/download_models.sh`, `demo_rtsp.sh`, optional `seed_demo.py`
- [ ] README operator guide + threshold calibration notes
- [ ] Run dress-rehearsal checklist from `plans/11` + CEO script from `plans/10`
- [ ] **Refactor/Optimize:** final pass across api/web for simplicity
- [ ] **Perfect gate:** full acceptance criteria 1–10

---

## Verification plan (goal-level)

Run and capture evidence under a scratch dir or `artifacts/verify/` (gitignored) as phases complete:

1. **Unit/API:** `cd apps/api && pytest -q` → exit 0  
2. **Types web:** `cd apps/web && npx tsc --noEmit` → exit 0  
3. **Health:** API `/api/health` shows expected readiness fields  
4. **Demo path:** MediaMTX up + `scripts/demo_rtsp.sh` (or documented equivalent)  
5. **Enroll smoke:** create employee + images → embedding ready  
6. **Live smoke:** detections WS messages with bbox schema  
7. **Attendance smoke:** commit path produces daily row + CSV headers  
8. **UI smoke:** `/`, `/employees`, `/attendance` render; banner visible  
9. **Dress rehearsal:** CEO 5-minute script twice (or document blockers honestly)

## Implementation approach

1. Work **one phase at a time**; do not start Phase N+1 until Phase N Perfect gate.  
2. Inside a phase, slice by **testable vertical** (e.g. FSM module before RTSP wiring).  
3. Prefer **pure functions** at the core (match, vote, FSM) so TDD is cheap without cameras.  
4. Keep InsightFace behind a narrow `FaceEngine` protocol for test doubles.  
5. Native Python venv on Mac for vision; Docker for MediaMTX.  
6. Commit when a slice is green; message references phase (`phase-B: gallery match tests + service`).  
7. Use `update_goal` progress messages after each Perfect gate.  
8. If blocked 3+ times on the same issue (e.g. model install), document blocker and continue with interfaces + mocks only where plan allows demo theater — but do not mark vision Perfect until real or honest substitute works.

## Task checklist (execution order)

- [ ] Read all `plans/00`–`11` + `DESIGN.md` once; note deviations only if forced  
- [ ] Phase A loop → Perfect  
- [ ] Phase B loop → Perfect  
- [ ] Phase C loop → Perfect  
- [ ] Phase D loop → Perfect  
- [ ] Phase E loop → Perfect  
- [ ] Goal-level verification plan items 1–9  
- [ ] Final README + known limitations  
- [ ] Mark goal completed only when acceptance criteria 1–10 hold  

## Risks

| Risk | Mitigation |
|---|---|
| buffalo_l / onnxruntime install pain on Mac | Document; CPU EP first; test doubles keep TDD moving |
| No real RTSP cameras | Demo publisher + sample video mandatory |
| CPU too slow for 2 cams | Priority cam, lower FPS/det_size; still pass 1-cam demo |
| Scope creep (AdaFace, FAISS…) | Reject; plans already decided |
| TDD skipped under time pressure | Forbidden by this goal — tests before or with red→green, never “later” for core logic |
| Refactor skipped | Perfect gate requires explicit refactor pass |

## Progress reporting

After each phase Perfect gate, report:

- Phase id + one-line outcome  
- Tests run + result  
- Verify checks done  
- Refactor summary (what got simpler)  
- Remaining phases  

On full completion: summarize how CEO demo script is run, and list residual known limitations.

---

## How to start this goal in Grok

```text
/goal Implement PKSP Check-In MVP per GOAL.md: phases A→E with thoughtful TDD → implement → test → verify → refactor → optimize → repeat until Perfect each phase. Source of truth: plans/ and DESIGN.md. Stack: Next.js, FastAPI, InsightFace buffalo_l, MediaMTX, SQLite, NumPy cosine. No FAISS/Postgres/cloud face APIs.
```

Or paste this file’s **Objective** + **Acceptance criteria** into `/goal`.

## Definition of done (one line)

A second operator can start MediaMTX + API + web from README, enroll faces, see live HUD recognition on a demo stream, and export today’s attendance CSV — with green tests and a BMW M sci-fi dashboard — without cloud services.
