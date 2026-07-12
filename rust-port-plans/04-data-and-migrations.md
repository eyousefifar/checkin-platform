# 04 — Data Model & Migrations

## 1. Goals

- **Byte-compatible** embeddings with Python (`float32` LE, dim 512, L2-normalized).
- Reuse existing `data/pksp.db` and `data/enroll/` when possible.
- Replace `create_all` with **versioned sqlx migrations**.
- Fix camera seed to **upsert** from env.

## 2. Schema parity (SQLite)

### `employees`

| Column | Type | Notes |
|---|---|---|
| id | INTEGER PK AUTOINCREMENT | |
| employee_code | TEXT UNIQUE NOT NULL | e.g. E1001 |
| full_name | TEXT NOT NULL | |
| department | TEXT NULL | |
| is_active | INTEGER NOT NULL DEFAULT 1 | bool |
| created_at | TEXT/DATETIME | store UTC ISO or unix; match Python naive UTC |
| updated_at | TEXT/DATETIME | |

### `employee_images`

| Column | Type | Notes |
|---|---|---|
| id | INTEGER PK | |
| employee_id | INTEGER FK CASCADE | |
| file_path | TEXT | relative to data root: `enroll/{id}/{uuid}.jpg` |
| usable | INTEGER | |
| reject_reason | TEXT NULL | no_face, multi_face, low_quality, decode_error, missing_file |
| created_at | DATETIME | |

### `employee_embeddings`

| Column | Type | Notes |
|---|---|---|
| employee_id | INTEGER PK FK | 1:1 |
| dim | INTEGER DEFAULT 512 | |
| vector | BLOB NOT NULL | 2048 bytes = 512 × f32 |
| num_images_used | INTEGER | |
| model_name | TEXT | buffalo_l / mock |
| updated_at | DATETIME | |

### `cameras`

| Column | Type | Notes |
|---|---|---|
| id | TEXT PK | cam_in, cam_out |
| name | TEXT | |
| rtsp_url | TEXT | |
| webrtc_path | TEXT | MediaMTX/GStreamer path |
| direction | TEXT | in \| out \| bidirectional |
| enabled | INTEGER | |
| sort_order | INTEGER | |

### `attendance_events`

| Column | Type | Notes |
|---|---|---|
| id | INTEGER PK | |
| employee_id | INTEGER NULL FK | |
| camera_id | TEXT FK | |
| kind | TEXT | check_in, check_out, unrecognized, rejected_spoof, rejected_low_conf |
| score | REAL NULL | |
| margin | REAL NULL | |
| track_id | INTEGER NULL | |
| needs_review | INTEGER DEFAULT 0 | |
| meta_json | TEXT NULL | future zones debug |
| ts | DATETIME NOT NULL | |
| local_date | DATE NOT NULL | APP_TIMEZONE date |

Indexes (parity):

- `(local_date, employee_id)`
- `(ts)`
- `(employee_id, camera_id, ts)`

### `app_meta`

| Column | Type | Notes |
|---|---|---|
| key | TEXT PK | gallery_version |
| value | TEXT | |

## 3. Embedding serialization (frozen contract)

```text
pack:   f32 little-endian, C order, length == dim
unpack: from bytes → L2 normalize defensive
mean:   mean of L2-normalized vectors → L2 normalize again
```

Python reference: `apps/api/app/services/vision/embed.py`.

Rust:

```rust
fn pack_embedding(v: &[f32], dim: usize) -> Result<Vec<u8>, EmbedError>;
fn unpack_embedding(blob: &[u8], dim: usize) -> Result<Array1<f32>, EmbedError>;
fn mean_l2(vectors: &[Array1<f32>], dim: usize) -> Result<Array1<f32>, EmbedError>;
```

**Must be bitwise-compatible** so existing enrollments match live frames under the same model.

## 4. Filesystem layout

```
data/
  pksp.db
  enroll/
    {employee_id}/
      {uuid12}.jpg
  models/           # optional local ONNX cache
  tmp/
```

## 5. Migrations strategy

### Initial migration `001_init.sql`

Create all tables + indexes matching SQLAlchemy models (inspect live DB with `.schema` when implementing to guarantee match).

### Future migrations

| ID | Purpose |
|---|---|
| 002 | Optional: zone config table |
| 003 | Optional: track trajectory debug columns |

### Boot sequence

```
sqlx::migrate!().run(&pool).await?;
seed_or_upsert_cameras(&pool, &settings).await?;
ensure_app_meta_gallery_version(&pool).await?;
```

## 6. Camera seed: intentional improvement

### Python behavior (bug)

```python
if existing:
    return  # env changes ignored
```

### Rust behavior (required)

```text
UPSERT cameras by id:
  - On first insert: set all fields from Settings
  - On conflict: update rtsp_url, webrtc_path, direction, name, enabled
    IF settings.camera_upsert = true (default true)
  OR only update when env flag FORCE_CAMERA_SEED=1
```

Recommended default: **upsert network fields from env every boot**, never delete attendance history. Operators can override RTSP in DB only if `CAMERA_UPSERT=false`.

## 7. Gallery version

Keep `app_meta.gallery_version` integer string for parity with WS `hello.gallery_version` and worker reload.

Flow:

1. Enroll mutates embeddings  
2. `UPDATE app_meta SET value = value+1`  
3. API reloads `Gallery` into `RwLock`  
4. Vision worker sees version change (or receives `Notify`)

## 8. sqlx types mapping

| Domain | Rust type |
|---|---|
| timestamps | `chrono::NaiveDateTime` (match Python naive UTC) **or** `DateTime<Utc>` with conversion |
| local_date | `chrono::NaiveDate` |
| bool | `bool` via sqlx sqlite |
| blob | `Vec<u8>` |

**Parity note:** Python stores naive UTC. Pick one Rust representation and convert consistently in API ISO-Z responses (`...Z` suffix as today).

## 9. Faster / simpler / cleaner

| Improvement | Detail |
|---|---|
| WAL mode | Concurrent readers during vision |
| Single writer task | Avoid SQLite lock storms from multi-task commits |
| Migrations | Reproducible deploys |
| Upsert cameras | Fixes silent stale webrtc_path |
| Typed row structs | No ORM magic; explicit queries |

## 10. Compatibility with existing DB

1. If schema matches → attach and run; sqlx migrate table may need baseline.
2. Baseline strategy for existing Python DBs:
   - Option A: `sqlx migrate add` matching schema and mark applied if tables exist
   - Option B: export CSV + re-import (only if schema drift)
3. Enroll paths relative → keep `DATA_DIR` same absolute root.

## 11. Acceptance criteria

- [ ] Schema 1:1 with production Python DB (verify via `.schema`)
- [ ] Embedding pack/unpack tests cross-check known vectors from Python fixtures
- [ ] Migrations run on empty dir
- [ ] Camera upsert documented and tested
- [ ] Gallery version increments on enroll

## 12. Source map

| Python | Rust |
|---|---|
| `db/models.py` | `pksp-db/src/models.rs` + migrations |
| `db/session.py` | `pksp-db/src/pool.rs`, `seed.rs` |
| `embed.py` pack/unpack | `pksp-core/src/embed.rs` |
