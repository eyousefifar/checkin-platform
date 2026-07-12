# Plan 019: Upgrade MediaMTX behind the stream regression gate

> **Executor instructions**: This is an optional, high-risk dependency
> migration—not a “latest is better” update. Establish the old-version baseline,
> review every relevant upstream change, update both pins together, and roll
> back on any stream/supervisor regression. Update plan 019 in the index.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/edge/scripts/download-binaries.sh apps/edge/scripts/test-download-binaries.sh apps/edge/scripts/verify-pinned-download.sh apps/edge/scripts/smoke-media.sh apps/edge/scripts/smoke-media-container.sh apps/edge/crates/pksp-media apps/edge/configs/mediamtx.yml configs/mediamtx.yml docker-compose.yml apps/edge/docs/deploy.md advisor-plans/README.md`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: `advisor-plans/007-mediamtx-pipeline.md` and
  `advisor-plans/013-browser-video-state.md`
- **Category**: migration / dependencies
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

The repository pins MediaMTX v1.11.3 while the upstream release observed during
this audit is v1.19.2. The newer line publishes consolidated checksums and years
of fixes, but eight minor releases can change config and media behavior. Upgrade
only after deterministic RTSP→publisher→WHEP and browser gates exist.

## Current state

- Plan 007 deliberately keeps v1.11.3, verifies its per-archive `.sha256sum`,
  fixes platform naming, and establishes generated-video publication smoke.
- `apps/edge/scripts/download-binaries.sh:17` and `docker-compose.yml:3` are the
  two version pins and must stay identical.
- Bundled and container modes use `apps/edge/configs/mediamtx.yml` and
  `configs/mediamtx.yml` with the same path/port intent.
- Plan 013 establishes actual browser `playing` as the WHEP success criterion.
- The v1.19.2 release observed at planning time uses a consolidated
  `checksums.sha256` asset and Linux ARM64 tag `arm64` rather than v1.11.3’s
  `arm64v8` archive tag.

Official references to inspect at execution time:

- `https://github.com/bluenviron/mediamtx/releases`
- `https://mediamtx.org/docs/features/configuration`

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Old/new local smoke | `apps/edge/scripts/smoke-media.sh` | exit 0 on each tested pin |
| Media tests | `cd apps/edge && cargo test -p pksp-media -p pksp-api --locked media` | exit 0 |
| Downloader fixtures | `bash apps/edge/scripts/test-download-binaries.sh` | exit 0 for the current layout's success/failure matrix: sibling checksum before Step 3, consolidated checksum after |
| Real pinned download | `apps/edge/scripts/verify-pinned-download.sh` | temporary production-function download reports the current exact tag: `v1.11.3` before Step 3, `v1.19.2` after |
| Script syntax | `bash -n apps/edge/scripts/download-binaries.sh apps/edge/scripts/test-download-binaries.sh apps/edge/scripts/verify-pinned-download.sh apps/edge/scripts/smoke-media.sh apps/edge/scripts/smoke-media-container.sh` | exit 0 |
| Container config | `docker compose config --quiet` | exit 0 without printing expanded configuration |
| Container smoke | `apps/edge/scripts/smoke-media-container.sh` | exit 0; generated H.264 passes API-ready and RTSP-read checks |
| Container browser window | `PKSP_SMOKE_HOLD_SECONDS=600 apps/edge/scripts/smoke-media-container.sh` | same verified publisher remains available for a bounded ten-minute WHEP check, then cleanup runs |
| Browser app | `cd apps/web && npm run dev` | with the Step-4 generated-stream procedure, dashboard reaches `VIDEO LIVE` and recovers after supervised media restart |

## Scope

**In scope**:

- `apps/edge/scripts/download-binaries.sh`
- `apps/edge/scripts/test-download-binaries.sh` for the new consolidated
  checksum fixture
- `apps/edge/scripts/verify-pinned-download.sh` for the exact target tag/layout
- `apps/edge/scripts/smoke-media-container.sh` (create before changing the pin)
- `docker-compose.yml`
- both MediaMTX YAML files only for required documented key migrations
- `apps/edge/crates/pksp-media` and API media tests only if an upstream behavior
  change requires a compatibility adjustment
- `apps/edge/docs/deploy.md`
- `advisor-plans/README.md`

**Out of scope**: automatic latest resolution, unpinned image tags, camera codec
changes, HLS UI, recording, new media server/runtime, public deployment, and
`apps/api/**`.

## Git workflow

- Branch: `codex/019-mediamtx-upgrade`
- Suggested commits: `Add MediaMTX container regression gate`, then
  `Upgrade tested MediaMTX pin`.
- Keep the version/checksum/config migration in the second, single revertible
  commit; the first commit must pass on v1.11.3.

## Steps

### Step 1: Record the v1.11.3 regression baseline

With plan 007/013 complete, run local generated-video smoke twice, media/API
tests, supervisor forced-exit/restart, clean shutdown, and actual browser WHEP
playback. Record path names, API readiness, codec, startup/recovery times, and
whether any child/listener remains. Use no private camera.

