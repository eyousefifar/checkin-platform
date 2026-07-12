# Plan 016: Render the one-or-two-camera wall from health

> **Executor instructions**: Expose the backend capability already present—no
> camera-management product. Render only public health fields and cap the
> accepted UI at two enabled cameras. Update plan 016 in the index.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/web/src/app/page.tsx apps/web/src/app/camera-wall.test.tsx apps/web/src/components/CameraTile.tsx apps/web/src/hooks/useHealth.ts apps/web/src/lib/types.ts apps/edge/crates/pksp-api/src/routes.rs advisor-plans/README.md`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: `advisor-plans/007-mediamtx-pipeline.md`,
  `advisor-plans/008-live-websocket-state.md`, and
  `advisor-plans/013-browser-video-state.md`
- **Category**: direction / UI
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

The Rust API and product scope support one or two cameras, but the dashboard
hardcodes only `cam_in`. Rendering the typed public health list makes the
existing capability visible without adding camera CRUD, dynamic layout systems,
or another endpoint.

## Current state

- `/api/health` maps enabled cameras to `id`, `name`, `direction`, `enabled`,
  and browser-safe `webrtc_path` at
  `apps/edge/crates/pksp-api/src/routes.rs:22-71`.
- Dashboard derives only `cam_in` state/FPS/path and renders exactly one
  `CameraTile` at `apps/web/src/app/page.tsx:10-70`.
- `useLiveWs` already keys detections, FPS, and camera status by camera ID after
  plan 008.
- `useHealth` owns retrying public health after plans 011/013.
- `GOAL.md` accepts 1–2 RTSP cameras; larger camera walls are not a goal.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Dashboard tests | `cd apps/web && npm test -- --run camera-wall` | exit 0 |
| Web tests | `cd apps/web && npm test -- --run` | exit 0 |
| Web gates | `cd apps/web && npm run lint && npm run typecheck` | exit 0 |
| Build | `cd apps/web && npm run build` | exit 0 |

## Scope

**In scope**:

- `apps/web/src/app/page.tsx` and
  `apps/web/src/app/camera-wall.test.tsx` (create)
- `apps/web/src/components/CameraTile.tsx` only for optional/typed camera state
- `apps/web/src/lib/types.ts`, `apps/web/src/hooks/useHealth.ts` only if the
  existing health camera type is incomplete
- Rust health test only if a required public field is currently absent
- `advisor-plans/README.md`

**Out of scope**: camera add/edit/delete, authenticated camera endpoint, RTSP
URLs, sorting UI, drag/drop, arbitrary N-camera grids, per-camera settings,
media backend changes, and `apps/api/**`.

## Git workflow

- Branch: `codex/016-dynamic-camera-wall`
- Commit message: `Render configured camera wall`

## Steps

### Step 1: Freeze the public camera view model

Ensure `HealthCamera` contains only ID, name, direction, enabled,
`webrtc_path`, and any plan-007 publication status needed by the browser. It
must never contain `rtsp_url` or credentials. Preserve health list order, which
already follows configured camera order.

**Verify**: Rust/web type tests assert public projection and absence of source
URL fields.

### Step 2: Render zero, one, or two enabled cameras

Replace hardcoded `cam_in` with `health.cameras.filter(enabled).slice(0,2).map`.
For each ID, pass keyed detections, explicit camera status, keyed FPS, name,
direction, and WHEP path to `CameraTile`.

Layouts:

- one camera: preserve the current two-thirds video hero plus ticker;
- two cameras: a simple one/two-column grid inside the video area, responsive to
  viewport; keep the ticker unchanged;
- zero cameras: professional “No enabled cameras” state with deployment-doc
  guidance, not a fake demo path.

No generic layout component is needed.

**Verify**: tests for zero/one/two cameras assert correct tile props and no
hardcoded `cam_in` fallback.

### Step 3: Make health retry and excess scope explicit

While health retries, render a health state rather than camera-offline. If more
than two enabled cameras arrive, render the first two plus a small operator
warning that this appliance UI supports two; do not silently start unlimited
WebRTC streams.

**Verify**: tests cover initial failure/recovery and three-camera cap warning.

### Step 4: Dress-rehearse two generated streams

Use a fresh local session and the explicit external-publisher mode from plan
007. First check TCP ports 1935, 8554, 8888, 8889, 9997, 8000, and 3000 with
`lsof -nP -iTCP:<port> -sTCP:LISTEN` (or `ss -ltn` on Linux), and check the
explicit MediaMTX WebRTC media port with `lsof -nP -iUDP:8189` (or `ss -lun`).
If any belongs to an unrelated process, STOP; do not kill it or silently reuse
its stream.

In terminal A, start the isolated API/media stack with only generated-loopback
URLs:

```sh
cd apps/edge
RUN_DIR="$(mktemp -d)"
trap 'rm -rf "$RUN_DIR"' EXIT INT TERM
DATA_DIR="$RUN_DIR" DATABASE_URL="sqlite://$RUN_DIR/pksp.db?mode=rwc" \
  MEDIA_SOURCE_MODE=external \
  CAM_IN_RTSP=rtsp://127.0.0.1:8554/cam_in_h264 \
  CAM_OUT_RTSP=rtsp://127.0.0.1:8554/cam_out \
  CAM_IN_WEBRTC_PATH=cam_in_h264 CAM_OUT_WEBRTC_PATH=cam_out \
  CORS_ORIGINS=http://localhost:3000 \
  MOCK_VISION=true cargo run -p pksp-cli -- serve
```

After MediaMTX is listening, terminal B publishes visually distinct generated
streams and records only their own PIDs:

```sh
ffmpeg -nostdin -hide_banner -loglevel warning -re -f lavfi \
  -i 'testsrc2=size=1280x720:rate=15' -an -c:v libx264 -preset ultrafast \
  -tune zerolatency -pix_fmt yuv420p -f rtsp -rtsp_transport tcp \
  rtsp://127.0.0.1:8554/cam_in_h264 & IN_PID=$!
ffmpeg -nostdin -hide_banner -loglevel warning -re -f lavfi \
  -i 'smptebars=size=1280x720:rate=15' -an -c:v libx264 -preset ultrafast \
  -tune zerolatency -pix_fmt yuv420p -f rtsp -rtsp_transport tcp \
  rtsp://127.0.0.1:8554/cam_out & OUT_PID=$!
trap 'kill -TERM "$IN_PID" "$OUT_PID" 2>/dev/null; wait "$IN_PID" "$OUT_PID" 2>/dev/null' EXIT INT TERM
wait
```

In terminal C run `cd apps/web && npm run dev`, open
`http://localhost:3000` at
1440×900, and verify both WHEP videos, keyed HUD/status/FPS, and ticker for ten
minutes. Stop web and API with their foreground interrupts; terminal B's trap
stops only its recorded FFmpeg children. Confirm all seven checked TCP ports and
UDP 8189 are free afterward. Plan 007's local config must advertise
`127.0.0.1` as its WebRTC additional host for this same-host rehearsal. Do not
use `pkill`, real camera URLs, or private imagery, and do not tune/degrade video
without a recorded failure.

**Verify**: manual result records both streams playing without cross-wired HUD
or unbounded reconnects; automated commands remain green.

## Test plan

- Health zero/one/two/three cameras and retry recovery.
- Per-ID detections, camera status, FPS, direction, and path mapping.
- Public type excludes RTSP fields.
- Manual two-generated-stream dress rehearsal.

## Done criteria

- [ ] Dashboard source has no hardcoded `cam_in` tile/path/state lookup.
- [ ] Zero/one/two states are explicit and correct.
- [ ] At most two WHEP streams start; excess is explained.
- [ ] Per-camera HUD/status/FPS cannot cross wires.
- [ ] No private camera field reaches the browser.
- [ ] All commands and dress rehearsal pass; plan 016 is marked `DONE`.

## STOP conditions

- Product scope now exceeds two simultaneous cameras.
- Health no longer provides a safe browser-only camera projection.
- Two generated WHEP streams exceed appliance/browser budget; report measured
  CPU/FPS/reconnect facts before changing layout or codecs.

## Maintenance notes

- Keep the two-camera cap until a measured requirement and appliance budget
  justify a real camera-wall design.
- Camera configuration remains an operator/deployment concern for this MVP.
