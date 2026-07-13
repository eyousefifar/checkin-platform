# Goal: Finish all advisor-plans and fully retire Python

## Goal kind

`code-change` — execute every plan under `advisor-plans/` against the **active**
stack (`apps/edge` Rust + `apps/web` Next.js + MediaMTX), then **remove the
legacy Python backend** so the product is Rust-only end-to-end.

## Objective

Drive the repository from “Rust is primary, Python is rollback” to:

1. **All 19 advisor plans** status `DONE` in `advisor-plans/README.md`, with each
   plan’s done criteria, tests, and STOP conditions honestly satisfied.
2. **Python fully removed** as a runtime, test surface, and documented path:
   no `apps/api/`, no `pytest` API suite, no Python venv instructions, no
   dual-run “start uvicorn” quickstart — only `pksp serve` + Next.js + MediaMTX.
3. **CEO-demo grade** on the Rust stack: real SCRFD/ArcFace when models present,
   truthful live HUD and metrics, browser-safe video, secure defaults, enrollment
   integrity, and operator UX from plans 012–018.

For **every plan** (and for the final Python-retirement wave), run this loop
until the plan is **honestly** Perfect:

```
Read plan + drift check
  → Thoughtful TDD (failing tests first where the plan requires them)
  → Implement minimum code to pass
  → Run plan commands + active-stack gates
  → Verify done criteria (not just “code exists”)
  → Refactor toward simpler code (same external behavior)
  → Update advisor-plans/README.md status row
  → Commit on the plan’s branch / message convention
  → Repeat until Perfect for that plan
```

**Perfect** means: plan done criteria green, required commands exit 0, no
STOP-condition improvisation, no silent scope expansion, status row updated,
and known limitations documented rather than hidden.

---

## Source of truth (read in this order)

| Priority | Doc | Role |
|---|---|---|
| 1 | `advisor-plans/README.md` | Status board, waves, global boundaries |
| 2 | The selected `advisor-plans/NNN-*.md` | Steps, drift check, STOP, done criteria |
| 3 | This file (`GOAL-ADVISOR.md`) | Cross-plan order, Python retirement, whole-goal DoD |
| 4 | `apps/edge/README.md` + `apps/edge/docs/deploy.md` | How the active runtime is run |
| 5 | `DESIGN.md` | BMW M UI language (do not invent a new design system) |
| 6 | `rust-port-plans/05` (contracts) + `apps/web/src/lib/types.ts` | Frozen HTTP/WS field names |
| 7 | Historical only | `apps/api/**`, `GOAL.md`, `GOAL-RUST.md`, `plans/*` — **behavior reference**, not the active runtime |

Do **not** treat Python as the oracle during advisor plans (plans forbid modifying
or invoking `apps/api/**`). During Wave 8 (Python retirement), Python code is
**deleted**, not extended.

---

## Non-negotiable discipline

1. **One plan at a time** (or only the parallel pairs the README allows:
   after 001 Perfect, 002∥003; later 012∥013∥015; 018 may run with 016/017 once
   its own deps are green). Never open product work for plan N+1 while N is
   still `TODO` if N is a dependency.
2. **Drift check first** — every plan starts with its `git diff --stat …` check.
   If in-scope files moved, re-map the plan’s current-state excerpts before coding.
3. **Honor STOP conditions** — stop and report; do not invent architecture.
4. **Active stack only during plans 001–019** — do not edit `apps/api/**`, do not
   run Python pytest as a gate, do not “fix Python first then port.”
5. **Contracts before cleverness** — keep HTTP/WS JSON and env names stable for
   `apps/web` unless a plan explicitly changes them.
6. **No forbidden stack** — no FAISS, Postgres, Redis, Redux, React Query,
   component kit, Tailwind 4 rewrite, MediaMTX clone, cloud face APIs, or
   generic plugin layer (see README global boundaries).
7. **Never commit secrets or biometrics** — no credential values, no employee
   face images, no embeddings, no private RTSP URLs in git.
8. **Python dies only in Wave 8** — after 001–019 are green (019 optional only if
   operator defers upgrade; document if deferred). Do not delete `apps/api`
   mid-stream as a shortcut.
