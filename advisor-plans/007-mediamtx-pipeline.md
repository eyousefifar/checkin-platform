# Plan 007: Wire and verify one browser-safe MediaMTX path

> **Executor instructions**: Build one explicit camera-to-publisher-to-WHEP
> path. Never log or commit source URLs containing credentials. Keep MediaMTX as
> the media server; do not reimplement WebRTC. Update plan 007 in the index.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- .env.example apps/edge/Cargo.toml apps/edge/crates/pksp-db apps/edge/crates/pksp-media apps/edge/crates/pksp-api apps/edge/configs apps/edge/scripts apps/edge/docs apps/edge/README.md configs docker-compose.yml advisor-plans/README.md`

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MED
- **Depends on**: `advisor-plans/001-active-stack-verification.md` and
  `advisor-plans/003-security-containment.md`
- **Category**: bug / dependencies
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

When an explicit H.264 source is configured, the current policy disables the
transcoder but does not publish that source into the publisher-only MediaMTX
path. The browser is then given a path with no producer. The separate container
config also relies on undocumented YAML interpolation. This plan makes both
native-H.264 and H.265 inputs publish to one verified browser-safe path.

## Current state

- `apps/edge/configs/mediamtx.yml:17-30` declares `demo`, `cam_in`,
  `cam_in_h264`, and `cam_out` as publisher paths.
- `should_transcode` returns false whenever `CAM_IN_H264_RTSP` exists at
  `apps/edge/crates/pksp-media/src/lib.rs:347-352`.
- `serve` then gives `MediaSupervisor` no source at
  `apps/edge/crates/pksp-api/src/lib.rs:54-78`; nothing publishes the native
  H.264 feed.
- `run_transcoder_loop` publishes only an H.265 input to
  `rtmp://127.0.0.1:1935/cam_in_h264` at
  `apps/edge/crates/pksp-media/src/lib.rs:139-215`.
- `health` advertises a configured/preferred path without proving it has a
  publisher at `apps/edge/crates/pksp-api/src/routes.rs:22-71`.
- `configs/mediamtx.yml:24-31` contains variable syntax not interpreted by
  MediaMTX itself.
- `download-binaries.sh:17-36` pins MediaMTX 1.11.3 and installs an unverified
  archive. `docker-compose.yml:3` pins the same version.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Media unit tests | `cd apps/edge && cargo test -p pksp-media --locked` | exit 0 |
| API health tests | `cd apps/edge && cargo test -p pksp-api --locked media` | exit 0 |
| Rust suite | `cd apps/edge && cargo test --locked` | exit 0 |
| Script tests | `bash apps/edge/scripts/test-download-binaries.sh` | exit 0; mismatch fails in a temporary tree and installed binaries are untouched |
| Real pinned download | `apps/edge/scripts/verify-pinned-download.sh` | downloads into a temporary destination and reports exactly `v1.11.3` |
| Script syntax | `bash -n apps/edge/scripts/download-binaries.sh apps/edge/scripts/test-download-binaries.sh apps/edge/scripts/verify-pinned-download.sh apps/edge/scripts/smoke-media.sh` | exit 0 |
| Local smoke | `apps/edge/scripts/smoke-media.sh` | exit 0; MediaMTX API reports published path ready |

## Scope

**In scope**:

- `apps/edge/Cargo.toml`, `apps/edge/crates/pksp-media/Cargo.toml`,
  `apps/edge/crates/pksp-media/src/lib.rs`, and tests
- `apps/edge/crates/pksp-db/src/lib.rs` for the validated source-mode setting
- `apps/edge/crates/pksp-api/src/{lib.rs,routes.rs}` and tests
- `apps/edge/configs/mediamtx.yml`, `configs/mediamtx.yml`
- `apps/edge/scripts/download-binaries.sh`,
  `apps/edge/scripts/test-download-binaries.sh` (create), and
  `apps/edge/scripts/verify-pinned-download.sh` (create), and
  `apps/edge/scripts/smoke-media.sh` (create)
- `docker-compose.yml`, `apps/edge/docs/deploy.md`, `apps/edge/README.md`
- `.env.example` path/mode documentation without values
- `advisor-plans/README.md`

**Out of scope**:

- Camera firmware changes, recording/HLS, a media-server rewrite, GStreamer,
  browser UI, multiple arbitrary transcoding profiles, public deployment, and
  `apps/api/**`.
- Logging or testing a real private RTSP URL.

## Git workflow

- Branch: `codex/007-mediamtx-pipeline`
- Suggested commits: `Wire browser safe media publisher`, then
  `Verify bundled MediaMTX artifacts`.

## Steps

### Step 1: Define one unambiguous publication policy

Keep MediaMTX paths publisher-only. Add one parsed enum setting,
`MEDIA_SOURCE_MODE=external|copy|transcode`, defaulting to `external`. The
default is intentional: the current default `CAM_IN_RTSP` is the local
`/demo` subscriber URL, so non-emptiness cannot identify a physical camera.
Remove `FORCE_TRANSCODE` inference and do not add `auto`; deployment must state
which source owns publication.

