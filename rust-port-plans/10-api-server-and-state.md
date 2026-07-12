# 10 — API Server & Application State

## 1. Goals

- Axum server implementing contract in `05`
- Typed `AppState` replacing Python globals
- Clean hub based on `broadcast`
- Multipart enrollment, JWT auth, CORS

## 2. Crate layout (`pksp-api`)

```
pksp-api/src/
  lib.rs
  state.rs          # AppState
  auth.rs           # JWT
  error.rs          # AppError → Response
  hub.rs            # LiveHub
  routes/
    mod.rs
    health.rs
    auth_routes.rs
    employees.rs
    attendance.rs
    cameras.rs
    ws.rs
  services/         # thin wrappers calling db/vision
```

Binary in `pksp-cli` calls `pksp_api::serve(settings)`.

## 3. AppState

```rust
#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub db: SqlitePool,
    pub gallery: Arc<RwLock<Gallery>>,
    pub hub: LiveHub,
    pub engine: Arc<dyn FaceEngine>,
    pub vision: VisionControl,   // metrics, online map
    pub media: MediaControl,     # optional handles
}
```

Handlers: `State<AppState>`.

## 4. LiveHub design

```rust
#[derive(Clone, Serialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    Hello { server_ts: f64, gallery_version: u64 },
    CameraStatus { .. },
    Detections { .. },
    Attendance { .. },
    Metrics { .. },
    Error { code: String, message: String },
}

pub struct LiveHub {
    tx: broadcast::Sender<WsEvent>,
    gallery_version: AtomicU64,
    metrics: Arc<RwLock<Metrics>>,
}
```

WS handler:

1. Accept, optionally verify JWT from query  
2. Send `Hello`  
3. Subscribe to `broadcast::Receiver`  
4. Loop: `recv` → `send_json` until client gone  

Vision/DB tasks: `hub.publish(event)` — works from any thread/task without asyncio bridge hacks.

**Simpler than Python `broadcast_nowait`.**

## 5. Auth

- `POST /api/auth/login` body password  
- `jsonwebtoken` HS256  
- Middleware/extractor `AuthUser` for protected routes  
- Constant-time password compare (`subtle` crate)

## 6. Error handling

```rust
enum AppError {
    Unauthorized,
    NotFound,
    Conflict(String),
    BadRequest(String),
    Internal(anyhow::Error),
}
// IntoResponse → status + {"detail": "..."}
```

## 7. Employees + enroll

- CRUD via sqlx  
- Multipart: collect `(filename, bytes)`  
- Call `pksp_vision::enroll::save_images_and_enroll`  
- Bump gallery version; `gallery.write().load()`  
- Publish optional metrics update  

**Blocking ONNX on request path:** use `spawn_blocking` so axum workers stay free.

## 8. Attendance routes

- Delegate aggregate to `pksp_core::daily`  
- CSV: write to `String` / stream body  
- Dates parsed as `NaiveDate`

## 9. Health

- Read engine ready/provider  
- gallery size  
- cameras from DB (enabled) including `webrtc_path`  
- optional media online  

## 10. Lifespan / startup (`pksp-cli serve`)

```
1. load Settings
2. ensure data dirs
3. connect pool + migrate + camera upsert
4. construct FaceEngine (mock or ort)
5. load Gallery
6. start media supervisor
7. start vision workers
8. bind axum
9. graceful shutdown: abort tasks, close pool
```

## 11. Faster / simpler / cleaner

| Python | Rust improvement |
|---|---|
| module globals | AppState |
| threadsafe asyncio | broadcast channel |
| Depends(get_db) session | sqlx pool in state |
| blocking infer on event loop risk | spawn_blocking explicit |
| CORS middleware | tower-http CorsLayer |
| scattered logging | tracing spans per request |

## 12. Settings loading

Mirror env vars from `config.py` (see inventory).  
Implement with `figment` or manual `envy`/`std::env` + defaults.  
Document `.env` file support (optional `dotenvy`).

## 13. Acceptance criteria

- [ ] All routes in `05` implemented  
- [ ] JWT login works with existing web login page  
- [ ] WS hello + detections received by `useLiveWs`  
- [ ] Multipart enroll updates gallery_version  
- [ ] Graceful shutdown does not corrupt SQLite  
- [ ] No process-global mutable state  

## 14. Source map

| Python | Rust |
|---|---|
| `main.py` | `pksp-cli` + `pksp-api::serve` |
| `auth.py` | `auth.rs` |
| `routers/*` | `routes/*` |
| `ws/hub.py` | `hub.rs` |
| `config.py` | `settings` in core or api |