9. **No push / no PR** unless the operator explicitly asks.
10. **Status board is truth** — update `advisor-plans/README.md` when a plan is
    `IN PROGRESS`, `DONE`, `BLOCKED`, or `REJECTED`.

---

## Target end state

```
pksp serve                    # apps/edge Rust binary — sole control + vision plane
├── Axum REST + JWT + WS
├── SQLite (sqlx migrations)
├── SCRFD + ArcFace (ort) | mock
├── track / vote / FSM / cooldown
├── MediaMTX supervisor (+ H.264 path)
└── enrollment cumulative & recoverable
         │
         ▼ same contracts
apps/web (Next.js)            # BMW M admin UI — camera wall, enroll, attendance
configs/ + apps/edge/configs  # MediaMTX pins
data/                         # SQLite + enroll (restricted perms)
scripts/                      # shell only: demo RTSP, download models, benches
                              # NO seed_demo.py / NO Python entrypoints
```

**Removed:**

- `apps/api/**` (FastAPI, InsightFace Python path, pytest suite)
- Python venv / `requirements.txt` / uvicorn quickstart from root docs
- Any dual-run “Python rollback in 10 minutes” as a **supported** product path
  (historical notes may remain in `rust-port-plans/` as archive)

**Kept:**

- `apps/edge/**` Rust workspace
- `apps/web/**` Next.js
- MediaMTX (pinned; upgraded only via plan 019)
- SQLite, flat cosine match, BMW M design

---

## Constraints (product)

| Constraint | Value |
|---|---|
| Product | On-prem LAN check-in; CEO demo grade, not certified payroll |
| Runtime | **Rust only** after Wave 8 |
| UI | Next.js 16 + React 19 + Tailwind 3 + BMW M |
| Match | Cosine on L2 512-d embeddings — not FAISS |
| DB | SQLite; embeddings float32 LE, 2048-byte blobs |
| Models | buffalo_l ONNX via `ort` (+ mock); non-commercial banner remains |
| Gallery | ≪ 50 employees |
| Cameras | 1–2 RTSP; demo publisher allowed |
| Media | MediaMTX supervise — no full media-server rewrite |
| Secrets | Env only; JWT not in logs; biometric files restricted |

## Non-goals

- Rewrite Next.js / Electron / mobile
- Pure-Rust WebRTC SFU / MediaMTX reimplementation
- FAISS, Postgres, Redis, Kubernetes, cloud face APIs
- Playwright suite as a prerequisite (Vitest + manual media dress rehearsal)
- GPU EP expansion before CPU correctness + measured need
- Unpinned “always latest” MediaMTX
- KYC anti-spoof certification / commercial model license purchase flow
- Feature creep beyond the 19 plans + Python retirement
- Keeping Python “just in case” after Wave 8 Perfect

---

## Universal engineering loop (every plan)

### 1. Read + drift

1. Read the full plan file end-to-end (not just the title).
2. Run the plan’s drift check; reconcile any moved symbols.
3. Set status to `IN PROGRESS` in `advisor-plans/README.md` if starting work.

### 2. Thoughtful TDD

When the plan calls for tests (most do):

1. Name the behavior in one sentence from the plan.
2. List failure modes / STOP edges.
3. Write failing tests first (router harness, pure SCRFD decode, enroll bounds,
   WS freshness, timezone CSV, Vitest UI, etc.).
4. Prefer fast tests without models/cameras; real ONNX only when the plan
   requires a local model gate and models are present.
5. No private biometrics in git — synthetic fixtures only.

### 3. Implement

- Smallest change that satisfies **this** plan’s scope list.
- Stay inside **In scope**; do not pre-build later plans “while you’re there.”
- Preserve crate boundaries: pure logic in `pksp-core`, I/O in `pksp-db` /
  `pksp-vision` / `pksp-media` / `pksp-api`.

### 4. Test & verify

- Run every command in the plan’s “Commands you will need” table.
- Capture exit codes; do not claim green without them.
- Meet **Done when** / acceptance bullets in the plan body.

