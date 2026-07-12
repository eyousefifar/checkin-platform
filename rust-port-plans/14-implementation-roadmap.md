# 14 — Implementation Roadmap

Ordered milestones. Each has exit criteria. Do not start M{n+1} features before M{n} exit is honest.

## Overview

| Milestone | Theme | Primary crates |
|---|---|---|
| **M0** | Workspace scaffold | all empty shells |
| **M1** | Pure core + tests | `pksp-core` |
| **M2** | DB + API + mock vision + WS | `pksp-db`, `pksp-api`, `pksp-vision` (mock), `pksp-cli` |
| **M3** | Real ONNX engine + enroll parity | `pksp-vision` |
| **M4** | Media supervision (MediaMTX + transcoder) | `pksp-media` |
| **M5** | Smart scene | `pksp-core` zones, worker policy |
| **M6** | Hardening, benches, cutover | all |

Frontend remains `apps/web` throughout.

---

## M0 — Workspace scaffold

### Work

- Create `apps/edge/` Cargo workspace with crate stubs  
- Shared `workspace.dependencies`  
- `pksp-cli` hello `serve` that binds axum health `"ok"`  
- README snippet for building  

### Exit

- [ ] `cargo build` succeeds  
- [ ] Crate graph matches `02-target-architecture.md`  

### Docs

`02`, `03`

---

## M1 — Pure core

### Work

- Port embed, match, quality, track, vote, fsm, daily  
- Full unit tests from Python suite  
- Stub zones/trajectory types (can be incomplete)  

### Exit

- [ ] `cargo test -p pksp-core` green  
- [ ] No I/O deps in core  

### Docs

`06`, `13`

---

## M2 — API + mock vision (frontend works theater mode)

### Work

- sqlx migrations + camera upsert  
- JWT auth, all REST routes  
- LiveHub broadcast WS  
- Mock FaceEngine + synthetic capture worker  
- Gallery load/match  
- Enroll with mock embeddings  
- Attendance commit path  
- Point Next.js at Rust; CEO mock path  

### Exit

- [ ] Login, employees, attendance, dashboard WS work  
- [ ] Health returns cameras + webrtc_path (media may still be external)  
- [ ] `cargo test` default features green  

### Docs

`04`, `05`, `10`, `11`

---

## M3 — Real models

### Work

- ort SCRFD + ArcFace (or face_id spike outcome)  
- Model download path  
- Embedding cosine ≥ 0.99 vs Python fixtures  
- Live RTSP capture backend (GStreamer or retina+ffmpeg)  
- Enroll real photos  
- `MOCK_VISION=false` path  

### Exit

- [ ] Real face match on door cam or recorded sample  
- [ ] Existing DB usable if cosine gate passes  
- [ ] EP CPU works; OpenVINO optional documented  

### Docs

`07`, `12`

---

## M4 — Media plane ownership

### Work

- Supervise MediaMTX child from `pksp-media`  
- H265→H264 transcoder pipeline auto-restart  
- Ensure health `webrtc_path` is browser-safe  
- Remove dependency on `/tmp` scripts  
- Prefer H264 camera URL when configured  

### Exit

- [ ] Dashboard video WHEP works after `pksp serve` only (+ system GStreamer plugins)  
- [ ] Transcoder recovery tested  
- [ ] Document camera H264 preferred config  

### Docs

`09`, `camera_issue_fix.md` lessons

---

## M5 — Smart scene

### Work

- Zone config per camera  
- Trajectory walk-by suppression  
- Pose/blur quality gates  
- Commit only in active zone  
- Bidirectional motion hint  
- HUD states additive  

### Exit

- [ ] Walk-by does not punch  
- [ ] Legitimate door stop punches  
- [ ] Unit tests for trajectory/zones  
- [ ] Feature flag to disable for A/B  

### Docs

`08`

---

## M6 — Hardening & cutover

### Work

- Benchmarks recorded  
- Graceful shutdown  
- Tracing levels  
- Deploy notes (systemd unit sketch)  
- Optional: GStreamer WHEP experiment  
- Deprecate Python path in README (keep code until confident)  

### Exit

- [ ] Verification checklist `13` complete  
- [ ] Rollback procedure drilled once  
- [ ] CEO 5-minute path on Rust only  

### Docs

`12`, `13`, `15`

---

## Parallelism notes

- M1 can start immediately alone.  
- M4 media can prototype in parallel with M3 if staffed, but API contract must exist (M2).  
- M5 must not break M2/M3 parity tests (flag off = old behavior).  

## Estimated effort (order-of-magnitude)

| Milestone | Rough effort |
|---|---|
| M0 | 0.5–1 day |
| M1 | 2–4 days |
| M2 | 1–2 weeks |
| M3 | 1–2 weeks (alignment risk) |
| M4 | 1 week |
| M5 | 1 week |
| M6 | 0.5–1 week |

Depends on GStreamer familiarity and embedding parity luck.

## Definition of done (full port)

All M0–M6 exits green and success criteria in `00-README.md` checked.
