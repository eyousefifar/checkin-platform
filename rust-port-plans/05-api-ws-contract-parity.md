# 05 — API & WebSocket Contract Parity

**Freeze rule:** The Next.js app in `apps/web` must work against the Rust API with at most environment variable changes. Do not rename fields without a coordinated frontend change.

Sources: `plans/06-api-and-realtime.md`, `apps/api/app/routers/*`, `apps/web/src/lib/{api,types}.ts`.

## 1. Base

| Item | Value |
|---|---|
| Prefix | `/api` |
| Default bind | `0.0.0.0:8000` |
| OpenAPI | Nice-to-have via `utoipa` later; not required for parity |
| CORS | `CORS_ORIGINS` list; credentials allowed |
| Auth | `Authorization: Bearer <jwt>` |
| Error shape | FastAPI-like `{ "detail": "..." }` for 4xx where frontend expects `j.detail` |

Frontend error parsing (`api.ts`):

```ts
detail = j.detail || JSON.stringify(j);
```

## 2. Auth

### `POST /api/auth/login`

Request:

```json
{ "password": "..." }
```

Response:

```json
{ "access_token": "<jwt>", "token_type": "bearer" }
```

- Compare password to `ADMIN_PASSWORD` (constant-time).
- JWT HS256 claims: `sub` (e.g. `"admin"`), `exp`.
- TTL: `JWT_TTL_HOURS` (default 12).
- Wrong password → 401 `{ "detail": "..." }`.

### Auth dependency

- Missing/invalid bearer → 401.
- Health may be public (current Python: health public).
- WS: `?token=` query accepted (frontend `wsUrl()`).

## 3. Health (public)

### `GET /api/health`

```json
{
  "status": "ok",
  "vision_ready": true,
  "vision_provider": "CPUExecutionProvider",
  "gallery_size": 3,
  "cameras": [
    {
      "id": "cam_in",
      "name": "Entrance",
      "direction": "bidirectional",
      "enabled": true,
      "webrtc_path": "cam_in_h264"
    }
  ]
}
```

**Critical:** `page.tsx` uses `cameras[].webrtc_path` for WHEP. Never omit.

## 4. Employees (auth)

| Method | Path | Notes |
|---|---|---|
| GET | `/api/employees?q=` | search name/code |
| POST | `/api/employees` | 201 create |
| GET | `/api/employees/{id}` | detail + images |
| PATCH | `/api/employees/{id}` | update fields / active |
| DELETE | `/api/employees/{id}` | soft or hard — match Python |
| POST | `/api/employees/{id}/images` | multipart files |
| DELETE | `/api/employees/{id}/images/{image_id}` | if implemented |
| POST | `/api/employees/{id}/recompute-embedding` | rebuild mean |

### Employee JSON (`_emp_dict`)

```json
{
  "id": 1,
  "employee_code": "E1001",
  "full_name": "John Doe",
  "department": "Engineering",
  "is_active": true,
  "image_count": 5,
  "usable_images": 4,
  "embedding_ready": true,
  "num_images_used": 4,
  "images": [
    {
      "id": 10,
      "file_path": "enroll/1/abc.jpg",
      "usable": true,
      "reject_reason": null
    }
  ]
}
```

### Create body

```json
{
  "employee_code": "E1001",
  "full_name": "John Doe",
  "department": "Engineering"
}
```

Duplicate code → 409.

### Upload response

```json
{
  "received": 5,
  "usable": 4,
  "rejected": [{ "filename": "x.jpg", "reason": "no_face" }],
  "embedding_ready": true,
  "num_images_used": 4
}
```

## 5. Attendance (auth)

| Method | Path | Notes |
|---|---|---|
| GET | `/api/attendance/daily?date=YYYY-MM-DD` | aggregated rows |
| GET | `/api/attendance/events?date=&employee_id=` | raw events |
| GET | `/api/attendance/daily.csv?date=` | CSV download |

### Daily row

```json
{
  "employee_id": 1,
  "employee_code": "E1001",
  "full_name": "John Doe",
  "department": "Engineering",
  "first_in": "2026-07-12T08:01:02Z",
  "last_out": "2026-07-12T17:04:11Z",
  "duration_minutes": 543,
  "status": "present",
  "check_in_count": 1,
  "check_out_count": 1
}
```