The exact policy is:

- `external`: start MediaMTX but no FFmpeg publisher; `CAM_IN_WEBRTC_PATH`
  names a path supplied by a generated demo, another service, or an operator;
- `copy`: require non-empty `CAM_IN_H264_RTSP`, stream-copy its video with
  FFmpeg, and publish H.264 to `cam_in_h264`;
- `transcode`: require non-empty `CAM_IN_RTSP`, transcode video to H.264, and
  publish it to `cam_in_h264`.

Reject an unknown mode, a missing required input, or `external` combined with
a non-empty `CAM_IN_H264_RTSP` before starting a child. `CAM_IN_RTSP` remains
the vision-capture source; `copy` may deliberately use a separate lower-cost
H.264 substream for browser publication. Change `MediaConfig` from an H.265-only
field to `{source_rtsp, source_mode, publish_path}` and use one supervised
FFmpeg loop whose only mode-specific arguments are copy versus encode.

Do not put source URLs in application logs, API status, or child error text.
FFmpeg still receives its input URL in process arguments; document the actual
boundary rather than promising otherwise: production runs under a dedicated
service account on a dedicated appliance, and the URL remains visible to that
UID and privileged host users. A shared interactive account or host without
process isolation is not an accepted credential boundary.

**Verify**: unit tests assert exact mode/path/argument selection for external,
copy, and transcode; reject invalid combinations; and compare only redacted
argument projections, never a credential-bearing fixture.

### Step 2: Make readiness reflect a live publisher

Starting FFmpeg is not readiness. Add a narrow loopback MediaMTX API client in
`pksp-media` using the workspace-pinned Hyper/serde_json stack; do not add a
second HTTP client. Add `MEDIAMTX_API_ADDR`, default
`127.0.0.1:9997`, parse it as `SocketAddr`, and reject a malformed or
non-loopback address before child startup. Carry that address in `MediaConfig`
so tests can inject an ephemeral loopback server without a global environment
mutation. Restrict supported path names to `[A-Za-z0-9_-]+`, then poll
`GET /v3/paths/get/{path}` while MediaMTX is alive. Set
`preferred_webrtc_path` and publication state `ready` only when the response is
2xx, names the expected path, and reports it ready with a live publisher. Clear
readiness immediately on API failure, missing/not-ready path, FFmpeg exit, or
MediaMTX exit, while retaining a non-sensitive `starting`/`unavailable` state.

External mode uses the same poll: it can wait indefinitely for its external
publisher without claiming success. Copy/transcode starts polling after the
child spawn, but child spawn alone never changes the path to ready. Use capped
250 ms→2 s polling and stop it through the existing supervisor cancellation;
no separate health scheduler is needed. Health may expose process/publication
state but never source URL.

When no publisher is ready, health should retain the configured path for demo
mode or report the camera path as unavailable/starting rather than silently
claiming browser video is ready. Preserve the existing camera fields.

**Verify**: supervisor tests use a local fake HTTP server to cover process-up/
path-absent, path-not-ready, path-ready, malformed response, child exit,
MediaMTX exit, restart, and stop. Health tests cover external-waiting,
publishing, and failed publisher states.

### Step 3: Remove unsupported configuration interpolation

Make both YAML files valid static publisher configurations. Inject source URLs
only through the Rust supervisor or documented `MTX_*` environment overrides;
do not embed shell-style expressions or credential examples in YAML.

Align container and bundled modes on the same path names/ports. Document that
the container alone needs an external publisher when the Rust supervisor is not
running. Remove Compose `env_file` and its interpolation comment because the
static publisher config consumes no camera secret; operators pass credentials
only to the dedicated publisher process.

Make implicit transport listeners explicit in both YAML files:

- set `rtspTransports: [tcp]`, because all application/smoke FFmpeg commands
  already use RTSP-over-TCP and this avoids hidden UDP 8000/8001 listeners;
- set `webrtcLocalUDPAddress: :8189` and map `8189:8189/udp` in Compose;
- set `webrtcAdditionalHosts: [127.0.0.1]` for the deterministic same-host
  default. Deployment notes must require the appliance's actual LAN address via
  the documented `MTX_WEBRTCADDITIONALHOSTS` override before remote LAN browser
  acceptance; never guess an interface/public address in code.

Keep WebRTC HTTP on 8889 and API on 9997. Do not enable WebRTC/TCP, extra ICE
servers, or a port range without an observed network requirement.

**Verify**: `smoke-media.sh` starts the config successfully; a static scan finds
no `${` or credential-bearing URL in either YAML or Compose. Compose config
shows TCP 1935/8554/8888/8889/9997 and UDP 8189 explicitly, with no RTSP UDP
mapping.

### Step 4: Verify the current pinned MediaMTX artifact

