# Plan 006: Make enrollment cumulative, bounded, and recoverable

> **Executor instructions**: Enrollment mutates biometric files, image metadata,
> embeddings, and live gallery state. Write regression tests first, stage file
> work, and never delete existing rows before successful analysis. Update plan
> 006 in `advisor-plans/README.md` when complete.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- .env.example apps/edge/crates/pksp-api apps/edge/crates/pksp-db apps/edge/crates/pksp-vision apps/edge/migrations apps/web/src/lib/types.ts advisor-plans/README.md`

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MED
- **Depends on**: `advisor-plans/001-active-stack-verification.md`,
  `advisor-plans/004-real-face-pipeline.md`, and
  `advisor-plans/005-frame-inference-scheduling.md`
- **Category**: bug / security
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

Incremental uploads currently replace an employee embedding using only the new
request, while recompute deletes image rows and recopies every file before it
knows processing will succeed. Ordinary multi-photo requests can also hit
Axum’s implicit body limit, and the handler buffers fields without explicit
count/file/dimension bounds. This plan makes the existing two endpoints safe at
the accepted 5–10-photo scale without inventing a new storage service.

## Current state

- `upload_images` buffers every multipart field into a `Vec` at
  `apps/edge/crates/pksp-api/src/routes.rs:195-236`; no route-specific body
  limit or validation policy exists.
- `enroll_images` writes each file/row immediately, then averages only vectors
  from the current request at
  `apps/edge/crates/pksp-vision/src/lib.rs:1107-1218`.
- `recompute_embedding` reads existing files, deletes all image rows, and calls
  `enroll_images`, which creates new UUID paths and file copies at
  `apps/edge/crates/pksp-api/src/routes.rs:238-292`.
- If usable vectors fall below `MIN_ENROLL_IMAGES`, the old embedding is not
  deleted.
- Schema already separates `employee_images` and one employee embedding in
  `apps/edge/migrations/001_init.sql:11-27`; no schema change is required.
- Response fields are frozen: `received`, `usable`, `rejected`,
  `embedding_ready`, and `num_images_used`. Additive fields are allowed.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Vision enrollment tests | `cd apps/edge && cargo test -p pksp-vision --locked enroll` | exit 0 |
| API upload tests | `cd apps/edge && cargo test -p pksp-api --locked enrollment` | exit 0 |
| DB tests | `cd apps/edge && cargo test -p pksp-db --locked` | exit 0 |
| Full Rust | `cd apps/edge && cargo test --locked` | exit 0 |

## Scope

**In scope**:

- `apps/edge/crates/pksp-db/src/lib.rs` and `Settings`
- `apps/edge/crates/pksp-vision/src/lib.rs`
- `apps/edge/crates/pksp-api/src/{lib.rs,routes.rs}`
- focused tests in the three crates
- `.env.example` for limit names without secrets
- `apps/web/src/lib/types.ts` only for an additive upload-result type
- `advisor-plans/README.md`

**Out of scope**:

- New endpoints, image deletion UI, object storage, background jobs, schema
  redesign, unbounded streaming infrastructure, model changes, and
  `apps/api/**`.
- Transactionally coupling employee metadata creation to image processing; plan
  012 makes the existing two-request UI recoverable.

## Git workflow

- Branch: `codex/006-enrollment-integrity`
- Suggested commits: `Add bounded enrollment contract`, then
  `Make enrollment recompute atomic`.

## Steps

### Step 1: Characterize the mutation contract

Using the plan-001 API harness, write failing tests for:

1. upload batch A then batch B produces an embedding using all usable A+B images;
2. recompute changes no file path and creates no duplicate file;
3. failed decode/analysis leaves prior rows, files, embedding, and gallery version
   unchanged;
4. fewer than minimum usable images deletes the stale embedding;
5. missing employee returns 404;
6. empty, too-many, too-large, and over-dimension images return 4xx without
   mutation.

Use `MockFaceEngine`, generated image bytes, temporary SQLite/data directories,
and path/count assertions. Do not use real faces or models.

**Verify**: the new regression tests fail for the named current behaviors before
implementation.

### Step 2: Make upload limits explicit and configurable

Add settings with conservative defaults suitable for 5–10 images:

- `MAX_ENROLL_UPLOAD_BYTES=33554432` (32 MiB total request);
- `MAX_ENROLL_FILES=10`;
- `MAX_ENROLL_FILE_BYTES=5242880` (5 MiB each);
- `MAX_ENROLL_IMAGE_DIM=4096` and `MAX_ENROLL_PIXELS=20000000`.

Validate settings at startup. Apply `DefaultBodyLimit::max(total)` only to the
image-upload route. In the handler accept only file fields and reject an empty
batch. Read every field by chunks and stop before it exceeds its per-file
limit—do not call `field.bytes()` first. Detect JPEG/PNG/WebP from content,
derive the stored extension from that detected format, inspect header
dimensions, and apply the `image` crate allocation/dimension limits before full
decode. Preserve Axum’s 413 for total body overflow and use the existing
`{detail}` 400 shape for field/image errors.

Do not disable body limits globally and do not add a multipart library.

**Verify**: `cargo test -p pksp-api --locked enrollment_limits` → pass.

### Step 3: Separate analysis from persistence

Refactor enrollment into narrow stages:

1. validate/decode/analyze a bounded set of new or existing files;
2. produce records containing original path (if existing), proposed final path
   (if new), usable/reason, and optional finite embedding;
3. persist the complete result once.

Run decode/model work through the blocking pattern established by plan 005.
For every upload, analyze all existing image rows plus new staged files so the
mean represents the complete current gallery. At this scale, reanalysis is the
simple correct choice; do not add per-image embedding storage or a cache.

Keep ordinary image outcomes separate from systemic failure:

- reject an unsupported/corrupt new upload during request validation with 4xx
  and no mutation;
- persist a decoded image that has no face, multiple faces, or fails the quality
  gate as `usable=false` with a stable reason in `results`;
- treat model/session/tensor/non-finite errors, filesystem failures, and DB
  errors as systemic: abort the whole request before commit and preserve all
  prior state;
- during recompute, a pre-existing missing/unreadable/corrupt file remains an
  explicit rejected row because the request did not introduce it.

Do not catch `FaceError` as `no_face`; successful zero-face inference is the
only path to that ordinary result.

Return the frozen aggregate fields plus additive
`results: [{filename, usable, reason}]` for plan 012. Do not expose absolute
paths.

**Verify**: pure/mock analysis tests pass and two batches use the combined count.

### Step 4: Persist one recoverable unit

Write new files to UUID-named staging paths under the employee directory. After
all analysis succeeds, move staged files to final names, begin one sqlx
transaction, insert/update image rows in place, upsert or delete the employee
embedding, and bump gallery version. Commit only after every DB statement
succeeds. On any pre-commit failure, roll back and remove newly moved/staged
files; never remove pre-existing files.

Filesystem and SQLite cannot share a true transaction, so make recovery
explicit: file moves happen before DB commit and every error path deletes only
files created by this request. A process crash may leave an unreferenced new
file; document a later startup cleanup as deferred rather than deleting unknown
files automatically.

Reload the in-memory gallery only after commit. A reload failure after commit
must never become a 500 that invites the client to repeat an already-committed
upload. Attempt one immediate bounded retry; if it still fails, log a sanitized
error and return the normal success status with additive
`gallery_reload_pending: true`. The already-bumped DB gallery version is the
source of convergence: each vision loop's existing version check retries the
reload, the next enrollment/recompute attempts it again, and process startup
loads the committed DB state. Return `gallery_reload_pending: false` when the
request-side reload succeeds. Keep the version bump inside the same SQL
transaction as rows/embedding.

**Verify**: forced DB failure leaves prior rows/embedding/version unchanged and
no request-created file remains. A forced post-commit reload failure returns
success with `gallery_reload_pending=true`, creates exactly one committed batch,
and converges when the existing version poll succeeds; a client retry is not
required.

### Step 5: Recompute existing records in place

Make `recompute_embedding` call the same analysis/persist path with zero new
files. It must retain each `employee_images.id` and `file_path`, update
usable/reason in place, never copy a file, and delete the embedding when usable
count is below minimum. Missing/unreadable files become explicit rejected rows;
they are not silently dropped.

**Verify**: recompute-twice test keeps identical IDs/paths and file count.

## Test plan

- API boundary limits: empty, count, per-file, total-body, pixel count, corrupt
  image, and valid batch.
- Analysis outcome split: ordinary no-face/quality rejection is recorded;
  structural model error aborts without mutation.
- State integrity: two uploads cumulative; recompute idempotent; below-minimum
  removes embedding; error keeps old rows/blob/version; request files cleaned.
- Additive result objects preserve original filenames and rejection reasons.
- All fixtures generated in test code and removed afterward.

## Done criteria

- [ ] Upload request/count/file/pixel limits are explicit and route-scoped.
- [ ] Embedding mean uses all currently stored usable images.
- [ ] Recompute preserves image IDs, paths, and file count.
- [ ] Below-minimum state has no active stale embedding.
- [ ] Failure leaves prior DB/gallery state and pre-existing files untouched.
- [ ] Post-commit gallery reload failure reports committed success plus pending
  convergence and cannot cause a duplicate-retry 500.
- [ ] Response retains frozen fields and includes additive per-file results.
- [ ] All four commands pass; plan 006 is marked `DONE`.

## STOP conditions

- Correct cumulative recompute requires a schema migration or per-image
  embeddings; report why before adding one.
- The model engine cannot run through plan 005’s blocking boundary.
- A failure path could delete or overwrite a pre-existing image.
- Product requirements demand batches above the explicit bounded policy.
- Any test needs private enrollment data.

## Maintenance notes

- At fewer than 50 employees and 5–10 images, full reanalysis is intentionally
  simpler than caching. Revisit only after measured enrollment latency hurts.
- Any future image-deletion endpoint must call this same recompute transaction.
