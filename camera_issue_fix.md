# Camera Video / WHEP Issue Fix Report

**Date:** 2026-07-12  
**Project:** PKSP Check-In Platform (checkin-platform)  
**Issue Type:** Live camera video not displaying in dashboard (WHEP/WebRTC failure)  
**Symptoms:** "Vision online · awaiting WebRTC", `http://localhost:8889/cam_in`, "WHEP · WHEP 400 Bad Request" (or NetworkError)

## 1. Issue Description

The main operations dashboard (`/`) displays camera tiles with two layers:
- **Video background**: Delivered via MediaMTX using the WHEP protocol (WebRTC over HTTP) from an RTSP camera feed.
- **Canvas HUD overlay**: Face detections, tracks, and labels delivered independently via WebSocket from the vision pipeline.

The HUD (synthetic or real detections) was working correctly via WS, but the video element remained in a placeholder state:

```
Vision online · awaiting WebRTC
http://localhost:8889/cam_in
WHEP · WHEP 400 Bad Request
Canvas HUD from WS · video via MediaMTX when stream is live
```

This occurred even after MediaMTX was running and successfully ingesting the camera.

## 2. Symptoms Observed

- Tile showed "awaiting WebRTC" or raw error messages.
- Browser DevTools / console showed failed WHEP POSTs returning 400.
- Manual curl test to the WHEP endpoint also returned 400.
- MediaMTX logs showed successful RTSP pull but failed WebRTC sessions.
- Previous states included "NetworkError when attempting to fetch resource" (before MediaMTX was started) and path mismatches (`/cam_in` vs `/demo`).

The system was partially functional (vision, WS, API, attendance, enrollment), but the "live video" part of the "Live operations" dashboard was broken.

## 3. Root Cause Analysis

### Primary Cause: Codec Incompatibility (H.265 / HEVC)

The IP camera (`10.39.45.167`) was delivering its streams using **H.265 (HEVC)**:

- Confirmed via OpenCV probe: `FOURCC: hevc`
- MediaMTX source log: `[path cam_in] [RTSP source] ready: 2 tracks (H265, G711)`
- Stream1 (high quality 2560x1440) and Stream2 (sub) were both HEVC.

**Why this breaks WHEP:**

- The frontend uses native `RTCPeerConnection` + WHEP (POST SDP offer to `/cam_in/whep`).
- Browser WebRTC implementations (Chrome, Firefox, Edge, etc.) do **not** support H.265 decoding in the standard WebRTC pipeline (as of 2026).
- Supported video codecs in WebRTC offers are typically: H.264 (AVC), VP8, VP9, AV1.
- When the browser sent its SDP offer, MediaMTX found **no overlapping video codec** with the ingested H.265 track.
- MediaMTX rejected the session: `closed: codecs not supported by client`
- The WHEP HTTP response was **400 Bad Request**.
- In `CameraTile.tsx` / `whep.ts`, this was turned into the visible error.

### Contributing Factors (Encountered During Debugging)

1. **MediaMTX not running at all** (initial state)
   - Docker CLI was unavailable in the environment (`docker: command not found`).
   - Standalone binary had to be downloaded and run from `/tmp/mediamtx-standalone`.

2. **Stale database configuration**
   - `data/pksp.db` contained `webrtc_path = 'cam_in'` and old RTSP URL pointing at the real camera.
   - Current `.env` had `CAM_IN_WEBRTC_PATH=demo`.
   - `seed_cameras()` only runs on first boot (if no cameras exist), so updates to `.env` had no effect until manual DB fix.
   - Frontend was (initially) hardcoding `webrtcPath="cam_in"` in `page.tsx`.

3. **RTP packet size issues**
   - High-resolution H.265 produced packets too large for UDP.
   - Fixed by setting `rtspTransport: tcp` in the path config.

4. **Path mismatch between demo and real setups**
   - `mediamtx.yml` defines `demo` as `source: publisher` and `cam_in` as RTSP pull.
   - `scripts/demo_rtsp.sh` publishes to `demo`.
   - Hardcoded frontend + stale DB led to requests going to the wrong path.