### 5. Refactor

With tests green: delete dead paths introduced by the fix, clarify names, no
behavior change. Re-run the plan’s critical tests.

### 6. Close the plan

- [ ] Status row → `DONE` in `advisor-plans/README.md`
- [ ] Branch/commit per plan git workflow (unless operator prefers another flow)
- [ ] Report: plan id, tests, verify notes, any deferred STOP items

---

## Execution waves (mandatory order)

Execute in this order. Do not mark a wave Perfect until every plan in it is
`DONE` (or explicitly `REJECTED` / deferred with operator sign-off for 019 only).

### Wave 0 — Orientation (no product commits required)

- [ ] Read `advisor-plans/README.md` fully (boundaries, waves, rejects).
- [ ] Skim all plan titles + status table; confirm every plan is still `TODO`
      or note any already-`DONE` work.
- [ ] Confirm active stack builds:
  - `cd apps/edge && cargo test --locked`
  - `cd apps/web && npm run typecheck` (or establish baseline failures plan 001 will fix)
- [ ] Confirm **no** work will touch `apps/api/**` until Wave 8.

### Wave 1 — Foundation

| Plan | Title | Notes |
|---|---|---|
| **001** | Active-stack verification baseline | **Serial first.** Router + Vitest harness + CI scripts. All later plans depend on this. |
| **002** | Rust fmt + Clippy gates | Parallel with 003 after 001. Mechanical only. |
| **003** | Security containment | Parallel with 002 after 001. Credentials, JWT logging, file modes, bind defaults. |

**Wave 1 Perfect gate:**

- [ ] 001–003 status `DONE`
- [ ] CI-local commands from 001 green; fmt/clippy green; security audits from 003 green
- [ ] No secrets committed; RTSP user-info audit clean

### Wave 2 — Recognition correctness (P0 trust)

| Plan | Title | Notes |
|---|---|---|
| **004** | Real SCRFD + ArcFace pipeline | Largest risk. Real vision fail-closed. Pure decode tests first. |
| **005** | Frame-once + isolate blocking inference | After 004. No frozen-capture live refresh. |
| **006** | Enrollment cumulative / bounded / recoverable | After 004+005. Never delete before successful analysis. |

**Wave 2 Perfect gate:**

- [ ] 004–006 status `DONE`
- [ ] Mock path still green; real models path fail-closed when broken
- [ ] Enroll multi-photo is cumulative; recompute is safe
- [ ] `cargo test --locked` green

### Wave 3 — Live-system truth

| Plan | Title | Notes |
|---|---|---|
| **007** | Browser-safe MediaMTX path | After 001+003. One verified publisher→WHEP path. |
| **008** | Live WebSocket + camera state | After 001+005. Explicit fresh/unknown/offline. |
| **010** | Rust employee + CSV contracts | After 001+003. API/export contract complete. |
| **011** | `APP_TIMEZONE` dates/times/CSV | After 010. Single validated timezone path. |
| **009** | Truthful metrics + scene outcomes | After 005, 008, 011. No fake metrics. |

**Wave 3 Perfect gate:**

- [ ] 007–011 status `DONE` (009 last among these)
- [ ] Media smoke path green; WS state truthful; CSV/timezone correct
- [ ] Metrics and scene labels match real worker outcomes

### Wave 4 — Operator UX + cheap performance

| Plan | Title | Notes |
|---|---|---|
| **012** | Enrollment UX recoverable | After 006. |
| **013** | Browser video state truthful | After 007, 008, 011. |
| **015** | Low-cost frontend hotspots | After 001; can parallel 012/013. |

**Wave 4 Perfect gate:**

- [ ] 012, 013, 015 status `DONE`
- [ ] Web tests/lint/typecheck/build gates green for touched surfaces

### Wave 5 — Product direction

| Plan | Title | Notes |
|---|---|---|
| **016** | Dynamic one/two camera wall | After 007, 008, 013. |
| **017** | Attendance explainability | After 009, 011, 016. Reuse events API. |
| **018** | Employee edit + reversible deactivate | After 010, 012; may parallel 016/017. |

