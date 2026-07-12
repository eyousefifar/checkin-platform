# 02 — Target Architecture

## 1. Design principles

1. **One product binary** (`pksp serve`) owns control + vision; media is either in-process (GStreamer) or a supervised child (MediaMTX), never a manual operator step.
2. **Pure core, impure edges** — algorithms in `pksp-core` have zero I/O; they are unit-tested and free of async.
3. **Latest-frame discipline** — under load, drop stale frames; never build an unbounded queue.
4. **Contract stability** — HTTP/WS JSON matches existing frontend (`05`).
5. **Smarter by policy** — intelligence is zones + time + quality, not a heavier black-box model by default.
6. **Prefer fewer pulls of the camera** — one decode path fans out to vision and browser where practical.

## 2. High-level architecture

```
                         ┌─────────────────────────────┐
                         │        IP Camera(s)         │
                         │   RTSP H.265 and/or H.264   │
                         └─────────────┬───────────────┘
                                       │
                    ┌──────────────────┼──────────────────┐
                    │                  │                  │
                    ▼                  ▼                  │
           ┌────────────────┐  ┌──────────────┐          │
           │  pksp-media    │  │ (optional)   │          │
           │  RTSP client / │  │ MediaMTX     │◄─────────┘
           │  GStreamer     │  │ supervised   │
           │  transcode     │  └──────┬───────┘
           │  WHEP / HLS    │         │ WHEP
           └────────┬───────┘         │
                    │ frames          ▼
                    │          Browser <video>
                    ▼
           ┌────────────────┐
           │ FrameBus       │  watch/ring: Arc<Frame>
           │ (latest only)  │
           └────────┬───────┘
                    │
                    ▼
           ┌────────────────┐     ┌─────────────────┐
           │ pksp-vision    │────▶│ pksp-core       │
           │ worker pool    │     │ quality/track/  │
           │ ort sessions   │     │ vote/fsm/zones  │
           │ gallery RwLock │     └────────┬────────┘
           └────────┬───────┘              │
                    │ commit               │
                    ▼                      │
           ┌────────────────┐              │
           │ pksp-db        │◄─────────────┘
           │ sqlx pool      │
           └────────┬───────┘
                    │
           ┌────────▼───────┐     broadcast channel
           │ pksp-api       │──────────────────────▶ WS clients
           │ axum REST      │
           └────────────────┘
```

## 3. Cargo workspace layout

```
apps/edge/
  Cargo.toml
  crates/
    pksp-core/          # pure
    pksp-db/            # sqlx + migrations + seed
    pksp-vision/        # FaceEngine, gallery, worker, enroll helpers
    pksp-media/         # MediaPlane trait + GStreamer/MediaMTX backends
    pksp-api/           # routes, auth, hub, AppState
    pksp-cli/           # main.rs: serve | migrate | seed
  migrations/           # sqlx SQL files (or under pksp-db)
  configs/              # optional mediamtx.yml template
```

**Repo placement:** `apps/edge/` mirrors `apps/api` / `apps/web`. Alternative: top-level `crates/` — prefer `apps/edge` for consistency with monorepo layout.

## 4. Process models (phased)

### Phase M2–M3 (parity-first)

| Component | How it runs |
|---|---|
| `pksp serve` | API + vision + enroll |
| MediaMTX | Child process spawned by `pksp-media` **or** external docker (feature flag) |
| Transcoder | GStreamer pipeline owned by `pksp-media` when source is H.265 |

### Phase M4+ (unified media)

| Component | How it runs |
|---|---|
| `pksp serve` | API + vision + **in-process** GStreamer WHEP/HLS where viable |
| MediaMTX | Optional fallback backend only |

**Rejected:** reimplementing full MediaMTX protocol matrix in pure Rust for v1.

## 5. Frame bus

### Requirements

- Multiple cameras (`cam_in`, `cam_out`)
- Capture task always writes **latest** frame (overwrite)
- Process task never blocks capture for more than brief lock
- Frame metadata: `camera_id`, `captured_at`, `width`, `height`, `pixel_format` (BGR8 preferred for model path)
- Vision may downscale before det (`long edge` / fixed det_size)

### Recommended primitive

```rust
// Conceptual
struct Frame {
    camera_id: CameraId,
    captured_at: Instant,
    width: u32,
    height: u32,
    // BGR interleaved, owned
    data: bytes::Bytes, // or Arc<Vec<u8>>
}

type FrameWatch = tokio::sync::watch::Sender<Option<Arc<Frame>>>;
```

