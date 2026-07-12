# 11 — Verification

## Purpose

Prove the system works **without** relying on a full office crowd or perfect hardware, then validate with real walk-throughs before the CEO session.

## Test pyramid

| Layer | What | Tools |
|---|---|---|
| Unit | Embed pack/unpack, cosine, quality rules, FSM transitions, cooldown | pytest |
| Component | Enrollment on still images; gallery match | pytest + sample faces |
| Integration | API CRUD, daily aggregate, WS message shape | httpx / starlette |
| System | Compose + demo RTSP + UI smoke | Manual / Playwright optional |
| Demo dress rehearsal | Full CEO script | Human |

## Fixtures

Prepare `tests/fixtures/faces/`:

- `person_a/` 5 images
- `person_b/` 5 images  
- `impostor/` 2 images  
- `low_quality/` blur / tiny face samples  
- `multi_face.jpg`

Use team-consent photos or public research faces; do not commit private employee biometrics to public git if repo is shared.

## Unit tests (must pass)

### Matching

- Identical normalized vectors → score ≈ 1.0
- Orthogonal vectors → score ≈ 0.0
- Threshold + margin accept/reject matrix

### Attendance FSM

- First commit bidirectional → check_in
- Second after dwell → check_out
- Within cooldown → skip
- IN camera never emits check_out

### Quality gate

- Tiny box rejected
- High det_score large face accepted

## Component tests

### Enrollment

1. Upload 5 good images for A → `embedding_ready true`, `usable ≥ 3`
2. Upload no-face image → rejected with reason
3. Recompute after delete image updates vector

### Recognition offline

1. Build gallery A,B
2. Probe image of A → top1 A above threshold
3. Probe impostor → unknown

## Integration tests

| Case | Expect |
|---|---|
| POST employee duplicate code | 409 |
| GET daily empty day | all active employees absent |
| Insert check_in/out | daily present + duration |
| CSV content-type and header row | correct |
| WS connect receives hello | within 1s |

## System tests (manual checklist)

### Boot

- [ ] `docker compose up` MediaMTX healthy
- [ ] API `/api/health` → vision_ready true after model load
- [ ] Web loads, license banner visible
- [ ] Login works with admin password

### Video path

- [ ] Demo RTSP path plays in browser (WebRTC or fallback)
- [ ] Camera offline simulation shows OFFLINE badge
- [ ] Reconnect restores ONLINE

### Live AI

- [ ] Face box appears on moving subject in sample video / webcam
- [ ] Enrolled subject gets correct name ≥ 80% of stable frontal dwells
- [ ] Wrong name rate ~0 on small gallery with calibrated threshold
- [ ] HUD lag acceptable (&lt;300ms subjective)

### Attendance

- [ ] Single dwell → one check_in
- [ ] Leave and return after cooldown → rules per camera mode hold
- [ ] Daily table and CSV match events list
- [ ] Unrecognized does not create employee attendance row

### Resilience

- [ ] Kill MediaMTX → UI degrades gracefully; restart recovers
- [ ] API restart reloads gallery from DB
- [ ] CPU peg: system still serves UI; may drop vision FPS without crash

## Calibration session protocol

1. Enroll 3 staff at door angle (5+ photos each).
2. Each walks past IN camera 5 times; log scores.
3. One non-enrolled walks past 5 times; log max scores.
4. Set `MATCH_THRESHOLD` above impostor max + buffer; below true min if possible.
5. Set `MATCH_MARGIN` to stop A/B confusion if any.
6. Freeze values in `.env` for demo.

## Performance targets

| Metric | Pass |
|---|---|
| Vision FPS (1 cam) | ≥ 3 on target Mac |
| Vision FPS (2 cam) | ≥ 2 each or priority cam ≥ 3 |
| Event latency | ≤ 2s from stable face to ticker |
| Enroll 5 images | ≤ 30s total on CPU |
| UI load | &lt; 3s on LAN |

## CEO dress rehearsal

Run [demo script](./10-implementation-roadmap.md) twice:

1. Operator A primary
2. Operator B following README only

Record issues; fix blockers only (no feature creep day-of).

## Regression after changes

Always re-run:

1. pytest unit suite  
2. Enroll smoke  
3. One live recognition  
4. One attendance daily row  

## Known acceptable failures (document, don’t hide)

- Profile / extreme backlight misses
- Anti-spoof false rejects on some screens/glare
- Overlay desync under heavy CPU
- buffalo_l non-commercial limitation

## Sign-off template

```
Date:
Host machine:
Cameras used:
Thresholds: MATCH=  MARGIN=  FPS=
pytest: PASS/FAIL
Dress rehearsal: PASS/FAIL
Known issues:
Signed:
```