**Wave 5 Perfect gate:**

- [ ] 016–018 status `DONE`
- [ ] Dashboard wall matches health; attendance expands to raw events; edit/deactivate UI works

### Wave 6 — Shared UI accessibility

| Plan | Title | Notes |
|---|---|---|
| **014** | Responsive + accessible operational UI | **Last shared markup pass** after 012–013 and 015–018. |

**Wave 6 Perfect gate:**

- [ ] 014 status `DONE`
- [ ] Keyboard/focus/responsive checks in plan green

### Wave 7 — Optional MediaMTX upgrade

| Plan | Title | Notes |
|---|---|---|
| **019** | MediaMTX upgrade behind regression gate | Only after 007+013 green. High risk. Pin both download script and docker-compose together. |

**Wave 7 Perfect gate (or explicit deferral):**

- [ ] 019 `DONE`, **or** operator-approved `REJECTED — deferred` with pin remaining at proven version and reason recorded in README status

### Wave 8 — Full Python removal (required for this goal)

This wave is **not** one of the numbered advisor plans (they intentionally
excluded Python retirement). It is **required** to complete **this** goal.

#### 8.1 Pre-delete proof (do not delete until all hold)

- [ ] Waves 1–6 Perfect (and 7 resolved)
- [ ] Root/active docs already describe Rust as primary (fix any remaining
      “start uvicorn” as primary path in the same wave)
- [ ] CEO dress-rehearsal path works on **Rust only**:
  1. `pksp serve` (mock or real vision as configured)
  2. Login → employees → enroll photos → `embedding_ready`
  3. Dashboard: video state truthful + WS HUD
  4. Attendance daily + CSV export
  5. Non-commercial / known-limits banner still accurate
- [ ] Active gates green:
  - `cd apps/edge && cargo test --locked`
  - `cd apps/edge && cargo fmt --all -- --check`
  - `cd apps/edge && cargo clippy --all-targets --locked -- -D warnings`
  - `cd apps/web && npm run lint && npm run typecheck && npm test -- --run && npm run build`
- [ ] Optional: archive a one-time tarball of `apps/api` **outside the repo**
      (operator decision) if local history is wanted; git history already retains it

#### 8.2 Delete Python runtime and tests

- [ ] Remove `apps/api/` entirely (source, tests, `requirements.txt`, `pytest.ini`, venvs if tracked)
- [ ] Remove `scripts/seed_demo.py` **or** replace with a Rust/`pksp` subcommand /
      documented SQL/shell seed that does not require Python
- [ ] Grep for residual product instructions:
  - `uvicorn`, `apps/api`, `pytest`, `pip install -r`, `InsightFace` as **runtime**
  - Fix root `README.md`, `.env.example` comments, `docker-compose` notes, deploy docs
- [ ] Update stack tables: API = Axum/`pksp serve`, vision = `ort` buffalo_l, match = Rust cosine
- [ ] Update repo layout diagrams: no `apps/api`
- [ ] CI: ensure no job still runs Python API tests
- [ ] `.gitignore`: drop Python-only cruft that no longer applies if desired; keep data ignores

#### 8.3 Doc & goal archive cleanup

- [ ] `README.md` quickstart is **Rust-only** (MediaMTX → `pksp serve` → Next.js)
- [ ] `GOAL.md` / `GOAL-RUST.md`: add a short banner at top —
      “Superseded for active work by `GOAL-ADVISOR.md`; historical.”
      Do not rewrite entire historical plans unless necessary.
- [ ] `rust-port-plans/12` cutover language: mark Python rollback as **historical**;
      supported recovery is **git restore + previous binary**, not dual-run product mode
- [ ] `advisor-plans/README.md`: note that after Wave 8, the global boundary
      “do not modify `apps/api/**`” is vacuous because the tree is gone; active
      boundary becomes “do not reintroduce a Python backend”

#### 8.4 Verification after delete