Before changing either pin, create `smoke-media-container.sh` as the exact
Compose gate. It must:

1. require `docker`, `docker compose`, `ffmpeg`, `ffprobe`, and `curl`;
2. refuse to run if any container in this Compose project already exists, if
   TCP 1935/8554/8888/8889/9997 belongs to any listener, or if UDP 8189 is in
   use; use `lsof`/`ss` with the correct protocol and do not kill an owner;
3. run `docker compose config --quiet`, then start only `mediamtx` and remember
   that this script owns it; the rendered service must map `8189:8189/udp`, the
   YAML must retain `rtspTransports: [tcp]` and
   `webrtcLocalUDPAddress: :8189`, and the local browser fixture must advertise
   `127.0.0.1` through `webrtcAdditionalHosts`;
4. publish `lavfi testsrc2` as H.264 to
   `rtsp://127.0.0.1:8554/cam_in_h264`;
5. poll `http://127.0.0.1:9997/v3/paths/get/cam_in_h264` until the response is
   2xx and its parsed JSON reports the exact path ready; use the installed Node
   runtime for JSON parsing instead of substring matching or another package;
6. use `ffprobe -v error -rtsp_transport tcp -select_streams v:0
   -show_entries stream=codec_name -of default=nw=1:nk=1
   rtsp://127.0.0.1:8554/cam_in_h264` and require the single value `h264`;
7. when `PKSP_SMOKE_HOLD_SECONDS` is unset, finish immediately after the gates;
   when set to an integer from 1 through 600, keep the verified publisher and
   container alive for exactly that bounded browser window;
8. trap every exit, terminate only its recorded FFmpeg PID, run
   `docker compose down` only because it proved the project was initially
   stopped, and confirm all five TCP listeners plus UDP 8189 are gone.

The script must not read a camera URL, print Compose expansion, use `pkill`, or
touch an already-running container. Run the normal command twice on the old
pin. Then run the exact bounded browser-window command from the table and,
during its 600-second hold, open
`http://127.0.0.1:8889/cam_in_h264` in a supported browser and verify the
MediaMTX read page actually plays, not merely loads. Let the script's trap own
all publisher/container cleanup.

**Verify**: every table command and the browser playback check passes on
v1.11.3 before a pin changes. If Docker daemon or browser control is unavailable
or not authorized, stop and mark this migration `BLOCKED`; unit/local smoke is
not equivalent evidence.

### Step 2: Review upstream changes through the target

Read official release/config notes for every release after v1.11.3 through
v1.19.2. Build a short migration checklist in the PR description covering only
keys/protocols this repo uses: RTSP, RTMP publisher, WebRTC/WHEP, API, path
source/publisher semantics, environment overrides, logging, and shutdown.

Do not copy a new sample config wholesale. If any used key was removed or
changed semantically, update only that key and add a targeted regression check.

**Verify**: reviewer can map every YAML change to one official release/config
note; unrelated defaults remain untouched.

### Step 3: Update both pins and checksum layout together

Set the downloader release tag to exactly `v1.19.2` and the Compose image tag to
exactly `1.19.2`; the leading `v` belongs to the release/archive naming and not
the container tag. Compare pins after normalizing only that leading `v`, and
reject every other mismatch. Update the downloader to select the exact archive
entry from `checksums.sha256` before extraction and use the new platform asset
matrix, including Linux ARM64 `arm64`. Match a complete checksum-file field and
exact basename—never a substring—and preserve fatal behavior for a missing,
duplicate, or mismatched entry.

Do not retain two download layouts or version branches. The old commit remains
the rollback path.

Update `test-download-binaries.sh` to exercise the target's consolidated file:
one exact matching entry succeeds; missing, duplicate, wrong-basename, and
wrong-digest fixtures fail before extraction and leave the destination sentinel
unchanged. Do not reach the network in this test. Update
`verify-pinned-download.sh` to pass only `v1.19.2` to the shared production
function and require that exact normalized version from its temporary binary.

**Verify**: the named downloader fixture command passes; a controlled temporary
install reports `v1.19.2`; production downloader `v1.19.2` and Compose
`1.19.2` match after the defined normalization.

### Step 4: Run the complete regression gate

Repeat every Step-1 check on the new pin, then run the container path. Verify:

- generated H.264 publication and any configured transcode path;
- MediaMTX API reports the expected path ready;
- RTSP read and browser WHEP actual `playing`;
- supervisor observes forced exit and restarts cleanly;
- API health path/status remains accurate;
- clean shutdown leaves no child/listener;
- no URL credential appears in logs/status.

Run at least ten minutes to expose restart/ICE flapping. Do not accept “process
started” as media success.

Run the container portion with the normal and bounded-hold commands in the
table. Run the application WHEP portion separately so its managed MediaMTX does
not collide with Compose. After confirming TCP ports
1935/8554/8888/8889/9997/8000/3000 and UDP port 8189 are free, terminal A uses
an isolated DB/data tree and prints the exact edge PID:

```sh
cd apps/edge
cargo build -p pksp-cli
RUN_DIR="$(mktemp -d)"
env DATA_DIR="$RUN_DIR" DATABASE_URL="sqlite://$RUN_DIR/pksp.db?mode=rwc" \
  MEDIA_SOURCE_MODE=external \
  CAM_IN_RTSP=rtsp://127.0.0.1:8554/cam_in_h264 \
  CAM_IN_WEBRTC_PATH=cam_in_h264 \
  CORS_ORIGINS=http://localhost:3000 MOCK_VISION=true \
  ./target/debug/pksp serve & EDGE_PID=$!
echo "EDGE_PID=$EDGE_PID"
trap 'kill -INT "$EDGE_PID" 2>/dev/null; wait "$EDGE_PID" 2>/dev/null; rm -rf "$RUN_DIR"' EXIT INT TERM
wait "$EDGE_PID"
```

Terminal B publishes the same generated H.264 source used by the smoke scripts
and records its PID:

```sh
ffmpeg -nostdin -hide_banner -loglevel warning -re -f lavfi \
  -i 'testsrc2=size=1280x720:rate=15' -an -c:v libx264 -preset ultrafast \
  -tune zerolatency -pix_fmt yuv420p -f rtsp -rtsp_transport tcp \
  rtsp://127.0.0.1:8554/cam_in_h264 & PUB_PID=$!
trap 'kill -TERM "$PUB_PID" 2>/dev/null; wait "$PUB_PID" 2>/dev/null' EXIT INT TERM
wait "$PUB_PID"
```

Terminal C runs `cd apps/web && npm run dev`; open
`http://localhost:3000`, require the camera's separate browser state to reach
`VIDEO LIVE`, and leave it playing. To exercise supervised restart, copy only
the printed numeric edge PID into a fourth terminal, run
`MTX_PID="$(pgrep -P "$EDGE_PID" -x mediamtx)"`, require exactly one line, and
inspect `ps -p "$MTX_PID" -o command=`. Only if it is the expected bundled
MediaMTX child, run `kill -TERM "$MTX_PID"`. Require health/video to leave ready,
the supervisor to create a new child, then rerun terminal B's exact publisher
after its old connection exits. The dashboard must return to `VIDEO LIVE`
without a reload. Stop all foreground sessions normally and confirm the checked
ports are free; never use `pkill` or kill an unverified PID.

The edge trap deliberately sends `SIGINT`, which the current `ctrl_c` graceful
shutdown path handles. Do not substitute `SIGTERM` until the application has an
implemented and tested Unix terminate-signal path. If the edge process does not
exit and reap its children after verified `SIGINT`, record the regression and
STOP rather than forcing cleanup and claiming success.

**Verify**: all table commands and both dress rehearsals pass with evidence in
the PR. If Docker, loopback listeners, or browser interaction are not available
under the executor's authority, mark the plan `BLOCKED` rather than omitting the
gate or claiming inferred success.

### Step 5: Update operator documentation minimally

Update the pinned version and any required config migration note. Keep rollback
as reverting the single migration commit; do not document automatic upgrades.

**Verify**: static search finds one version in script/Compose and no `latest`
image/tag behavior.

## Test plan

- Old and new local generated-stream smoke.
- Checksum success/failure and platform asset matrix.
- Media/API unit tests.
- Bundled supervisor restart and clean shutdown.
- Container config/start/publication/WHEP.
- Browser actual `playing` and recovery after media restart.

## Done criteria

- [ ] Old-version baseline passed before migration.
- [ ] Every relevant upstream change through v1.19.2 was reviewed.
- [ ] Downloader release tag is `v1.19.2`, Compose image tag is `1.19.2`, and
  checksum verification precedes extraction/execution.
- [ ] Local/container RTSP, publication, API, WHEP, restart, and shutdown gates
  match or improve the baseline.
- [ ] RTSP remains explicitly TCP-only, WebRTC UDP 8189 is mapped/checked, and
  the tested additional-host candidate matches the browser's host reachability.
- [ ] No credential leaks or unrelated config rewrite occurred.
- [ ] All commands pass; plan 019 is marked `DONE`.

## STOP conditions

- Any used config key/protocol changed without an authoritative migration path.
- Local or container smoke, browser `playing`, restart, or shutdown regresses.
- The target checksum asset/entry is unavailable or mismatched.
- Docker cannot be tested while Compose remains a supported path.
- A supported browser cannot be controlled or observed under current authority;
  actual WHEP `playing` and recovery are mandatory migration gates.
- Fixing the upgrade requires speculative application/media architecture.

If stopped, revert the single pin/config commit, keep verified v1.11.3, mark
plan 019 `BLOCKED` or `REJECTED` with the observed regression, and do not merge
a partial upgrade.

## Maintenance notes

- Repeat this same gate for every future MediaMTX migration.
- Newer upstream versions after the planned target are out of scope; create a
  fresh reviewed plan rather than changing the target mid-execution.
