# 05 — Data Model

## Storage

- **Engine:** SQLite file `data/pksp.db`
- **ORM:** SQLAlchemy 2.x
- **Embeddings:** BLOB (float32 little-endian, length 512 × 4 = 2048 bytes) + metadata columns
- **Images:** filesystem `data/enroll/{employee_id}/{uuid}.jpg` with DB paths

## ER diagram

```
employees 1───* employee_images
employees 1───* attendance_events
cameras   1───* attendance_events
employees 1───0..1 employee_embeddings   (mean vector; 1:1 for MVP)
```

## Tables

### `employees`

| Column | Type | Notes |
|---|---|---|
| `id` | INTEGER PK | Internal |
| `employee_code` | TEXT UNIQUE | e.g. E1001 |
| `full_name` | TEXT NOT NULL | |
| `department` | TEXT NULL | |
| `is_active` | BOOLEAN DEFAULT 1 | Soft disable |
| `created_at` | DATETIME | UTC |
| `updated_at` | DATETIME | UTC |

### `employee_images`

| Column | Type | Notes |
|---|---|---|
| `id` | INTEGER PK | |
| `employee_id` | FK → employees | ON DELETE CASCADE |
| `file_path` | TEXT | Relative to data root |
| `usable` | BOOLEAN | Passed face quality |
| `reject_reason` | TEXT NULL | no_face, multi_face, low_quality |
| `created_at` | DATETIME | |

### `employee_embeddings`

| Column | Type | Notes |
|---|---|---|
| `employee_id` | INTEGER PK/FK | 1:1 |
| `dim` | INTEGER | 512 |
| `vector` | BLOB | float32 LE |
| `num_images_used` | INTEGER | |
| `model_name` | TEXT | e.g. buffalo_l |
| `updated_at` | DATETIME | |

### `cameras`

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PK | `cam_in`, `cam_out` |
| `name` | TEXT | Display |
| `rtsp_url` | TEXT | May also come from env |
| `webrtc_path` | TEXT | MediaMTX path name |
| `direction` | TEXT | `in` \| `out` \| `bidirectional` |
| `enabled` | BOOLEAN | |
| `sort_order` | INTEGER | Dashboard tile order |

Seed from env on first boot if table empty.

### `attendance_events`

| Column | Type | Notes |
|---|---|---|
| `id` | INTEGER PK | |
| `employee_id` | FK NULL | Null if unrecognized |
| `camera_id` | FK → cameras | |
| `kind` | TEXT | check_in, check_out, unrecognized, … |
| `score` | REAL NULL | Cosine score |
| `margin` | REAL NULL | top1−top2 |
| `track_id` | INTEGER NULL | |
| `needs_review` | BOOLEAN DEFAULT 0 | |
| `meta_json` | TEXT NULL | Extra debug |
| `ts` | DATETIME NOT NULL | Event time UTC |
| `local_date` | DATE NOT NULL | APP_TIMEZONE date for fast daily queries |

Indexes:

- `(local_date, employee_id)`
- `(ts)`
- `(employee_id, camera_id, ts)`

### `app_meta` (optional)

| Column | Type | Notes |
|---|---|---|
| `key` | TEXT PK | e.g. `gallery_version` |
| `value` | TEXT | |

## Embedding serialization

```python
import numpy as np

def pack(vec: np.ndarray) -> bytes:
    v = vec.astype(np.float32).reshape(-1)
    assert v.shape[0] == 512
    return v.tobytes(order="C")

def unpack(blob: bytes) -> np.ndarray:
    return np.frombuffer(blob, dtype=np.float32).copy()
```

Always L2-normalize after unpack before match (defensive).

## Gallery load

On startup / version bump:

```python
rows = select active employees join embeddings
matrix = stack vectors  # (N, 512)
ids = [...]
names = [...]
```

In-memory structure held by `GalleryService`.

## Migrations

MVP: `create_all` on startup is acceptable.  
Prefer Alembic from Phase B if multiple engineers touch schema.

## File layout

```
data/
  pksp.db
  enroll/
    1/
      a1.jpg
      a2.jpg
  models/          # insightface ~/.insightface may be used instead
  tmp/
```

Gitignore entire `data/` except `.gitkeep`.

## Privacy-sensitive fields

- Face images on disk
- Embedding vectors (biometric derivative)
- Optional future: event thumbnails

See [09-security-privacy](./09-security-privacy.md).

## Example daily query

```sql
SELECT e.employee_code, e.full_name,
       MIN(CASE WHEN ae.kind = 'check_in' THEN ae.ts END) AS first_in,
       MAX(CASE WHEN ae.kind = 'check_out' THEN ae.ts END) AS last_out
FROM employees e
LEFT JOIN attendance_events ae
  ON ae.employee_id = e.id AND ae.local_date = :d
WHERE e.is_active = 1
GROUP BY e.id
ORDER BY e.full_name;
```

## Seed data (demo)

- 2 cameras
- 0–3 sample employees (optional synthetic) via `scripts/seed_demo.py`
- No fake attendance unless requested for UI screenshots