- [ ] `test ! -d apps/api`
- [ ] `! rg -n 'uvicorn|apps/api|pytest' README.md apps/edge/README.md apps/edge/docs --glob '!**/target/**'`
      (allow historical archives under `plans/` / `rust-port-plans/` / old GOAL files if clearly marked historical)
- [ ] Full active-stack gate suite still green
- [ ] Manual smoke: health, login, enroll, WS, attendance CSV against Rust
- [ ] Commit message e.g. `Retire Python API; Rust is the only backend`

**Wave 8 Perfect gate:**

- [ ] No Python backend in tree or primary docs
- [ ] Second operator can follow root README and run CEO path without Python
- [ ] All advisor-plan status rows remain `DONE` (019 resolved)

---

## Plan checklist (status board mirror)

Copy progress into `advisor-plans/README.md` as the source of truth; this list
is the goal-level rollup.

### Foundation
- [ ] 001 Active-stack verification baseline
- [ ] 002 Rust quality gates (fmt/clippy)
- [ ] 003 Security containment

### Recognition
- [ ] 004 Real face pipeline (SCRFD + ArcFace)
- [ ] 005 Frame inference scheduling
- [ ] 006 Enrollment integrity

### Live truth
- [ ] 007 MediaMTX browser-safe path
- [ ] 008 Live WebSocket / camera state
- [ ] 010 Employee + CSV contracts
- [ ] 011 App timezone
- [ ] 009 Truthful metrics / scene

### UX / performance
- [ ] 012 Enrollment UX
- [ ] 013 Browser video state
- [ ] 015 Low-cost hotspots

### Product
- [ ] 016 Dynamic camera wall
- [ ] 017 Attendance explainability
- [ ] 018 Employee edit / deactivate

### Final UI + media pin
- [ ] 014 Responsive / accessible UI
- [ ] 019 MediaMTX upgrade (or deferred with sign-off)

### Python retirement
- [ ] Wave 8 pre-delete proof
- [ ] Wave 8 delete + doc rewrite
- [ ] Wave 8 post-delete verification

---

## Acceptance criteria (whole goal)

1. **All advisor plans 001–018 are `DONE`** in `advisor-plans/README.md`.
2. **Plan 019 is `DONE` or operator-deferred** with explicit status reason.
3. **Every plan’s verification commands** were green at completion time (not
   “assumed”).
4. **Active stack quality gates** green at goal end:
   - Rust: `cargo test --locked`, `cargo fmt --check`, `clippy -D warnings`
   - Web: `lint`, `typecheck`, `test --run`, `build`
5. **Recognition trust:** real SCRFD/ArcFace path per 004; scheduling per 005;
   enrollment integrity per 006; mock still works for theater.
6. **Live trust:** MediaMTX path per 007; WS freshness per 008; metrics per 009;
   contracts per 010; timezone per 011.
7. **Operator UX:** 012–018 product surfaces usable without API/DB surgery.
8. **Security:** 003 containment holds after all later work (re-run RTSP/secret
   audits after Wave 8 docs edits).
9. **Python gone:** `apps/api` removed; no supported Python runtime path; seed
   path is non-Python or deleted with documented alternative.
10. **Docs honesty:** README describes Rust-only stack, model license limits,
    camera/media deps, and how to run CEO demo in ≤ 5 minutes of steps.
11. **No forbidden architecture** reintroduced (FAISS, Postgres, MediaMTX rewrite, etc.).
12. **No secrets/biometrics** in git history of **new** commits from this goal
    (do not re-commit historical bad examples).

---

## Goal-level verification plan

Record evidence under a gitignored path if useful (e.g. `artifacts/verify-advisor/`).

### Automated
1. `cd apps/edge && cargo test --locked` → 0
2. `cd apps/edge && cargo fmt --all -- --check` → 0
3. `cd apps/edge && cargo clippy --all-targets --locked -- -D warnings` → 0
4. `cd apps/edge && cargo check --all-features --locked` → 0
5. `cd apps/web && npm run lint && npm run typecheck && npm test -- --run && npm run build` → 0
6. Plan 003-style secret audits still clean after doc edits
7. `test ! -d apps/api` after Wave 8

