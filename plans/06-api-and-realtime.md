# 06 — API & Realtime

## Base

- Service: FastAPI
- Prefix: `/api`
- OpenAPI: `/docs` (disable or password-protect if exposed beyond trusted LAN)
- Auth MVP: `Authorization: Bearer <token>` after `POST /api/auth/login` with admin password, **or** single shared API key header for LAN demo

## Auth (MVP)

```
POST /api/auth/login
{ "password": "..." }
→ { "access_token": "...", "token_type": "bearer" }
```

- Token: signed JWT (HS256) or random server-side session.
- TTL: e.g. 12 hours.
- WebSocket: `?token=` or first message auth.

All mutating routes require auth. Read-only health may be open on LAN.

## REST endpoints

### Health

| Method | Path | Description |
|---|---|---|
| GET | `/api/health` | `{ status, vision_ready, gallery_size, cameras }` |

### Cameras

| Method | Path | Description |
|---|---|---|
| GET | `/api/cameras` | List cameras + online status |
| PATCH | `/api/cameras/{id}` | Enable/disable, update RTSP (optional MVP) |

### Employees

| Method | Path | Description |
|---|---|---|
| GET | `/api/employees` | List + search `?q=` |
| POST | `/api/employees` | Create metadata |
| GET | `/api/employees/{id}` | Detail + image list + enrollment status |
| PATCH | `/api/employees/{id}` | Update fields / active |
| DELETE | `/api/employees/{id}` | Soft-delete or hard-delete (prefer soft) |
| POST | `/api/employees/{id}/images` | Multipart upload one or many images |
| DELETE | `/api/employees/{id}/images/{image_id}` | Remove image |
| POST | `/api/employees/{id}/recompute-embedding` | Rebuild mean embedding |

#### Create body

```json
{
  "employee_code": "E1001",
  "full_name": "John Doe",
  "department": "Engineering"
}
```

#### Upload response

```json
{
  "received": 5,
  "usable": 4,
  "rejected": [{"filename": "x.jpg", "reason": "no_face"}],
  "embedding_ready": true,
  "num_images_used": 4
}
```

### Attendance

| Method | Path | Description |
|---|---|---|
| GET | `/api/attendance/daily?date=YYYY-MM-DD` | Aggregated daily rows |
| GET | `/api/attendance/events?date=&employee_id=` | Raw events |
| GET | `/api/attendance/daily.csv?date=` | CSV export |
| POST | `/api/attendance/events` | Manual add (optional MVP) |

#### Daily row schema

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

### Live / vision control (optional)

| Method | Path | Description |
|---|---|---|
| GET | `/api/vision/status` | FPS, last frame age, model name |
| POST | `/api/vision/reload-gallery` | Force gallery rebuild |

## WebSocket

### Endpoint

`WS /api/ws/live`

### Client → server (optional)

```json
{ "type": "ping" }
{ "type": "subscribe", "cameras": ["cam_in", "cam_out"] }
```

### Server → client messages

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

`bbox` = normalized `[x1,y1,x2,y2]` relative to frame.

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

Periodic every 2–5s or on change.

#### `error`

```json
{ "type": "error", "code": "vision_degraded", "message": "..." }
```

## Error format (REST)

```json
{ "detail": "Employee code already exists" }
```

Use proper HTTP codes: 400 validation, 401 auth, 404 missing, 409 conflict, 503 vision not ready.

## CORS

Allow Next origin (e.g. `http://localhost:3000` and LAN IP) via env `CORS_ORIGINS`.

## Rate limits

Not required for LAN MVP. Optional: upload max 10 images / request, max 5MB each.

## Idempotency

Enrollment recomputes are safe to retry. Attendance events are server-generated (not client idempotent keys).

## Testing the API

- `pytest` + `httpx.AsyncClient` for REST
- WS test with starlette test client
- Vision mocked in unit tests; integration test with sample images