5. **No transcoding pipeline**
   - Direct H.265 → browser WebRTC is impossible without either:
     - Camera outputting H.264 on a stream, **or**
     - On-the-fly transcoding.

6. **HLS as partial fallback**
   - HLS (`http://localhost:8888/.../index.m3u8`) sometimes worked (200), but was not automatically used.
   - H.265 HLS has spotty browser support anyway.

## 4. Diagnostic Steps Performed

- Inspected running processes, ports, `.env`, and DB state.
- Read MediaMTX logs in real time (`tail -f /tmp/mediamtx.log`).
- Used Python + OpenCV (from the API venv) to:
  - Confirm camera reachability and FOURCC.
  - Probe multiple streams (`stream1`, `stream2`, etc.).
- Manual WHEP tests with `curl` using various SDP bodies.
- Inspected frontend code:
  - `apps/web/src/lib/whep.ts` (SDP offer creation, `addTransceiver`, fetch).
  - `apps/web/src/components/CameraTile.tsx` (useEffect, error display, path resolution).
  - `apps/web/src/app/page.tsx` (hardcoded prop).
- Checked GStreamer availability (had `x264enc`, `flvmux`, `rtmpsink`, `rtspsrc`, `decodebin` — no `rtspclientsink`).
- Verified flatpak ffmpeg existed but was unusable standalone (missing shared libs, stack smashing when LD_LIBRARY_PATH forced).
- Used MediaMTX API (`/v3/paths/list`) and HLS endpoint testing.
- Confirmed successful RTMP publish vs. WebRTC reader sessions in logs.

## 5. Fix Implementation

### Step 1: Make `webrtc_path` dynamic (prevent future mismatches)

In `apps/web/src/app/page.tsx`:
- Fetch `/api/health` (public endpoint) on mount.
- Extract `cameras[].webrtc_path` for `cam_in`.
- Pass it as prop to `<CameraTile webrtcPath={...} />`.
- Default to `"demo"` if fetch fails.

This replaced the hardcoded `webrtcPath="cam_in"`.

### Step 2: Update MediaMTX configuration

Created/updated `/tmp/mediamtx-standalone/mediamtx.yml`:

```yaml
paths:
  cam_in:
    source: rtsp://admin:campkspQq123@10.39.45.167:554/stream1
    rtspTransport: tcp

  cam_in_h264:
    source: publisher
```

- Kept original high-quality H.265 path (for vision if `MOCK_VISION=false` later).
- Added dedicated publisher path for browser-compatible stream.
- Used TCP transport to eliminate UDP packet size warnings.

Standalone MediaMTX was used (no Docker).

### Step 3: Real-time H.265 → H.264 transcoding + RTMP publish

Wrote `/tmp/start-h264-transcode.sh` (resilient loop):

```bash
while true; do
  gst-launch-1.0 -v \
    rtspsrc location=... protocols=tcp latency=200 ! \
    decodebin ! videoconvert ! \
    x264enc tune=zerolatency speed-preset=ultrafast bitrate=1800 ! \
    flvmux streamable=true ! \
    rtmpsink location=rtmp://127.0.0.1/cam_in_h264
  sleep 5
done
```

- Uses available GStreamer elements (no external ffmpeg binary required).
- Publishes H.264 via RTMP (MediaMTX port 1935) to the new path.
- Wrapped in a while loop for automatic recovery.

Launched in background; logs to `/tmp/gst-h264-transcode.log`.

### Step 4: Update database to use the new path

```sql
UPDATE cameras
SET webrtc_path = 'cam_in_h264',
    rtsp_url = 'rtsp://admin:campkspQq123@10.39.45.167:554/stream1'
WHERE id = 'cam_in';
```

This made `/api/health` and camera APIs return the correct value.

### Step 5: Restart all services cleanly

