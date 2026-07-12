# Rust Port Plans — PKSP Check-In

**Status:** Research & design complete (documentation phase)  
**Date:** 2026-07-12  
**Scope:** Full rewrite of the backend/edge stack in Rust; Next.js admin UI retained

## Goals

1. **Single edge runtime** — one primary Rust binary owns API, vision, and media façade (no ad-hoc `/tmp` transcoder scripts).
2. **Faster / simpler / cleaner** than the Python monolith where the architecture allows (pure core, typed state, one frame bus).
3. **Contract parity** with the existing Next.js app (`apps/web`) so the dashboard keeps working.
4. **Smarter scene awareness** — zones, trajectory, pose/blur quality, walk-by rejection (designed in these docs, implemented in vision milestones).
5. **Optimized model usage** — ONNX Runtime via `ort`, execution providers (CPU / OpenVINO / CoreML), shared SCRFD+ArcFace path for live + enroll.

## Non-goals (this port)

| Non-goal | Reason |
|---|---|
| Rewrite Next.js in Rust/WASM | UI already works; BMW M design is TS/React |
| Reimplement MediaMTX from scratch | Multi-year protocol surface; embed or GStreamer WHEP instead |
| FAISS / Postgres / cloud face APIs | Explicitly rejected in original plans; N≪50 |
| PyO3 + InsightFace Python | Defeats no-Python / single-binary goal |
| Production payroll certification | Still on-prem LAN research/demo grade |
| Commercial model license purchase | Document only; keep non-commercial banner |

## Reading order

| # | Doc | Purpose |
|---|---|---|
| 00 | This file | Index and success criteria |
| 01 | [Current system inventory](./01-current-system-inventory.md) | What exists in Python/Next/MediaMTX today |
| 02 | [Target architecture](./02-target-architecture.md) | Workspace, planes, frame bus |
| 03 | [Crate matrix & research](./03-crate-matrix-and-research.md) | Library choices, rejects, licenses |
| 04 | [Data & migrations](./04-data-and-migrations.md) | SQLite schema parity, sqlx |
| 05 | [API & WS contract parity](./05-api-ws-contract-parity.md) | Frozen HTTP/WS shapes for frontend |
| 06 | [Core logic port](./06-core-logic-port.md) | Pure algorithms → `pksp-core` |
| 07 | [Vision engine & models](./07-vision-engine-and-models.md) | buffalo_l ONNX, alignment, mock |
| 08 | [Worker & smart scene](./08-vision-worker-and-smart-scene.md) | Capture, zones, trajectory, commit |
| 09 | [Media plane](./09-media-plane.md) | RTSP, H.265→H.264, WHEP/HLS |
| 10 | [API server & state](./10-api-server-and-state.md) | Axum, AppState, hub, enroll |
| 11 | [Frontend integration](./11-frontend-integration.md) | Next.js touchpoints |
| 12 | [Migration & cutover](./12-migration-and-cutover.md) | Dual-run, DB reuse, rollback |
| 13 | [Verification & benchmarks](./13-verification-and-benchmarks.md) | Tests + FPS/latency gates |
| 14 | [Implementation roadmap](./14-implementation-roadmap.md) | Milestones M0–M6 |
| 15 | [Risks & open questions](./15-risks-and-open-questions.md) | Decision log |

## Target product shape

```
pksp (Rust binary)
├── media plane     RTSP, optional transcode, WHEP/HLS
├── vision plane    SCRFD + ArcFace, track/vote/zones, attendance commits
└── control plane   Axum REST + WebSocket + SQLite + enroll FS
         │
         ▼  same contracts
    apps/web (Next.js) — unchanged except env if ports differ
```

## Workspace (planned)

```
apps/edge/
  Cargo.toml                 # workspace root
  crates/
    pksp-core/               # pure logic (no I/O)
    pksp-db/                 # sqlx + migrations
    pksp-vision/             # ONNX engine, gallery, worker
    pksp-media/              # RTSP / transcode / WHEP façade
    pksp-api/                # Axum routes, auth, hub
    pksp-cli/                # binary: serve, migrate, seed
```

## Relationship to existing `plans/`

| Existing | Role after Rust port docs |
|---|---|
| `plans/00`–`11` | Original product design — still authoritative for *behavior* |
| `plans/06` | API/WS contracts — frozen for frontend |
| `plans/03`–`04` | Vision + attendance — extended by smart scene in `08` |
| `rust-port-plans/*` | **How** to implement that behavior in Rust + improvements |
| `camera_issue_fix.md` | H.265/WHEP lesson — absorbed into `09` |

## Overall success criteria (implementation, later)

- [ ] One `pksp serve` process runs API + vision + media façade on LAN
- [ ] Next.js dashboard works against Rust API with ≤ env changes
- [ ] Pure logic unit tests green (port of pytest suite)
- [ ] Mock vision mode + real buffalo_l ONNX mode both work
- [ ] Browser video via WHEP (H.264 path) without manual `/tmp` scripts
- [ ] Existing SQLite DB + enroll images load (or migrate cleanly)
- [ ] Smart scene: zone-aware commit + walk-by rejection implemented and tested
- [ ] Camera seed **upserts** env paths (fixes Python seed-only-if-empty bug)
- [ ] License banner still present; model non-commercial status documented

## Documentation phase exit criteria

- [x] Plan approved
- [x] All files `00`–`15` written with source maps, research, improvements, acceptance criteria
- [x] Every `apps/api/app/**/*.py` module appears in inventory
- [x] Every frontend API/WS usage covered in `05` / `11`
- [x] Crate matrix has primary + rejected alternative per major concern
- [x] Roadmap milestones are dependency-ordered with exit criteria

## How to use these docs when coding

1. Pick milestone from `14-implementation-roadmap.md`.
2. Read the linked subsystem docs (`06`–`10`).
3. Implement tests from `13` first for pure modules.
4. Do not invent FAISS, Postgres, cloud faces, or a full MediaMTX clone.
5. Prefer simpler designs when they preserve contracts.