Statuses: `absent` | `present` | `incomplete` | `anomaly` (match `daily.py`).

## 6. Cameras (auth)

| Method | Path | Notes |
|---|---|---|
| GET | `/api/cameras` | list + online if available |
| PATCH | `/api/cameras/{id}` | enable, rtsp, webrtc_path optional |

## 7. WebSocket

### Endpoint

`WS /api/ws/live?token=...`

### Server → client

#### `hello`

```json
{ "type": "hello", "server_ts": 1710000000.0, "gallery_version": 3 }
```

#### `camera_status`

```json
{
  "type": "camera_status",
  "camera_id": "cam_in",
  "online": true,
  "last_frame_age_ms": 120
}
```

#### `detections`

```json
{
  "type": "detections",
  "camera_id": "cam_in",
  "ts": 1710000000.12,
  "frame_w": 1920,
  "frame_h": 1080,
  "faces": [
    {
      "track_id": 3,
      "bbox": [0.4, 0.2, 0.55, 0.5],
      "label": "John Doe",
      "employee_id": 1,
      "score": 0.72,
      "quality_ok": true,
      "state": "tracking"
    }
  ]
}
```

- `bbox` normalized `[x1,y1,x2,y2]` in 0–1.
- `state`: `tracking` | `committed` | future: `approaching` | `low_quality` | `cooldown`.

#### `attendance`

```json
{
  "type": "attendance",
  "event_id": 99,
  "employee_id": 1,
  "name": "John Doe",
  "kind": "check_in",
  "camera_id": "cam_in",
  "score": 0.72,
  "ts": 1710000001.0
}
```

#### `metrics`

```json
{
  "type": "metrics",
  "cameras_online": 2,
  "present_count": 12,
  "events_today": 28,
  "vision_fps": { "cam_in": 5.1, "cam_out": 4.8 }
}
```

#### `error` (optional)

```json
{ "type": "error", "code": "vision_degraded", "message": "..." }
```

### Client → server (optional)

```json
{ "type": "ping" }
{ "type": "subscribe", "cameras": ["cam_in"] }
```

MVP may ignore subscribe and broadcast all (Python behavior).

## 8. Intentional additive extensions (non-breaking)

| Extension | Why | Compatibility |
|---|---|---|
| Extra fields on detections (`zone`, `velocity`) | Smart scene | Frontend ignores unknown JSON fields |
| `vision_provider` already present | health | keep |
| New WS type `scene` | debug | ignore if client unknown — **must not break** parsers that switch on `type` |

Frontend `useLiveWs` must tolerate unknown types (verify when implementing; add default branch if missing).

## 9. Video is not API

WHEP base is **not** served by FastAPI today:

- `NEXT_PUBLIC_WEBRTC_BASE` default `http://localhost:8889`
- Path from health `webrtc_path`
- HLS fallback port `8888`

Rust media may keep these ports or proxy under `/media/` later — if ports change, document env updates in `11`.

## 10. Faster / simpler / cleaner

| Item | Improvement |
|---|---|
| Typed DTOs | `serde` structs shared; single source of truth |
| Unified error mapper | map `AppError` → status + `{detail}` |
| Broadcast hub | `tokio::sync::broadcast` of `WsEvent` enum |
| OpenAPI optional | generate only if needed for external clients |

## 11. Acceptance criteria

- [ ] Every endpoint used by `apps/web` listed and implemented
- [ ] JSON field names match TypeScript types
- [ ] Health includes `webrtc_path`
- [ ] WS message types match `types.ts`
- [ ] 401 clears token behavior still works (status code parity)
- [ ] Contract tests: golden JSON fixtures from Python responses

## 12. Source map

| Python | Rust |
|---|---|
| `routers/*.py` | `pksp-api/src/routes/*` |
| `auth.py` | `pksp-api/src/auth.rs` |
| `ws/hub.py` | `pksp-api/src/hub.rs` |
| `lib/types.ts` | contract consumer (keep) |
