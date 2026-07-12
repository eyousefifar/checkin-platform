# 03 — Crate Matrix & Research

Research compiled 2026-07. Prefer **boring, well-maintained** crates over novel stacks.

## 1. Summary matrix

| Concern | **Primary** | Alternatives | Decision |
|---|---|---|---|
| Runtime | `tokio` | smol, async-std | Primary: ecosystem default |
| HTTP + middleware | `axum` + `tower` + `tower-http` | actix-web, poem, rocket | Axum owns 2025–26 Rust web |
| WebSocket | `axum::extract::ws` | tokio-tungstenite direct | Use axum integration |
| Config | `serde` + env via `figment` **or** `config` | clap-only, envy | Figment: layered env + file like pydantic-settings |
| CLI | `clap` (derive) | argh | `pksp serve/migrate` |
| DB | `sqlx` (sqlite, runtime-tokio) | sea-orm, diesel, rusqlite | sqlx: async + migrations |
| JWT | `jsonwebtoken` | josekit | HS256 parity |
| JSON | `serde_json` | — | — |
| Time | `chrono` + `chrono-tz` | `time` | local_date / TZ |
| Errors | `thiserror` (libs) + `anyhow` (bin) | eyre, snafu | Split by crate role |
| Logging | `tracing` + `tracing-subscriber` | log + env_logger | Structured spans for FPS |
| Arrays | `ndarray` | nalgebra, faer | Cosine gallery matmul |
| Image I/O | `image` | opencv | Enroll decode/encode JPEG/PNG |
| UUID | `uuid` (v4) | — | Enroll filenames |
| Multipart | axum multipart | multer | Enroll uploads |
| Bytes | `bytes` | — | Frame buffers |
| ONNX | `ort` (pykeio) | tract, candle | EP coverage + production use |
| Face high-level | custom ort **or** evaluate `face_id` | rusty_scrfd, rust-faces | See §4 |
| RTSP client | `retina` | ffmpeg-only, gstreamer rtspsrc | Pure-Rust client used in NVR space |
| Decode/transcode | `gstreamer` / gstreamer-rs | ffmpeg-next | GStreamer for WHEP + x264 path |
| WebRTC serve | GStreamer `whepserversink` / webrtchttp | webrtc-rs DIY, MediaMTX child | Prefer not DIY SFU |
| Process spawn | `tokio::process` | std::process | Supervise MediaMTX |
| Rand (mock) | `rand` | — | Mock embeddings |
| Test | cargo test, `proptest`, `tokio::test` | — | — |
| Bench | `criterion` | — | match + pipeline |

## 2. Deep research notes

### 2.1 Web stack (Axum)

- Industry default with Tokio/Tower in 2025–2026 surveys and community consensus.
- Compose CORS, trace, compression via tower-http.
- WebSocket and multipart are first-class enough for this app’s surface.
- **Rejected Actix-web:** still fast, but ecosystem gravity and tower integration favor Axum for greenfield.

### 2.2 Database (sqlx)

- Compile-time checked queries optional (`SQLX_OFFLINE`); for SQLite MVP, runtime queries acceptable.
- Built-in migrations (`sqlx migrate`) beat SQLAlchemy `create_all`.
- **SeaORM:** nicer relations; adds ORM weight — optional later if CRUD boilerplate hurts.
- **Diesel:** sync-first historically; less natural for axum async handlers.
- **rusqlite:** fine for embedded tools; weaker multi-connection async story.

SQLite settings for multi-task access:

```
PRAGMA journal_mode=WAL;
PRAGMA busy_timeout=5000;
PRAGMA foreign_keys=ON;
```

### 2.3 ONNX (`ort`)

