# 13 — Verification & Benchmarks

## 1. Philosophy

Mirror original GOAL.md loop: tests first for pure logic, then integration, then manual demo path. Optimize only with measurements.

## 2. Unit tests (`pksp-core`)

| Suite | Cases (from Python) |
|---|---|
| embed | pack/unpack dim; mean L2; bad dim errors |
| match | threshold reject; margin ambiguous; empty gallery; top1 hit |
| quality | low score; small face; bad aspect; ok face |
| track | IoU match; new track; max age drop; history bounds |
| vote | needs 3/5; avg score; ignore null employee |
| fsm | in/out/bidirectional; cooldown; min dwell |
| daily | present/incomplete/absent/anomaly |
| zones | point in polygon; track zone classification |
| trajectory | walk-by vs approach synthetic paths |

Run: `cargo test -p pksp-core`

## 3. DB tests (`pksp-db`)

- migrations on temp file  
- camera upsert  
- employee CRUD  
- embedding blob roundtrip  
- attendance insert + query by local_date  

## 4. Vision tests (`pksp-vision`)

| Mode | What |
|---|---|
| Mock engine | deterministic faces; enroll mean; match hits |
| Ort (ignored if no models) | `#[ignore]` or feature `models` cosine vs fixture |
| Pipeline | process synthetic frame → detections shape |

Fixture policy: no real biometric PII in git; use synthetic or public sample faces if needed.

## 5. API tests (`pksp-api`)

Use `axum`/`tower` oneshot or hyper client against `Router` with test state:

- login ok/fail  
- employees 401 without token  
- create + list  
- health shape includes webrtc_path  
- daily empty day  
- WS connect → hello (tokio-tungstenite)  

## 6. Contract goldens

Capture Python responses once:

```
tests/goldens/health.json
tests/goldens/employee.json
tests/goldens/daily_row.json
tests/goldens/ws_detections.json
```

Rust tests: field presence + types (not full brittle equality if timestamps differ).

## 7. Forbidden stack test

Port `test_forbidden_stack.py` spirit:

- Cargo.toml workspace must not depend on FAISS, cloud face SDKs, postgres for MVP features  
- Document allowlist  

## 8. Benchmarks (`criterion` or simple bins)

| Bench | Target (initial) | Notes |
|---|---|---|
| match N=50 dim=512 | ≪ 0.1 ms | should be noise |
| quality+track 5 faces | ≪ 0.05 ms | |
| SCRFD+ArcFace one face 640 | measure on HW | goal: sustain ≥5 FPS with headroom |
| end-to-end process loop | p50/p95 | under load |

Record baseline on the Intel Linux box used for VAAPI work.

## 9. Manual / system verification (CEO path)

From root README:

1. Cold open dashboard  
2. Enroll colleague  
3. Walk past → name locks ~2s with vote  
4. Attendance first_in → CSV  
5. License banner visible  

Plus media:

6. WHEP video visible without `/tmp` manual transcoder  
7. Kill RTSP → online false → recover  

## 10. Smart scene verification

| Scenario | Expected |
|---|---|
| Walk across FOV outside active zone | no check_in |
| Stop in active zone with enrolled face | check_in |
| Stand in cooldown | no double punch |
| Bidirectional: enter then later leave | in then out after dwell |
| Two faces | correct assignment / no cross-id flip |

## 11. Acceptance criteria

- [ ] `cargo test` green for default features (mock)  
- [ ] Optional models tests documented  
- [ ] Manual CEO path passes on Rust  
- [ ] Benchmark notes stored (markdown table in PR)  
- [ ] No attendance doubles under reconnect storms  

## 12. Source map

| Python tests | Rust |
|---|---|
| `apps/api/tests/*` | `crates/*/tests` + `tests/` integration |
| `plans/11-verification.md` | this doc + roadmap exits |
