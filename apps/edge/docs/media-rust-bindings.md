# Media stack: bundled binaries vs Rust bindings

## What we ship today (recommended for PKSP)

| Component | Approach | Why |
|---|---|---|
| **MediaMTX** | **Vendored single binary** under `apps/edge/bin/mediamtx`, supervised by `pksp serve` | Zero-dep media server; RTSP/RTMP/HLS/WHEP; no pure-Rust MediaMTX library exists |
| **Transcode H.265→H.264** | **Vendored/local `ffmpeg` CLI** supervised as child (not GStreamer) | Single process to spawn; same pipeline as proven camera fix |
| **GStreamer** | **Not required** for default path | Full plugin stack; hard to “download one binary” |

There is **no official Rust crate that embeds MediaMTX**. The correct Rust integration is: **spawn + supervise the official binary** (what `pksp-media` does).

---

## Best Rust bindings / crates (research 2026)

### If you want in-process media later

| Crate | Role | Fit for PKSP | Notes |
|---|---|---|---|
| **`gstreamer` / `gstreamer-app` / `gstreamer-rtsp`** ([gstreamer-rs](https://gitlab.freedesktop.org/gstreamer/gstreamer-rs)) | Safe GStreamer bindings | **Best** for in-process RTSP decode + appsink frames + optional WHEP via **gst-plugins-rs** | Requires system GStreamer + plugins; not a single binary |
| **gst-plugins-rs** (`whepserversink`, `whip*`, `webrtcsink`) | WHIP/WHEP elements in GStreamer | Ideal if dropping MediaMTX long-term | Plugin install matrix (Linux/macOS) |
| **`ffmpeg-next`** | Safe FFmpeg libav* bindings | Decode/transcode **in-process** without CLI | Maintenance-mode; complex build; needs FFmpeg dev libs |
| **`ffmpeg-dev` / static ffmpeg crates** | Bundled static FFmpeg FFI | Portable builds | Larger compile; license GPL considerations (x264) |
| **`webrtc` (webrtc-rs)** | Pure Rust WebRTC | Custom WHEP server | High effort (ICE/DTLS/SDP); reimplements MediaMTX surface |
| **`wrtc`** | DX wrapper over webrtc-rs | Same | Still no RTSP SFU |
| **`retina`** | Pure Rust **RTSP client** | Vision pull without OpenCV/FFmpeg CLI | Decode still needs ffmpeg-next or GStreamer |
| **`srt_whep`** | SRT → WHEP app/lib | Adjacent, not RTSP camera | Uses GStreamer under the hood often |

### Explicitly **not** best for us now

| Option | Why reject for v1 |
|---|---|
| Reimplement MediaMTX in Rust | Multi-year protocol surface |
| PyO3 + Python GStreamer | Defeats Rust edge goal |
| Depend only on system `mediamtx` on PATH | Fragile ops; we vendor under `bin/` |

---

## Recommended roadmap

1. **Now (done direction):** supervise **bundled MediaMTX + ffmpeg** — simplest, matches industry pattern (FFmpeg + MediaMTX → browser WebRTC).
2. **Later (optional):** `retina` + `ffmpeg-next` for **vision frames only** (no second RTSP pull).
3. **Later (optional):** `gstreamer-rs` + `whepserversink` to drop MediaMTX child if ops want single process.

---

## Commands

```bash
# Download / refresh binaries into apps/edge/bin/
./apps/edge/scripts/download-binaries.sh

# pksp auto-finds: env → apps/edge/bin → PATH
pksp serve
```