### System / manual
8. Health JSON: status, vision_ready, gallery_size, cameras with truthful state
9. Auth login → JWT → employees list
10. WS `/api/ws/live` hello + detections under mock; camera offline/unknown honest
11. Enroll multi-image cumulative → embedding_ready; recompute recoverable
12. Real vision (if models present): recognition + attendance commit; fail-closed if models missing/broken
13. Browser WHEP: truthful video state, retry, live after supervised media restart
14. Attendance daily + expandable events + CSV with timezone-local dates
15. Employee edit name/department + reversible deactivate
16. Camera wall shows one or two cameras from health
17. CEO 5-minute dress rehearsal **twice** on Rust-only stack

---

## Implementation approach

1. Work **wave by wave**, **plan by plan**; Perfect before dependents.
2. Prefer **failing tests first** for behavior fixes (especially 004–006, 008–011).
3. Keep commits **plan-scoped** (one plan’s branch/message per advisor README).
4. When blocked 3+ times on the same issue: set status `BLOCKED — reason`, stop
   inventing, report to operator.
5. Parallelism only where README allows (002∥003; 012∥013∥015; 018 with 016/017).
6. Do **not** start Wave 8 until product waves are green — deleting Python early
   removes a historical reference and confuses “don’t touch apps/api” executors
   mid-flight.
7. After Wave 8, **refuse** any request to reintroduce FastAPI without a new
   explicit product decision.

### Suggested first session

```
1. Read GOAL-ADVISOR.md + advisor-plans/README.md
2. Execute plan 001 only until DONE
3. Then 002 and 003 (parallel ok)
4. Stop and report Wave 1 Perfect before 004
```

Do **not** jump to SCRFD (004), MediaMTX (007), or Python deletion on day one.

---

## Risks (keep visible)

| Risk | Mitigation |
|---|---|
| Skipping 001 → later plans have no harness | Serial Perfect on 001 first |
| 004 “looks ok on HUD” without decode tests | Pure SCRFD/ArcFace tests + fail-closed health |
| Bundling many plans in one mega-diff | Forbidden; plan-scoped commits |
| Touching Python during 001–019 | Forbidden; Wave 8 only |
| Deleting Python before dress rehearsal | Wave 8.1 pre-delete proof mandatory |
| Secret reintroduction in docs during cleanup | Re-run 003 audits after README rewrites |
| MediaMTX upgrade breaks WHEP | 019 only after 007+013; rollback pin together |
| Scope creep (analytics charts, FAISS, Tailwind 4) | Reject; see advisor README rejects |
| Dual vision paths during transition | Single `pksp serve` commit path; no uvicorn |

---

## Progress reporting

After each plan Perfect gate, report:

- Plan id + one-line outcome
- Commands run + results
- README status updated
- Next plan / wave

After Wave 8 Perfect, report:

- Confirmation `apps/api` gone
- CEO demo runbook (Rust-only)
- Residual known limits (CPU FPS, model license, PAD not certified)
- All advisor status rows final

---

## How to start this goal (agent / human)

```text
Execute GOAL-ADVISOR.md: complete every advisor-plans/* plan in the documented
waves (001→019), with thoughtful TDD → implement → test → verify → refactor →
status update per plan. Honor STOP conditions and global boundaries. Do not
modify apps/api during plans 001–019. After all plans are DONE (019 resolved),
run Wave 8: fully remove Python (delete apps/api, replace seed_demo.py, rewrite
README to Rust-only pksp serve + Next.js + MediaMTX). Active stack only:
apps/edge + apps/web. No FAISS/Postgres/MediaMTX rewrite. Start with plan 001
only until DONE.
```

Or paste this file’s **Objective** + **Acceptance criteria** + **Wave 8** into a
goal runner.

---

## Definition of done (one line)

Every advisor plan is honestly `DONE` (019 resolved), the active Rust+web+MediaMTX
stack is trustworthy for CEO demo (real faces when models present, truthful live
state, secure defaults, complete operator UX), and **Python is fully removed** —
a second operator runs only `pksp serve` + Next.js with green active-stack gates
and no FastAPI/uvicorn path left in the product.