- `ort` (https://github.com/pykeio/ort) is the maintained ONNX Runtime binding for Rust.
- Supports CPU, CUDA, CoreML, OpenVINO, etc. via execution providers.
- Used in production-style pipelines (e.g. real-time face/ONNX posts 2026).
- **tract:** pure Rust; good for constrained embeds; weaker for SCRFD/ArcFace + EP.
- **candle/burn:** great for training/research; not needed when weights are ONNX.

Threading: configure intra/inter op threads (mirror `ONNX_INTRA_OP_NUM_THREADS`).

### 2.4 Face recognition in Rust

| Option | Pros | Cons |
|---|---|---|
| **Custom SCRFD + ArcFace via ort** | Full control, match buffalo_l files, landmarks for pose | Must implement alignment |
| **`face_id` crate** | Already wraps SCRFD + ArcFace + alignment; buffalo_l paths | Dependency risk; API may not fit zones/quality |
| **`rusty_scrfd`** | Detection only | Still need recognition + align |
| **`rust-faces`** | Det models | Older / detection-focused |
| **PyO3 InsightFace** | Exact parity | Keeps Python; reject |

**Recommendation:** Start with **custom ort pipeline** (or thin wrap of `face_id` after spike). Spike milestone M1 should decide in ≤2 days:

Decision criteria for adopting `face_id`:

1. Landmarks available for pose gate  
2. Embedding dim 512 L2-normalized compatible with existing DB blobs  
3. EP configuration exposed  
4. License compatible (MIT/Apache)  
5. Active enough / vendored models clear  

If any fail → custom.

### 2.5 buffalo_l ONNX file layout (InsightFace)

Typical pack (names may vary by download):

| File | Role |
|---|---|
| `det_10g.onnx` (SCRFD) | Detection + 5 landmarks (approx) |
| `w600k_r50.onnx` | ArcFace recognition 512-d |
| `genderage.onnx` | **Do not load** for MVP (CPU waste) |

Alignment: 5-point similarity transform to template (InsightFace standard). Implement with `ndarray` + small linear algebra (or `nalgebra` for 2×3 affine).

### 2.6 RTSP

| Crate | Role |
|---|---|
| **retina** (scottlamb) | High-level pure-Rust RTSP client; used by Moonfire NVR / Savant Retina service narratives |
| **ffmpeg-next** | Decode annex-B H.264/H.265 to raw frames |
| **gstreamer** | `rtspsrc` → decodebin → appsink for BGR |

**Recommendation:**

- **Vision path:** GStreamer appsink **or** retina + ffmpeg-next decode — pick one per platform in spike.
- Prefer **GStreamer on Linux** (already used for transcoder; VAAPI possible).
- Prefer **retina + ffmpeg** if minimizing GStreamer dependency on Mac demo.

### 2.7 WebRTC / WHEP for browsers

| Approach | Effort | Risk |
|---|---|---|
| **A. Supervise MediaMTX** (child binary) | Low | Proven; still second binary on disk |
| **B. GStreamer whepserversink / webrtchttp** | Medium | Native embed; plugin install complexity |
| **C. webrtc-rs + custom WHEP** | High | ICE/DTLS/SDP edge cases |
| **D. Full MediaMTX reimplementation** | Extreme | Reject |

**Phase strategy:** A for parity → B when stable. Never D.

gst-plugins-rs includes: `whepserversink`, `whepsrc`, `whip*`, `webrtcsink` — research confirms first-class WHIP/WHEP elements exist in the GStreamer Rust plugin set.

### 2.8 Transcode H.265 → H.264

Mirror working pipeline from `camera_issue_fix.md`:

```
rtspsrc protocols=tcp ! decodebin ! videoconvert !
  x264enc tune=zerolatency speed-preset=ultrafast bitrate=1800 !
  ... publish to WHEP path or RTMP/MediaMTX
```

In Rust: build pipeline via gstreamer-rs; supervise bus messages; auto-restart.

**Preferred zero-CPU path:** camera outputs H.264 sub-stream → no transcoder.

## 3. License matrix

| Component | License | Commercial note |
|---|---|---|
| Axum, Tokio, sqlx, serde, ort (crate) | MIT/Apache-2.0 | OK |
| ONNX Runtime | MIT | OK |
| MediaMTX | MIT | OK |
| GStreamer | LGPL-2.1 | Dynamic link typical; respect LGPL |
| x264 | GPL | If linked, may force GPL obligations — **document**; prefer system plugin dynamic |
| buffalo_l **weights** | Non-commercial research | **Banner required**; license for prod |
| OpenCV (if used) | Apache-2.0 | OK |
| Next.js | MIT | OK |

**Action:** Keep `LicenseBanner` in UI. Prefer dynamic linking for GPL-ish media plugins. Document redistribution story in deploy guide.

## 4. Explicit rejects (with reasons)

| Rejected | Reason |
|---|---|
| FAISS | N≪50; ndarray matmul enough |
| Postgres / pgvector | Ops overkill for on-prem demo |
| Redis | Single-node broadcast channel enough |
| Actix-web | Ecosystem gravity |
| Diesel | Async ergonomics |
| Candle for buffalo_l | ONNX already canonical |
| PyO3 InsightFace | Python dependency |
| YOLO person detector (v1) | Add only if multi-person false faces force it |
| Full MediaMTX rewrite | Scope explosion |
| Electron | Web on LAN enough |

## 5. Version guidance (pin at scaffold time)

Do not pin here blindly — resolve at `cargo init` with current crates.io:

| Crate | Guidance |
|---|---|
| rustc | stable 1.83+ (or current stable) |
| tokio | 1.x full features |
| axum | 0.7 or 0.8 line current |
| sqlx | 0.8.x sqlite + runtime-tokio + migrate |
| ort | 2.x RC/stable line current in 2026 |
| gstreamer | match system GStreamer 1.22+ |
| ndarray | 0.16.x |
| jsonwebtoken | 9.x |
| chrono | 0.4 + clock |

## 6. Suggested `Cargo.toml` workspace sketch

```toml
[workspace]
resolver = "2"
members = [
  "crates/pksp-core",
  "crates/pksp-db",
  "crates/pksp-vision",
  "crates/pksp-media",
  "crates/pksp-api",
  "crates/pksp-cli",
]

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
axum = { version = "0.8", features = ["multipart", "ws"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "chrono", "migrate"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tracing = "0.1"
ndarray = "0.16"
chrono = { version = "0.4", features = ["serde"] }
# ...
```

Exact versions fixed at implementation.

## 7. Faster / simpler / cleaner via crate choices

| Choice | Benefit |
|---|---|
| sqlx migrations | Explicit schema history vs create_all |
| broadcast channel | Cleaner than thread-safe asyncio bridge |
| ort EPs | Same models, better hardware use |
| GStreamer single graph | Transcode + WHEP without shell scripts |
| pure pksp-core | Instant tests; no mock I/O |
| clap subcommands | `migrate` / `serve` / `seed` ops clarity |

## 8. Spike checklist (before locking media/vision crates)

1. Load `det_10g.onnx` + `w600k_r50.onnx` with ort on target hardware; measure FPS at 640 det.
2. Decode one RTSP frame (retina+ffmpeg **or** gstreamer appsink).
3. MediaMTX child WHEP still works from existing frontend.
4. Optional: 1-day `face_id` evaluation.

## 9. Acceptance criteria

- [x] Primary crate per concern selected
- [x] Rejects documented
- [x] License risks called out (buffalo_l, x264/GPL)
- [x] Spike criteria for vision/media crates defined
- [x] Workspace dependency sketch provided
