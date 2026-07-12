# 03 — Vision Pipeline

## Design intent

Produce **trustworthy identity hypotheses** from office camera frames under **CPU / Apple Silicon** limits. Prefer rejecting weak evidence over inventing attendance.

## Pipeline stages (MVP v1)

```
RTSP frame
  → latest-frame buffer (drop stale)
  → throttle (target 3–8 FPS)
  → optional resize (long edge ~640–960)
  → FaceAnalysis.get(frame)   # det + kps + embedding
  → per-face quality gate
  → cosine match vs gallery (if quality OK)
  → assign / update track_id (IoU)
  → append vote to track history
  → if vote commit → optional anti-spoof → attendance FSM
  → emit detections WS event every processed frame
```

## Model stack

### Primary: InsightFace `buffalo_l`

| Part | Role |
|---|---|
| Detector (SCRFD-class in pack) | Face boxes + score |
| Landmarks | Alignment for recognition |
| Recognition (R50 / ArcFace-family) | 512-d embedding |
| GenderAge | **Disable** for MVP (`allowed_modules` det+recognition only if needed) |

Init sketch:

```python
from insightface.app import FaceAnalysis

app = FaceAnalysis(
    name="buffalo_l",
    providers=["CPUExecutionProvider"],  # try CoreML later on Mac
)
app.prepare(ctx_id=-1, det_size=(640, 640))
```

### Optional: MiniFASNet anti-spoof

- Input: cropped face ~80×80 (per project docs).
- Output: live vs print/replay scores.
- **When:** only when a track is about to commit an attendance event.
- **Action:** if spoof score fails → mark `rejected_spoof`, no attendance row (still can show HUD warning).

### Explicitly deferred

| Model | When to revisit |
|---|---|
| Standalone SCRFD-10GF | Multi-cam GPU, crowded scenes |
| CVLFace AdaFace / KP-RPE | Hard low-quality office footage after baselines fail |
| YOLO person tracker | Frequent multi-person false faces / posters |
| FAISS | Gallery ≫ 10k or multi-node search |

## FPS and CPU budget

| Setting | Default | Notes |
|---|---|---|
| `VISION_TARGET_FPS` | 5 | Per camera |
| Max cameras | 2 | Share one inference lock |
| Inference size | det_size 640 | Lower to 320 if overloaded |
| Frame buffer | size 1 | Always freshest frame |
| HUD publish | every processed frame | UI independent of camera native FPS |

**Priority under load:** keep camera 1 (IN) at target FPS; drop camera 2 rate first.

## Quality gate

Reject faces that would produce garbage embeddings.

| Check | Default rule | Rationale |
|---|---|---|
| Detection score | `det_score ≥ 0.5` | Drop weak dets |
| Face size | `min(w,h) ≥ 60 px` (in full-res coords) | Small faces unstable |
| Aspect | optional reject extreme boxes | Det noise |
| Blur (optional v1.1) | Laplacian variance ≥ T | Motion blur |
| Pose (optional v1.1) | yaw/pitch from landmarks | Profile too hard |

Quality-fail faces may still draw a gray box labeled `LOW QUALITY` for sci-fi feel, but **must not** vote toward attendance.

## Recognition / matching

### Gallery representation

- Each active employee: one **mean L2-normalized** embedding (from 5–10 enroll images).
- Optionally keep per-image vectors for debug; match uses mean only in MVP.

### Similarity

```
score(a, b) = cosine(a, b) = dot(a, b)  # both L2-normalized
```

### Accept identity if all true

1. `top1_score ≥ MATCH_THRESHOLD` (start **0.40–0.50**, calibrate)
2. `top1_score - top2_score ≥ MATCH_MARGIN` (start **0.05–0.10**)
3. Employee `is_active = true`

Else label as `UNKNOWN` (or `AMBIGUOUS` if top1 high but margin low).

### Calibration procedure (demo day)

1. Enroll 3–5 staff with door-angle photos.
2. Collect true-match scores while walking past camera.
3. Collect false-match scores (impostors).
4. Set threshold near high true-positive with near-zero false accepts for CEO demo.
5. Prefer **fewer false accepts** over missing a few frames (voting recovers).

## Tracking

Lightweight IoU tracker (no ByteTrack required for MVP).

| Param | Default |
|---|---|
| IoU match threshold | 0.3 |
| Max age (frames without match) | ~1–2 seconds worth |
| New track id | monotonic integer per camera |

Each track holds:

- `track_id`
- `bbox` last
- `history`: deque of `{employee_id|None, score, ts}` length N
- `last_commit_ts`

## Temporal voting

Do not check in on one frame.

| Param | Default | Meaning |
|---|---|---|
| `VOTE_WINDOW` | 5 | last K processed frames for track |
| `VOTE_MIN_HITS` | 3 | min frames agreeing on same employee_id |
| `VOTE_MIN_AVG_SCORE` | threshold | average score of agreeing hits |

**Commit** when same `employee_id` appears in ≥ `VOTE_MIN_HITS` of last `VOTE_WINDOW` quality-ok frames and avg score OK.

Reset vote partials on identity flip.

## Anti-spoof policy

```
if not ENABLE_ANTISPOOF:
    allow commit
else:
    if spoof_score is live:
        allow commit
    else:
        emit warning; block attendance
```

Document clearly: demo-grade PAD only.

## Enrollment pipeline

For each uploaded image:

1. Decode image (BGR).
2. `faces = app.get(img)`.
3. If 0 faces → reject image reason `no_face`.
4. If &gt;1 face → pick largest with quality OK (or reject `multi_face` for strict mode).
5. Take embedding, L2-normalize, store.
6. After all images: mean of usable embeddings → L2-normalize → `employees.embedding`.
7. Require **≥ 3** usable images to activate (configurable).

### Enrollment photo guidance (show in UI)

- Face camera / similar angle to door cam
- Even lighting, no heavy sunglasses
- One person per photo
- Neutral expression mix OK
- 5–10 images better than 1 studio headshot

## Output events

### Per processed frame (`detections`)

Normalized bboxes `[x1,y1,x2,y2]` in 0–1 relative to frame so UI scales with player.

### On commit attempt

Hand off to attendance service with:

- `employee_id` or null
- `camera_id`
- `score`, `margin`
- `track_id`
- `ts`
- `spoof_ok`

## Performance checklist

- [ ] Single FaceAnalysis instance
- [ ] Inference lock across cameras
- [ ] Latest-frame only buffers
- [ ] det_size tunable
- [ ] Gender/age models not loaded if avoidable
- [ ] Anti-spoof not on every face every frame
- [ ] No video re-encode in Python for UI

## Failure injection tests

| Scenario | Expected |
|---|---|
| Blurry walk-by | No commit |
| Printed photo on phone | Spoof block (if enabled) or high caution |
| Two employees in frame | Two tracks; independent votes |
| Employee looks away | Quality fail / no vote |
| Gallery empty | All UNKNOWN boxes |

## Tuning knobs (config)

All thresholds must be config/env driven — never hardcode magic only in source without constants module.
