# 08 — Vision Worker & Smart Scene

## 1. Python worker baseline

`apps/api/app/services/vision/worker.py`:

| Loop | Role |
|---|---|
| Capture (per cam) | RTSP OpenCV or FFmpeg VAAPI → `LatestFrameBuffer` |
| Synthetic capture | Enroll images cycle or geometric mock faces |
| Process (per cam) | throttle FPS → infer → track → vote → commit → WS |

Shared inference lock across cameras. Adaptive FPS optional. Gallery reload on version change. Opens DB session per commit.

## 2. Rust worker design

```
for each enabled camera:
  spawn capture_task  → FrameBus (latest)
  spawn process_task  → read bus, run pipeline, publish events
```

### Capture backends (trait)

```rust
trait FrameSource {
    async fn run(self, tx: FrameWatch) -> Result<(), CaptureError>;
}
```

| Backend | Use |
|---|---|
| `SyntheticSource` | MOCK_VISION |
| `GStreamerSource` | RTSP → BGR appsink (preferred Linux) |
| `RetinaFfmpegSource` | pure-ish RTSP + decode |
| `MediaSharedSource` | frames from media plane fan-out (best architecture) |

### Process pipeline (per frame)

```
1. Take latest frame; compute age_ms
2. Optional: crop to detection ROI (union of non-Ignore zones)
3. engine.detect_and_embed (or det-only then selective embed)
4. quality_gate (+ pose/blur extensions)
5. gallery.match for quality_ok faces
6. assign_tracks (IoU)
7. update trajectory / zone state per track
8. evaluate_vote
9. smart commit eligibility (zone + trajectory)
10. if eligible → send CommitRequest to DB task
11. emit detections + camera_status WS
12. periodic metrics
```

### Commit path

```rust
// single writer
CommitRequest { employee_id, camera_id, score, track_id, ts }
→ load camera direction, last events
→ pksp_core::fsm::on_identity_commit
→ INSERT attendance_events
→ hub.attendance
```

**Cleaner than Python:** no Session-per-frame; channel + pool.

## 3. Smart scene — product design

Goal: be aware of **how people move through this doorway**, not only face ID.

### 3.1 Zones (normalized 0–1)

| Zone | Purpose |
|---|---|
| `ignore` | posters, TV reflections, door frame edge — det allowed but no vote |
| `approach` | person is coming; HUD can show APPROACHING |
| `active` | identity lock + punch allowed when vote commits |

Default single-cam layout (tunable JSON/env):

```
active:   x[0.30–0.70], y[0.25–0.85]   # center door
approach: x[0.15–0.85], y[0.10–0.90]   # broader
ignore:   everything else for voting (still draw dets optionally)
```

Config file: `configs/zones.cam_in.json` or DB later.

### 3.2 Trajectory rules

| Condition | Action |
|---|---|
| Track never enters `active` | **no attendance commit** (walk-by) |
| Track crosses `active` with vote OK | commit allowed (subject to FSM) |
| High lateral velocity, brief dwell < T | suppress commit |
| Bidirectional: net Δy over K frames | hint IN vs OUT when direction=bidirectional |
| Multi tracks in active | prefer highest quality_ok + largest face + best score |

### 3.3 Extended quality

| Check | Source | Fail label |
|---|---|---|
| Pose yaw/pitch | landmarks | LOW_POSE |
| Blur Laplacian | face crop gray | LOW_BLUR |
| Exposure | mean luma | LOW_LIGHT / HIGH_GLARE |

Fails → no vote (same as quality_ok=false).

### 3.4 Track states for HUD (additive WS fields)

| state | Meaning |
|---|---|
| `tracking` | parity default |
| `approaching` | in approach zone, no commit yet |
| `ready` | in active, vote building |
| `committed` | just punched |
| `cooldown` | FSM skip |
| `walkby` | rejected by trajectory |
| `low_quality` | quality fail |

Frontend may ignore unknown states until UI updated — still send `label`/`quality_ok`.

### 3.5 Camera health

| Signal | Emit |
|---|---|
| No frames > 2s | online=false |
| Frame frozen (hash identical N times) | `error` vision_degraded |
| Mean luma extreme | warning |
| Infer latency >> interval | drop FPS adaptively |

### 3.6 What we deliberately do **not** add in v1 smart scene

- Full body YOLO (unless false faces dominate)
- VLM scene captions on the hot path
- Cloud analytics

## 4. Adaptive FPS (port + improve)

Python adaptive nudges target 1–15 based on proc cost.

Rust:

```
if avg_infer < 0.6 * interval → raise target
if avg_infer > 1.3 * interval → lower target
priority: cam_in over cam_out under load
```

Expose `vision_fps` in metrics.

## 5. Faster / simpler / cleaner

| Opportunity | Detail |
|---|---|
| Frame fan-out from media | one RTSP session |
| Selective embedding | skip garbage faces |
| Zone ROI det | smaller tensors |
| Commit queue | SQLite friendly |
| Trajectory pure core | unit tested |
| Structured tracing spans | `camera_id`, `infer_ms`, `faces` |
| Remove synthetic theater complexity | keep mock engine; simplify enroll-frame cycling |

## 6. Concurrency diagram

```
capture ──watch──▶ process ──broadcast──▶ hub ──▶ WS clients
                      │
                      ├── gallery RwLock read
                      ├── engine (semaphore)
                      └── commit mpsc ──▶ db_writer ──▶ hub attendance
```

## 7. Config knobs (new + existing)

| Knob | Default | Notes |
|---|---|---|
| VISION_TARGET_FPS | 5 | |
| VISION_ADAPTIVE | false | |
| ZONE_CONFIG | path/json | |
| WALKBY_MIN_DWELL_FRAMES | 3 | in active zone |
| POSE_MAX_YAW | ~45° | tune |
| BLUR_MIN_VAR | tune on site | |
| ENABLE_SMART_SCENE | true | feature flag to disable for pure parity |

## 8. Acceptance criteria

- [ ] Parity mode: same pipeline as Python without zones (flag off)
- [ ] Smart mode: walk-by does not create attendance
- [ ] Active-zone vote still checks in legitimately
- [ ] Bidirectional motion hint tested with synthetic tracks
- [ ] Capture reconnects after RTSP drop
- [ ] Metrics include FPS and online counts
- [ ] No unbounded queues

## 9. Source map

| Python | Rust |
|---|---|
| `worker.py` | `pksp-vision/src/worker/*` |
| capture loops | `.../capture/*` |
| `_infer_frame` | `.../pipeline.rs` |
| — | `pksp-core` zones/trajectory |
| `commit_identity` | `pksp-db` + `pksp-api` attendance service |