- Killed old processes (API, Next.js, previous MediaMTX instances, transcoders).
- Started:
  - MediaMTX (standalone)
  - Transcoder script
  - FastAPI (`uvicorn`, using user-site packages)
  - Next.js (`npm run dev`)
- Verified:
  - `ss -tlnp` showed all ports (3000, 8000, 1935, 8554, 8888, 8889).
  - API health reported `webrtc_path: cam_in_h264`.
  - MediaMTX log: `is publishing to path 'cam_in_h264', 1 track (H264)`
  - Later: `is reading from path 'cam_in_h264', 1 track (H264)` (successful WebRTC session).

### Step 6: Frontend resilience (HLS fallback)

In `CameraTile.tsx` (previous + incremental changes):

- Compute `hlsUrl` from `webrtcBase`.
- On WHEP 400 (or codec errors), automatically switch:
  - `setUseHlsFallback(true)`
  - `video.src = hlsUrl; video.play()`
- Display "HLS fallback" badge.
- Improved error messages:
  - Network/fetch → actionable "MediaMTX unreachable..."
  - 400 → "WHEP 400 (codec mismatch — H265 source)"
- `showVideo` now considers fallback.
- Cleanup handles both `srcObject` (WHEP) and `src` (HLS).

This ensures the tile can still show video even if WHEP fails for other reasons.

## 6. Verification

- MediaMTX logs confirm H.264 publish and active WebRTC reader.
- API `/health` and cameras endpoint return `cam_in_h264`.
- WHEP test to `/cam_in_h264/whep` no longer fails for codec reasons (manual incomplete SDP still 400, but real browser offers succeed).
- After hard refresh of `http://localhost:3000`, the video tile should display the live (transcoded) feed.
- Canvas HUD continues to overlay detections from the WebSocket pipeline.
- Transcoder is resilient (auto-restarts on failure).
- Fallback logic is present as a safety net.

## 7. Files Changed / Created

- `apps/web/src/app/page.tsx` — dynamic `webrtcPath` from health.
- `apps/web/src/components/CameraTile.tsx` — 400 handling + HLS fallback, states, hlsUrl.
- `/tmp/mediamtx-standalone/mediamtx.yml` — added `cam_in_h264` publisher path + tcp.
- DB (`data/pksp.db`) — updated `webrtc_path`.
- `/tmp/start-h264-transcode.sh` + running process — transcoder.
- (Temporary) various logs in `/tmp/`.

No permanent source changes to backend vision code were needed (mock mode was active).

## 8. Recommendations for Long-Term / Production

- **Preferred**: Reconfigure the physical camera (via its web UI) to output **H.264** on at least one stream (preferably the sub-stream for lower CPU). Point MediaMTX at that stream directly — no transcoding overhead.
- Add `CAM_IN_WEBRTC_PATH` consistently in `.env` and ensure DB reseed on change (or make `seed_cameras` update existing rows).
- Consider adding a small status banner or settings page showing "Video codec: H264 (transcoded)" vs "native".
- Monitor CPU of the transcoder (GStreamer x264enc is reasonably efficient with `ultrafast` preset).
- For true zero-transcode WebRTC, camera H.264 output is ideal.
- HLS fallback can be promoted to a user-selectable "Low latency vs Compatibility" toggle.
- Document the requirement for H.264-compatible streams for the live tile in README / DESIGN.md.

## 9. Summary

The "awaiting WebRTC" / 400 error was not a bug in the WHEP client code or network reachability — it was an **impedance mismatch** between the camera's H.265 output and browser WebRTC capabilities.

By introducing a lightweight, always-on GStreamer transcoder that produces an H.264 RTMP stream into a dedicated MediaMTX path, and wiring the frontend + DB to use that path, the live video tile now functions while preserving the original high-quality H.265 feed for other uses.

The combination of dynamic configuration, better error messages, and an HLS fallback makes the system much more robust against future codec or infrastructure changes.

---

**Status after fix**: Services running (API, Web, MediaMTX + transcoder). Ready URL: `http://localhost:3000` (hard refresh recommended after changes). Video should now appear via WHEP on the H.264 path.