Keep v1.11.3 for this wiring plan and keep the script/container pins aligned.
That release publishes a sibling `.sha256sum` per archive. Correct its platform
matrix: Linux ARM64 uses `arm64v8`, while Darwin ARM64 uses `arm64`. Derive one
archive basename, download its checksum, verify with `shasum -a 256` or
`sha256sum`, and only then extract/copy/execute it. A missing checksum or
mismatch is fatal. If an override remains, require an operator-supplied trusted
checksum and exact asset tag; do not fetch “latest”.

Replace the script’s incidental path-resolution runtime with
`cp -Lf "$(command -v ffmpeg)" ...` and verify the copied binary. Do not add
another scripting dependency.

Refactor the script so checksum parsing/verification is a side-effect-free
shell function and sourcing the file does not install anything. Create
`test-download-binaries.sh`: in a `mktemp` tree, generate a harmless tiny
archive/checksum fixture, source the production functions, prove the correct
checksum reaches the staged install, then prove a one-character mismatch fails
before extraction and leaves an existing destination sentinel unchanged. The
test must never read, replace, or chmod `apps/edge/bin`.

Create `verify-pinned-download.sh` as the network/release integration gate. It
sources the same production `download_mediamtx(version, destination)` function,
passes exact tag `v1.11.3` and a private `mktemp` destination, and traps cleanup.
It must exercise production platform mapping, release URL, sibling checksum
asset selection, exact digest verification, extraction, and executable check;
then require the staged binary's version output to normalize to exactly
`v1.11.3`. It must neither consult nor replace `apps/edge/bin`, and must fail
closed when offline. The normal installer calls this same function with the
real bin directory, so the gate cannot drift into a second downloader.

**Verify**: both script commands in the table pass; the fixture output and real
download destination contain paths only under their temporary directories.

### Step 5: Add a local deterministic smoke script

Create `smoke-media.sh` that uses generated FFmpeg test video—never a real
camera—to:

1. create a private `mktemp` directory and choose high temporary RTSP, RTMP,
   WebRTC-HTTP, API TCP ports plus a WebRTC-media UDP port; use `lsof` or `ss`
   with the correct protocol to reject every occupied port immediately before
   spawn, retry a bounded number of random sets, and fail if no safe set is
   available;
2. write a temporary config containing those ports and the static path policy,
   `rtspTransports: [tcp]`, the chosen `webrtcLocalUDPAddress`, and
   `webrtcAdditionalHosts: [127.0.0.1]`, leaving the repository config and any
   existing service untouched;
3. start the pinned local MediaMTX with that temporary config;
4. publish `lavfi testsrc` H.264 to the chosen path;
5. poll the temporary MediaMTX API endpoint until that exact path reports ready;
6. stop only its recorded child PIDs on success/failure via `trap` and remove
   the temporary directory;
7. emit no source credentials and leave no process/listener behind.

Do not attempt full WebRTC rendering in shell. Plan 013 covers browser playback.

**Verify**: run the smoke command twice; both runs exit 0 and leave no listener
owned by the script.

## Test plan

- Pure source-mode/FFmpeg-argument tests.
- Supervisor status transitions including child exit/restart/stop.
- Health projection without source URLs.
- Downloader checksum failure and success in a temp directory.
- Real pinned-release download/version check in a separate temporary directory.
- Generated-video MediaMTX publication smoke on checked temporary TCP and UDP
  ports.

## Done criteria

- [x] Native H.264 and H.265 inputs both publish H.264 to the same declared
  browser-safe path; demo remains explicit.
- [x] Health distinguishes process/configuration from a ready publisher.
- [x] YAML contains only valid static config and no credential material.
- [x] RTSP transport is explicitly TCP-only; WebRTC UDP 8189/candidate hosts
  are explicit in config, Compose, and deployment guidance.
- [x] Script and container use one tested, pinned MediaMTX version.
- [x] Downloaded archive is checksum-verified before extraction/execution.
- [x] The production download function fetches and identifies exact v1.11.3 in
  a temporary destination; smoke does not rely only on a pre-existing binary.
- [x] All unit/full/smoke commands pass; plan 007 is marked `DONE`.

## STOP conditions

- Candidate MediaMTX config requires a migration beyond the current path/port
  keys.
- The release does not publish a verifiable checksum for the selected archive.
- FFmpeg stream-copy cannot publish the selected H.264 stream to MediaMTX.
- Acceptance requires real camera credentials or public network exposure.
- A change would log or return a source RTSP URL.
- The deployment host is shared with untrusted users under the service account,
  or its process table exposes that account's FFmpeg arguments to unprivileged
  users; obtain process isolation before placing camera credentials there.

## Maintenance notes

- Keep the binary and container pins synchronized; upgrade deliberately through
  plan 019’s regression gate, never through an automatic latest tag.
- Add another publication profile only after a named second-camera codec needs
  it; do not generalize preemptively.
