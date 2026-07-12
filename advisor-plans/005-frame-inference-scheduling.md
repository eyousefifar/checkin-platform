# Plan 005: Process each frame once and isolate blocking inference

> **Executor instructions**: Preserve latest-frame dropping and the one-inference
> semaphore. Fix duplicate consumption at the worker boundary, not inside the
> voting algorithm. Update plan 005 in `advisor-plans/README.md` when complete.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/edge/crates/pksp-vision apps/edge/crates/pksp-core apps/edge/crates/pksp-api advisor-plans/README.md`

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MED
- **Depends on**: `advisor-plans/001-active-stack-verification.md` and
  `advisor-plans/004-real-face-pipeline.md`
- **Category**: bug / performance
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

The worker can repeatedly process one captured image and append enough votes to
commit attendance during a brief camera freeze. It also calls synchronous ORT
inside a Tokio task, so CPU inference can stall API/WebSocket work. Finally,
health reports a configured execution-provider label that is never applied.

## Current state

- `process_loop` stores latest frames as
  `Option<(u32,u32,Vec<u8>,Instant)>` at
  `apps/edge/crates/pksp-vision/src/lib.rs:646` and clones/processes the same
  tuple on every target-FPS tick until it becomes two seconds old.
- `assign_tracks` appends one vote for every quality-ok detection at
  `apps/edge/crates/pksp-core/src/track.rs:128-141`; it correctly assumes each
  call represents a new observation.
- `infer_frame` calls `engine.detect_and_embed` synchronously at
  `apps/edge/crates/pksp-vision/src/lib.rs:925` before its async DB work.
- `ort_sessions::load` parses a provider label but builds both sessions with
  plain `Session::builder().commit_from_file` at
  `apps/edge/crates/pksp-vision/src/lib.rs:228-255`.
- `process_loop` already uses `spawn_blocking` for FFmpeg capture and a shared
  semaphore for inference. Reuse those patterns.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Vision tests | `cd apps/edge && cargo test -p pksp-vision --locked` | exit 0 |
| ORT compile/tests | `cd apps/edge && cargo test -p pksp-vision --features ort --locked` | exit 0 |
| API tests | `cd apps/edge && cargo test -p pksp-api --locked` | exit 0 |
| Full Rust | `cd apps/edge && cargo test --locked` | exit 0 |

## Scope

**In scope**: `apps/edge/crates/pksp-vision/src/lib.rs`, its focused tests,
`apps/edge/crates/pksp-api/src/routes.rs` health assertion if needed, and
`advisor-plans/README.md`.

**Out of scope**: tracker assignment/vote thresholds, model decoding/alignment,
GPU provider enablement, multiple simultaneous inference, adaptive-FPS redesign,
media capture implementation, and `apps/api/**`.

## Git workflow

- Branch: `codex/005-frame-inference-scheduling`
- Commit message: `Make frame inference fresh and nonblocking`
- Do not combine threshold or model changes.

## Steps

### Step 1: Give every captured frame a monotonic sequence

Replace the anonymous latest-frame tuple with a small private `LatestFrame`
struct containing width, height, BGR bytes, capture `Instant`, and a monotonically
increasing `u64 sequence`. Increment sequence only when capture publishes a new
frame. Keep one latest frame; do not queue frames.

In `process_loop`, retain `last_processed_sequence`. For a fresh-but-unchanged
frame, update/publish camera age/status as needed but skip detection, tracking,
voting, and detections broadcast. Mark a sequence processed only after the
worker owns its snapshot.

**Verify**: a focused test proves three polling ticks over one sequence call the
engine once and append at most one vote; a second sequence calls it again.

### Step 2: Split synchronous detection from async face processing

Refactor `infer_frame` into the smallest two stages:

1. a synchronous engine call that returns `Result<Vec<DetectedFace>,
   FaceError>`;
2. the existing async quality/match/track/DB/broadcast stage accepting the
   original BGR pixels and those faces.

After acquiring the existing semaphore, clone the `Arc<dyn FaceEngine>` and the
owned frame bytes into `tokio::task::spawn_blocking`. Because the extended blur
gate still needs source pixels, make the closure return the ownership-preserving
shape `(bgr, Result<Vec<DetectedFace>, FaceError>)` (or an equivalent private
struct); do not consume and discard the only pixel buffer. Await its join
result, then run the async stage.

Treat join/panic and `FaceError` separately. On either, log a rate-limited,
sanitized error, publish no detections or votes for that sequence, release the
permit, and keep the worker alive. Preserve the typed model error so plan 004's
structural failures can make real vision unhealthy; never translate it to an
ordinary empty face list.

Do not wrap SQLite or WebSocket work in `spawn_blocking`.

**Verify**: a Tokio test with a deliberately sleeping mock engine proves a
short timer/future still advances before inference completes.

### Step 3: Report only the execution provider actually in use

For the CPU-first MVP, have session construction store/report
`CPUExecutionProvider` unless code explicitly registers and verifies another
provider. If `ONNX_PROVIDERS` requests an unsupported provider, log a warning
and use/report CPU; do not claim the requested label.

Do not add CoreML/OpenVINO/CUDA features in this plan. Preserve the setting so a
measured future plan can wire one provider deliberately.

**Verify**: a unit test requests a non-CPU label and asserts the engine/health
reports CPU; ORT feature tests pass.

### Step 4: Preserve adaptive scheduling semantics

Measure `infer_ms` across the blocking detection plus async processing stage,
release the semaphore on every path, and keep existing target-FPS bounds. A
skipped duplicate sequence must not increment processed FPS.

**Verify**: vision tests assert duplicate polls do not inflate processed frame
count; full Rust tests pass.

## Test plan

- Latest-frame sequence: duplicate poll, next sequence, stale sequence, and
  sequence wrap policy (use wrapping increment without treating equality as new).
- Blocking isolation: sleeping mock engine plus independent Tokio timer.
- Failure: mock panic/join error does not retain semaphore or kill worker.
- Typed engine failure is observable, creates no vote, and is not treated as
  successful no-face inference.
- Provider truth: CPU default and unsupported requested label.
- Existing track/vote tests remain unchanged.

## Done criteria

- [ ] One capture sequence produces at most one detection/vote pass.
- [ ] Latest-frame dropping remains; no frame queue was introduced.
- [ ] ORT/model execution occurs inside `spawn_blocking` under the existing
  semaphore.
- [ ] Join failures are observable and do not deadlock.
- [ ] Health reports the applied provider, not requested configuration.
- [ ] All four commands pass; plan 005 is marked `DONE`.

## STOP conditions

- The `FaceEngine` implementation cannot safely be called from a blocking
  thread under its existing `Send + Sync` contract.
- Correctness appears to require moving DB/WS work into the blocking closure.
- The change would queue frames or allow concurrent ORT session use.
- A non-CPU provider is required for acceptance; that needs its own benchmarked
  plan.

## Maintenance notes

- If camera count or throughput grows, measure semaphore wait and inference time
  before changing concurrency.
- The frame sequence is the observation identity; future trackers must not
  append votes for status-only polling ticks.
