# 15 — Risks, Decisions & Open Questions

## 1. Decision log (accepted for this port)

| ID | Decision | Rationale | Doc |
|---|---|---|---|
| D1 | Full backend rewrite in Rust; keep Next.js | Unified edge binary + performance; UI already done | `00` |
| D2 | Axum + Tokio + sqlx | Ecosystem default 2025–26 | `03` |
| D3 | No pure-Rust MediaMTX reimplementation | Scope; use supervise or GStreamer WHEP | `09` |
| D4 | MediaMTX child first, GStreamer WHEP later | Parity speed | `09`, `14` |
| D5 | ONNX via `ort`, buffalo_l files | Embedding continuity | `07` |
| D6 | No FAISS / Postgres / cloud faces | Original product constraints | `03` |
| D7 | Smart scene = zones/trajectory/quality, not VLM | Latency + determinism for attendance | `08` |
| D8 | Camera env upsert on boot | Fix Python seed-only-if-empty | `04` |
| D9 | Preserve HTTP/WS JSON for apps/web | Zero forced frontend rewrite | `05`, `11` |
| D10 | Non-commercial model banner remains | buffalo_l license | `07` |

## 2. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| **Embedding space mismatch** Rust vs InsightFace | **Critical** | M3 cosine ≥0.99 gate; re-enroll plan; golden fixtures |
| GStreamer plugin matrix across Mac/Linux | High | Feature flags; MediaMTX path always available |
| H.265 transcoder CPU load | Medium | Prefer camera H264; ultrafast x264; lower res for browser |
| Dual RTSP pull overload | Medium | Tee frames in M4+/GStreamer |
| sqlx migrate vs existing DB | Medium | Backup + baseline migration strategy |
| WebRTC ICE on multi-homed LAN | Medium | webrtcAdditionalHosts / host candidates config |
| GPL x264 linking obligations | Medium | Dynamic plugin; document redistribution |
| Scope creep (full NVR) | High | Stick to roadmap; reject MediaMTX rewrite |
| Team Rust familiarity | Medium | Pure core first; copy algorithms from tested Python |
| Anti-spoof not ported | Low | Optional later; flag exists in design |
| Frontend assumes MediaMTX HLS ports | Low | Keep 8888/8889 defaults |

## 3. Open questions (resolve during spikes)

| # | Question | Options | When |
|---|---|---|---|
| Q1 | Custom ort pipeline vs `face_id` crate? | Spike both; pick by cosine + control | M3 start |
| Q2 | Vision capture: GStreamer appsink vs retina+ffmpeg? | Linux GST preferred; Mac may differ | M3 |
| Q3 | Single process WHEP worth MediaMTX removal? | Measure stability M6 | M6 |
| Q4 | Soft-delete vs hard-delete employees? | Match Python implementation exactly | M2 |
| Q5 | Bind media ports only on localhost vs LAN? | LAN demo needs LAN bind | M4 |
| Q6 | Zone config format: JSON file vs DB? | JSON file first | M5 |
| Q7 | Keep Python tree how long? | Until M6 + one successful demo week | M6 |
| Q8 | Commercial model path timing? | Document only until legal | later |
| Q9 | APP_TIMEZONE default for site? | Confirm Iran/office TZ | M2 |
| Q10 | Second camera (cam_out) in v1 Rust? | Support schema; enable when RTSP set | M2 |

## 4. Explicitly deferred features

- MiniFASNet anti-spoof integration  
- Multi-node HA / Redis  
- RBAC / SSO  
- Person YOLO  
- FAISS  
- Mobile apps  
- Annotated video encode from vision  
- Full OpenAPI client generation  

## 5. Performance budget (targets)

| Metric | Target |
|---|---|
| Vision process rate cam_in | ≥ 5 FPS sustained on demo HW |
| Match N≤50 | negligible vs infer |
| API p95 non-vision | < 50 ms local |
| WHEP startup | < 3 s typical LAN |
| Attendance false accept | prefer miss over wrong ID (margin) |

## 6. Security / privacy reminders

- Biometric images + embeddings on disk  
- Trusted LAN only  
- Rotate `JWT_SECRET` / `ADMIN_PASSWORD` for any non-dev  
- Do not commit `.env` credentials (already in gitignore norms)  
- RTSP passwords never logged  

## 7. Success review checklist (after M6)

- [ ] Single command (or systemd unit) brings full demo up  
- [ ] No manual transcoder scripts  
- [ ] Next.js unchanged or only env  
- [ ] Smart walk-by suppression demoable  
- [ ] Docs in `rust-port-plans/` still match code (update if drift)  
- [ ] Python path documented as legacy/rollback  

## 8. How to change a decision

1. Update this decision log with new ID and date.  
2. Update affected docs (`03`, `07`, `09`, `14`).  
3. Do not silently diverge implementation from docs.

## 9. Acceptance criteria for documentation phase

- [x] Risks listed with mitigations  
- [x] Open questions time-boxed to milestones  
- [x] Decisions explicit  
- [x] Deferred features listed  
