# Plan 004: Replace heuristic face inference with SCRFD and ArcFace

> **Executor instructions**: This plan controls whether real recognition is
> trustworthy. Follow the steps in order, keep real vision fail-closed, and do
> not tune thresholds around a broken decoder. Never commit employee imagery or
> embeddings. Update plan 004 in `advisor-plans/README.md` when complete.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/edge/crates/pksp-core apps/edge/crates/pksp-vision apps/edge/docs/deploy.md advisor-plans/README.md`

## Status

- **Priority**: P0
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: `advisor-plans/001-active-stack-verification.md` and
  `advisor-plans/002-rust-quality-gates.md`
- **Category**: bug / algorithms
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

The current ORT path does not decode SCRFD outputs or align faces for ArcFace.
It fabricates one box from an arbitrary high tensor value and ignores all
landmarks, so real-mode names and attendance are not trustworthy. This plan
implements only the exact buffalo_l model contract already chosen by the repo,
with pure deterministic tests and a required local real-model gate.

## Current state

- `apps/edge/crates/pksp-vision/src/lib.rs:341-395`
  (`decode_scrfd_heuristic`) scans every output, assumes a square score map,
  creates a box at 25% of frame width, returns at most one face, and always
  returns `None` landmarks.
- `apps/edge/crates/pksp-vision/src/lib.rs:397-438` (`embed_face`) ignores `_kps`
  and nearest-neighbor samples the detector box directly to 112×112.
- `apps/edge/crates/pksp-vision/src/lib.rs:305-339` centers a nearest-neighbor
  detector resize; the selected SCRFD preprocessing uses a bilinear resize in a
  top-left, zero-padded square and must be inverted consistently.
- `apps/edge/crates/pksp-vision/src/lib.rs:258-269` converts any pipeline error
  to an empty face list; health can still report the engine ready.
- `apps/edge/crates/pksp-core/src/quality.rs:148-200` already implements the
  optional pose/blur/exposure gate, but production callers use only the base
  gate.
- `apps/edge/crates/pksp-core/src/embed.rs:44-87` normalizes decoded/mean vectors
  without first rejecting NaN or infinity.
- `rust-port-plans/07-vision-engine-and-models.md` requires explicit SCRFD
  score/bbox/landmark decoding, NMS, inverse-letterbox coordinates, five-point
  similarity alignment, and L2-normalized 512-dimensional embeddings.

Keep the chosen architecture: one `FaceEngine` path shared by live inference
and enrollment, `ort` behind its existing feature, `image` for pixels, no
generic detector framework and no second ML runtime.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Pure core | `cd apps/edge && cargo test -p pksp-core --locked` | exit 0 |
| Vision pure tests | `cd apps/edge && cargo test -p pksp-vision --locked` | exit 0 |
| ORT compile/tests | `cd apps/edge && cargo test -p pksp-vision --features ort --locked` | exit 0 |
| Full Rust | `cd apps/edge && cargo test --locked` | exit 0 |
| Local model gate | `cd apps/edge && PKSP_VISION_FIXTURE_DIR=<operator-owned-dir> cargo test -p pksp-vision --features ort real_model -- --ignored` | exit 0 |

## Scope

**In scope**:

- `apps/edge/crates/pksp-vision/src/lib.rs`
- `apps/edge/crates/pksp-vision/src/scrfd.rs` (create)
- `apps/edge/crates/pksp-vision/src/align.rs` (create)
- `apps/edge/crates/pksp-core/src/{embed.rs,match_.rs,quality.rs}`
- focused tests in those modules and an ignored real-model test under
  `apps/edge/crates/pksp-vision/tests/`
- `apps/edge/docs/deploy.md` for the real-model verification command
- `advisor-plans/README.md`

**Out of scope**:

- New models/runtimes, model downloads in tests, threshold tuning, liveness,
  GPU providers, tracker/FSM changes, media, UI, and `apps/api/**`.
- Any private or identifying fixture in git. The real-model fixture directory
  is operator-owned, ignored, and contains a small manifest with expected face
  counts but no committed data.

## Git workflow

- Branch: `codex/004-real-face-pipeline`
- Suggested commits: `Implement SCRFD decoding`, then
  `Add ArcFace alignment and real model gate`.
- Do not push until the local model gate passes.

## Steps

### Step 1: Freeze the exact buffalo_l tensor contract

With the existing `det_10g.onnx`, inspect ORT input/output names and shapes and
record them as code comments beside a narrow adapter in `scrfd.rs`. Support only
the installed buffalo_l export: detector input NCHW at configured square size;
stride 8/16/32 score, bbox-distance, and five-landmark heads with the observed
anchor count. Reject missing, duplicate, wrong-rank, wrong-length, non-finite,
or ambiguous heads with a typed error.

Do not retain the heuristic as fallback. When `REQUIRE_REAL_VISION=true`, an
invalid contract must fail startup; otherwise the existing explicit mock
fallback may be used with a warning.

**Verify**: a test with a missing and a wrong-shaped synthetic head returns the
named error; `cargo test -p pksp-vision --locked` passes.

### Step 2: Implement canonical preprocessing, SCRFD decoding, and NMS

Replace detector preprocessing with a bilinear resize that preserves aspect
ratio, writes the resized image at the top-left of a zero-filled detector
square, converts BGR to RGB/NCHW, and preserves the model’s existing
normalization. Return the exact scale/padding metadata used to invert outputs.

In `scrfd.rs`, implement model-specific pure functions for:

1. anchor centers for the observed strides/anchor count;
2. score thresholding;
3. distance-to-box and distance-to-five-landmark decoding;
4. clipping in letterboxed detector coordinates;
5. inverse scale/padding back to original pixels;
6. descending-score IoU NMS at the selected model’s 0.4 default;
7. deterministic output ordering.

Every numeric input and output must be finite. Degenerate boxes and landmarks
outside a reasonable clipped frame boundary are rejected. Keep functions over
slices/structs so tests do not require ORT.

Tests must cover bilinear/top-left preprocessing, zero faces, all observed
strides/anchors, multiple faces, overlapping-face NMS, non-square original
frames with padding, edge clipping, malformed head lengths, and NaN/Inf
rejection.

**Verify**: `cargo test -p pksp-vision --locked scrfd` → all named tests pass.

### Step 3: Implement five-point ArcFace similarity alignment

In `align.rs`, define the standard 112×112 ArcFace five-point destination
template exactly as `(38.2946,51.6963)`, `(73.5318,51.5014)`,
`(56.0252,71.7366)`, `(41.5493,92.3655)`, and `(70.7299,92.2041)`, then solve
the 2D similarity transform from decoded landmarks. Apply
the inverse transform with bilinear sampling from BGR source pixels into an
RGB/NCHW tensor normalized exactly as the recognition model expects.

Use a small local solver and sampler; do not add an image-processing framework
for one transform. Reject singular/non-finite transforms and out-of-bounds
source buffers. Do not fall back to an unaligned bbox crop in real mode.

Tests must prove destination landmarks map within a small epsilon, identity and
known scale/translation transforms sample expected pixels, singular points are
rejected, and the output tensor has exactly `3*112*112` finite values.

**Verify**: `cargo test -p pksp-vision --locked align` → all pass.

### Step 4: Connect the decoder and alignment to ORT

Replace `decode_scrfd_heuristic` and the bbox-resize body of `embed_face` with
the two pure modules. Preserve `DetectedFace` shape, return all post-NMS faces,
and require five landmarks before recognition. Validate recognition output is
exactly 512 finite values before L2 normalization.

Change the shared trait boundary to
`FaceEngine::detect_and_embed(...) -> Result<Vec<DetectedFace>, FaceError>`.
`MockFaceEngine` returns `Ok(...)`; the ORT implementation returns a typed error
for bad tensors, session/runtime failure, invalid alignment, or invalid output.
An empty vector means a successful inference with no faces and must never stand
in for an error. Update every caller and mock explicitly rather than adding a
second compatibility method.

Make pipeline failure observable: store or log a rate-limited sanitized error
and mark real vision not ready on structural model errors. Do not log tensors,
pixels, embeddings, or source URLs.

**Verify**: tests distinguish `Ok(vec![])` from each typed error;
`cargo test -p pksp-vision --features ort --locked` → pass.

### Step 5: Fail closed throughout quality and matching

Add finite checks at `quality_gate`, `unpack_embedding`, `mean_l2_embedding`,
and `match_top1`. An invalid query/gallery/model vector must yield an error or
UNKNOWN, never select an employee.

Now that valid landmarks and an aligned crop exist, call
`quality_gate_extended` from both live and enrollment paths. Honor existing
`POSE_MAX_YAW` and `BLUR_MIN_VAR` only when greater than zero; preserve disabled
behavior at zero. Keep exposure disabled unless existing settings expose its
bounds—do not add speculative knobs.

**Verify**: `cargo test -p pksp-core -p pksp-vision --locked` → pass, including
new NaN/Inf and pose/blur cases.

### Step 6: Pass the operator-owned real-model gate

Create an ignored integration test that reads only
`PKSP_VISION_FIXTURE_DIR`. Its local manifest must provide, per image, expected
face count and whether an embedding is expected. Assert:

- correct face count for single-face, multi-face, and no-face fixtures;
- finite in-frame boxes and landmarks;
- finite 512-D unit-norm embeddings;
- deterministic repeated embedding for the same image (cosine at least 0.9999);
- cosine at least 0.99 against an operator-owned expected embedding produced by
  an independently trusted buffalo_l reference for the same fixture;
- no model/pixel/embedding data is written to the repository.

Document the command and fixture privacy rule. This gate is mandatory before
real recognition is enabled.

**Verify**: run the local model command in the table → exit 0.

## Test plan

- Pure decoder tests use hand-built tiny tensor heads.
- Alignment tests use generated grids, never faces.
- Core finite tests cover detector score/box, decoded blob, mean vectors, query,
  gallery, and sorting.
- The ignored real-model test uses operator-owned local images and exact count/
  shape/norm assertions.
- Run pure, feature, full, and local-model commands in that order.

## Done criteria

- [ ] `decode_scrfd_heuristic` and unaligned real-mode bbox resizing are gone.
- [ ] Multiple faces, landmarks, NMS, inverse letterbox, and affine alignment
  have deterministic pure tests.
- [ ] Every model/gallery numeric boundary rejects non-finite values.
- [ ] Pose/blur settings are either applied or remain explicitly disabled at 0.
- [ ] Structural model errors make real vision unavailable; no silent empty
  success remains.
- [ ] `FaceEngine` exposes typed inference failure and every mock/caller handles
  it explicitly.
- [ ] All five commands pass, including the operator-owned real-model gate.
- [ ] No private fixture/model output is tracked.
- [ ] Plan 004 is marked `DONE`.

## STOP conditions

- The installed model’s tensor names/shapes do not identify a single exact
  SCRFD contract; report the observed metadata without adding heuristics.
- No operator-owned, legally usable local face fixture set is available.
- Correct recognition requires changing the selected model or embedding size.
- A proposed fallback would emit embeddings from an unaligned crop.
- Any test or log would persist biometric pixels or embeddings.

## Maintenance notes

- Threshold calibration is a later camera-specific task; never compensate for
  decoder/alignment regressions with lower thresholds.
- If the model file changes, the tensor-contract and local fixture gates must
  pass before deployment.
- Keep SCRFD/ArcFace modules model-specific until a second real model is
  approved; a generic inference framework is unnecessary.