`watch` gives natural “latest only” semantics. Alternatives: `tokio::sync::Mutex<Option<Arc<Frame>>>` (simpler, fine for 2 cams).

### Improvement vs Python

Python uses a custom `LatestFrameBuffer` + threading lock. Rust version is clearer and avoids GIL contention between capture and infer if capture is async/blocking pool and infer is dedicated.

## 6. Control plane concurrency model

```
tokio runtime (multi-thread)
├── axum server (HTTP + WS)
├── per-camera capture tasks (blocking via spawn_blocking or dedicated threads)
├── per-camera process tasks (blocking ONNX in spawn_blocking)
├── media supervisor task (child MediaMTX / GStreamer bus)
└── optional metrics tick task
```

**Inference lock:** keep a global or per-device semaphore (Python used one `threading.Lock` for all cams). Prefer **one ONNX session per model** + semaphore for max concurrent infer = 1 on CPU.

**Attendance DB writes:** channel `mpsc` of `CommitRequest` handled by a single writer task (serialize SQLite writes cleanly).

## 7. Application state

Replace Python globals:

```rust
// Conceptual AppState
struct AppState {
    settings: Arc<Settings>,
    db: sqlx::SqlitePool,
    gallery: Arc<RwLock<Gallery>>,
    hub: LiveHub,                    // broadcast::Sender<WsEvent>
    vision: VisionHandle,            // control: reload, metrics
    media: MediaHandle,              // paths, online status
    face_engine: Arc<dyn FaceEngine>, // or enum Mock | Ort
}
```

No process-global mutable `hub` / `_gallery` / `_worker`.

## 8. Failure domains

| Domain | Failure mode | Behavior |
|---|---|---|
| Camera RTSP down | reconnect loop with backoff | `camera_status.online=false`; HUD empty |
| ONNX load fail | start with `vision_ready=false` | API up; mock optional; enroll may fail closed |
| MediaMTX child exit | supervisor restart | WHEP errors until back; vision can continue if own RTSP |
| SQLite locked | retry / single writer | no multi-process writers |
| WS client flood | per-client send fail → drop client | hub continues |
| Transcode fail | restart pipeline; fall back HLS if available | log + metrics |

## 9. Security model (parity)

- Trusted LAN MVP
- Single admin password → JWT HS256
- CORS from config
- No public internet hardening required for port parity
- Biometrics on disk under `data/` — same privacy posture as Python (`plans/09`)

## 10. Faster / simpler / cleaner (architecture-level)

| Area | Python pain | Rust approach |
|---|---|---|
| Globals | hub/gallery/worker singletons | `AppState` |
| Thread → async WS | `run_coroutine_threadsafe` | `broadcast` channel from any task |
| Dual camera pull | MediaMTX + OpenCV | single ingest fan-out (goal) |
| create_all schema | no migrations | sqlx migrations |
| Seed cameras | if empty only | upsert from env |
| Session per frame | many short sessions | pool + commit queue |
| Config | pydantic + lru_cache | once-loaded `Arc<Settings>` (+ optional hot-reload later) |

## 11. Feature flags (suggested Cargo features)

| Feature | Default | Purpose |
|---|---|---|
| `media-mediamtx` | on | spawn/supervise MediaMTX binary |
| `media-gstreamer` | on for Linux | in-process pipelines |
| `vision-ort` | on | real ONNX |
| `vision-mock` | on | CI / theater |
| `openvino` | off | EP |
| `coreml` | off | Apple EP |

## 12. Acceptance criteria

- [ ] Architecture supports single-binary mental model
- [ ] Frame bus latest-only semantics specified
- [ ] Crate boundaries clear; pure core has no sqlx/ort
- [ ] Media backend strategy phased (MediaMTX → GStreamer)
- [ ] AppState replaces all Python globals
- [ ] Failure domains documented

## 13. Source mapping

| Concept | Python | Rust |
|---|---|---|
| App lifespan | `main.lifespan` | `pksp-cli serve` |
| Latest frame | `LatestFrameBuffer` | `FrameBus` / watch |
| Hub | `LiveHub` | `tokio::sync::broadcast` |
| Gallery | `GalleryService` singleton | `Arc<RwLock<Gallery>>` in state |
| Worker threads | `threading.Thread` | tokio tasks + `spawn_blocking` |
