# 09 — Media Plane

## 1. Why media is a first-class crate

From `camera_issue_fix.md` and architecture:

- Browsers cannot play RTSP natively.
- Browser WebRTC does **not** negotiate H.265 in standard clients (2026).
- Camera `10.39.45.167` stream1 is **H.265 2560×1440**.
- Without a codec adapter, WHEP returns **400** (`codecs not supported by client`).

Python today: **MediaMTX** (external) + **ad-hoc GStreamer transcoder** → path `cam_in_h264`.

Rust goal: **owned, supervised, restartable** media — no manual `/tmp` scripts.

## 2. Responsibilities of `pksp-media`

| Responsibility | Notes |
|---|---|
| Path registry | cam_in, cam_in_h264, demo, cam_out |
| RTSP pull or accept publish | from IP camera or demo ffmpeg |
| Codec detection | H264 vs H265 |
| Optional transcode | H265→H264 low-latency |
| Browser egress | WHEP primary, HLS fallback |
| Frame tap | optional decoded frames to vision FrameBus |
| Health | path online, bitrate, last error |

## 3. Backend strategies

### Strategy A — MediaMTX child (M2 parity) **recommended first**

```
pksp-media:
  - write/generate mediamtx.yml from Settings
  - spawn mediamtx binary (bundled or PATH)
  - spawn/restart GStreamer transcoder when needed
  - expose local WHEP base http://127.0.0.1:8889
```

**Pros:** Frontend works unchanged; battle-tested ICE/WebRTC.  
**Cons:** Still a second binary on disk (but supervised).

Bundle options:

- Download MediaMTX at build/install time  
- Document system package  
- `tokio::process::Command` with restart backoff  

### Strategy B — GStreamer in-process (M4+)

```
rtspsrc ! decode ! tee name=t
  t. ! queue ! videoconvert ! appsink          # vision frames
  t. ! queue ! x264enc zerolatency ! whepserversink  # browser
```

Use gst-plugins-rs WHEP elements (`whepserversink`, webrtchttp).

**Pros:** True single process for media+vision.  
**Cons:** Plugin install matrix (Linux/Mac), ICE host candidates config, more code.

### Strategy C — Pure webrtc-rs WHEP server

**Rejected for v1** — high effort, high ICE footguns.

### Strategy D — Reimplement MediaMTX

**Rejected.**

## 4. Codec policy

```
if camera advertises H264 (or we configure H264 URL):
    webrtc_path = native path
    no transcoder
else if H265:
    start transcoder → publish H264 path
    cameras.webrtc_path = that path
    health API returns H264 path
else:
    error + HLS attempt
```

**Preferred ops fix:** reconfigure physical camera sub-stream to H.264 (document in README). Transcoder is fallback, not badge of honor.

## 5. Reference transcoder (working)

From camera fix report:

```bash
gst-launch-1.0 \
  rtspsrc location=... protocols=tcp latency=200 ! \
  decodebin ! videoconvert ! \
  x264enc tune=zerolatency speed-preset=ultrafast bitrate=1800 ! \
  flvmux streamable=true ! \
  rtmpsink location=rtmp://127.0.0.1/cam_in_h264
```

MediaMTX path:

```yaml
cam_in:
  source: rtsp://...
  rtspTransport: tcp
cam_in_h264:
  source: publisher
```

Rust should embed this as a structured pipeline description + auto-restart loop (not a hand-maintained shell script).

## 6. Ports (parity defaults)

| Port | Service |
|---|---|
| 8554 | RTSP |
| 1935 | RTMP (publish) |
| 8888 | HLS |
| 8889 | WebRTC/WHEP |
| 9997 | MediaMTX API (optional health) |

Frontend:

- `NEXT_PUBLIC_WEBRTC_BASE=http://localhost:8889`
- HLS: same host port 8888 `/{path}/index.m3u8`

## 7. Demo path

Keep `demo` publisher path + `scripts/demo_rtsp.sh` (ffmpeg) for no-camera dev.  
`pksp-media` can also run a test pattern source via GStreamer `videotestsrc` when `MOCK_VISION`/demo mode.

## 8. Interaction with vision

### Ideal

```
Camera → media decode tee → vision FrameBus
                         → H264 encode → WHEP
```

### Acceptable interim

```
Camera → MediaMTX (browser)
Camera → vision RTSP client (second pull)
```

Document CPU cost of dual pull; migrate to tee when GStreamer path lands.

## 9. Faster / simpler / cleaner

| Opportunity | Detail |
|---|---|
| Supervise transcoder | no human restarts |
| Auto-select H264 URL | if settings provide stream2 H264 |
| TCP RTSP | avoid UDP MTU issues (already learned) |
| Single path name in DB | always browser-compatible path in `webrtc_path` |
| Health: codec field | `"video_codec": "h264_transcoded"` for UI later |
| Drop dual pull | biggest structural win |

## 10. Security

- RTSP credentials in env only; never log full URLs  
- WHEP bound to LAN  
- Same trust model as Python (no public hardening)

## 11. Acceptance criteria

- [ ] Browser WHEP works for H264 sources without transcoder  
- [ ] H265 sources auto-transcode or clear error  
- [ ] `webrtc_path` in health always browser-safe  
- [ ] MediaMTX or GStreamer supervised; crash recovery  
- [ ] HLS fallback still possible  
- [ ] No dependency on `/tmp/start-h264-transcode.sh`  

## 12. Source map

| Current | Rust |
|---|---|
| `configs/mediamtx.yml` | generated or template in `pksp-media` |
| `docker-compose.yml` mediamtx | optional; prefer child process |
| camera fix GStreamer script | `pksp-media` transcoder module |
| frontend whep.ts | unchanged consumer |

## 13. Research references (compiled)

- MediaMTX: RTSP/WebRTC/HLS proxy (MIT), WHEP paths `/{name}/whep`
- Browser WebRTC codecs: H264/VP8/VP9/AV1 — not HEVC in standard pipeline
- gst-plugins-rs: `whepserversink`, `whepsrc`, WHIP elements
- retina: pure-Rust RTSP client (NVR ecosystem)
- Project incident: WHEP 400 = codec mismatch, not client bug
