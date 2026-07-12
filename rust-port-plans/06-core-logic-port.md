# 06 — Core Logic Port (`pksp-core`)

**Principle:** Zero I/O. No sqlx, no ort, no tokio required (may use std only). Port pure Python modules with tests first.

## 1. Module map

| Python | Rust module | Tests source |
|---|---|---|
| `services/vision/embed.py` | `embed` | `test_embed.py` |
| `services/vision/match.py` | `match` | `test_match.py` |
| `services/vision/quality.py` | `quality` | `test_quality_track_vote.py` |
| `services/vision/track.py` | `track` | same |
| `services/vision/vote.py` | `vote` | same |
| `services/attendance/fsm.py` | `fsm` | `test_fsm.py` |
| `services/attendance/daily.py` | `daily` | `test_daily.py` |
| *(new)* | `zones` | new unit tests |
| *(new)* | `trajectory` | new unit tests |

## 2. Algorithms (parity specs)

### 2.1 Embed

```
l2_normalize(v): v / ||v||  (eps 1e-12)
pack: f32 LE bytes, len==dim
unpack: from bytes + l2_normalize
mean_l2: mean of normalized vectors → normalize
```

### 2.2 Match

```
scores = gallery @ query   # both L2-normalized rows
top1, top2 = best two
margin = top1 - top2
if top1 < threshold → UNKNOWN (employee_id=None)
elif margin < margin_min and N>1 → AMBIGUOUS
else → identity with name
```

Defaults: threshold `0.45`, margin `0.08`.

**Simpler/faster:** store gallery as `Array2<f32>` already row-normalized; single GEMV; no FAISS.

### 2.3 Quality gate

Reject if:

- `det_score < min_det_score` (0.5)
- `min(w,h) < min_face_px` (60) in pixel space
- invalid bbox
- aspect `w/h` outside `[0.4, 2.5]`

Returns `{ ok, reason }`.

**Extension (smart):** optional yaw/pitch from landmarks; Laplacian blur; mean luma for darkness — keep as separate functions so base gate stays simple.

### 2.4 IoU tracker

- Age all tracks each frame
- Greedy match by IoU ≥ 0.3
- Append `TrackVote` when quality_ok
- Spawn new tracks for unmatched dets
- Drop tracks with age > max_age (10 frames)

### 2.5 Vote

- Last `vote_window` (5) history entries
- Best employee_id by (hit_count, avg_score)
- Commit if hits ≥ `vote_min_hits` (3) and avg ≥ min_avg_score

### 2.6 Attendance FSM

Inputs: camera `direction`, `now`, `last_today`, `last_same_camera_ts`, cooldown, min_dwell.

```
if cooldown → skip
direction in → check_in
direction out → check_out
bidirectional:
  no last or last was check_out → check_in
  last check_in and dwell ≥ min → check_out
  else → skip (no_transition)
```

### 2.7 Daily aggregate

Per employee for a date:

- first_in, last_out, counts, duration_minutes
- status: absent / present / incomplete / anomaly

CSV headers from `daily_csv_headers()`.

## 3. New: zones (smart scene core)

```rust
pub struct Zone {
    pub id: String,
    pub kind: ZoneKind, // Approach | Active | Ignore
    pub polygon: Vec<(f32,f32)>, // normalized 0-1
}

pub fn point_in_zone(cx: f32, cy: f32, zone: &Zone) -> bool;
pub fn track_zone(bbox: &[f32;4], zones: &[Zone]) -> Option<ZoneKind>;
```

Default config for single door cam (document tunable):

- **Ignore**: top 10% banner / bottom 5% floor optional
- **Approach**: middle band
- **Active**: lower-center “badge capture” rectangle near door

Commit policy uses Active zone (see `08`).

## 4. New: trajectory

Per track maintain centroid history:

```
centroid = ((x1+x2)/2, (y1+y2)/2)
velocity = centroid_t - centroid_{t-k}
```

Heuristics:

| Signal | Use |
|---|---|
| Speed toward Active zone | approach |
| Lateral high speed, never enter Active | walk-by → do not commit |
| Dwell in Active ≥ T frames | good punch candidate |
| Bidirectional: Δy sign over N frames | in vs out bias |

Keep pure and unit-tested with synthetic centroids.

## 5. API sketch for `pksp-core`

```rust
// match.rs
pub struct MatchResult { pub employee_id: Option<i64>, pub score: f32, pub margin: f32, pub label: String }

// quality.rs
pub struct QualityResult { pub ok: bool, pub reason: Option<String> }

// track.rs
pub struct TrackerState { /* ... */ }
pub fn assign_tracks(...) -> Vec<Track>;

// vote.rs
pub struct VoteCommit { pub employee_id: i64, pub avg_score: f32, pub hits: usize }
pub fn evaluate_vote(...) -> Option<VoteCommit>;

// fsm.rs
pub enum FsmAction { Commit { kind: EventKind }, Skip { reason: SkipReason } }
pub fn on_identity_commit(...) -> FsmAction;

// daily.rs
pub fn aggregate_daily(...) -> Vec<DailyRow>;
```

Prefer owned simple types over heavy traits.

## 6. Faster / simpler / cleaner

| Opportunity | Detail |
|---|---|
| No numpy object overhead | ndarray or plain `Vec<f32>` |
| Track history | `VecDeque` with fixed cap |
| Single-pass match | one matmul |
| Separate smart modules | don't tangle FSM with zones |
| Property tests | random embeddings still L2; IoU symmetric |

## 7. Test plan

Port cases 1:1:

1. Embed roundtrip dim 512  
2. Match threshold / margin / empty gallery  
3. Quality too small / low score / bad aspect  
4. Track IoU association + max age drop  
5. Vote requires 3/5  
6. FSM cooldown, dwell, directions  
7. Daily present/incomplete  
8. Zone point-in-polygon  
9. Walk-by trajectory rejects commit eligibility  

## 8. Acceptance criteria

- [ ] `pksp-core` builds with no optional heavy features
- [ ] All Python pure tests have Rust equivalents green
- [ ] Zones + trajectory APIs documented and tested
- [ ] No DB/network imports in crate

## 9. Source map

| Python path | Rust path |
|---|---|
| `services/vision/embed.py` | `crates/pksp-core/src/embed.rs` |
| `services/vision/match.py` | `.../match.rs` |
| `services/vision/quality.py` | `.../quality.rs` |
| `services/vision/track.py` | `.../track.rs` |
| `services/vision/vote.py` | `.../vote.rs` |
| `services/attendance/fsm.py` | `.../fsm.rs` |
| `services/attendance/daily.py` | `.../daily.rs` |
