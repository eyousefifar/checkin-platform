# Bundled edge binaries

These are **downloaded locally** and used by `pksp serve` so you do **not** need system-wide MediaMTX/GStreamer.

| File | Purpose |
|---|---|
| `mediamtx` | RTSP/RTMP/HLS/WebRTC media server (single binary) |
| `ffmpeg` | Optional H.265→H.264 transcode publish into MediaMTX |

## Download / refresh

From repo root or `apps/edge`:

```bash
./apps/edge/scripts/download-binaries.sh
# or
cd apps/edge && ./scripts/download-binaries.sh
```

## How `pksp` finds them

Resolution order for each tool:

1. Env override (`MEDIAMTX_BIN`, `FFMPEG_BIN`)
2. `apps/edge/bin/<name>` (this directory)
3. Directory next to the `pksp` executable
4. `PATH`

GStreamer is **not** required when `ffmpeg` + `mediamtx` are present.
