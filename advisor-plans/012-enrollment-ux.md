# Plan 012: Make employee creation and photo review recoverable

> **Executor instructions**: Employee metadata creation is the commit point.
> Once it succeeds, never submit it again because photo upload failed. Reuse the
> additive per-file result from plan 006. Update plan 012 in the index.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/web/src/app/employees apps/web/src/lib/types.ts advisor-plans/README.md`

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: `advisor-plans/001-active-stack-verification.md` and
  `advisor-plans/006-enrollment-integrity.md`
- **Category**: bug / UX
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

Metadata POST succeeds before image upload, but one catch block reports the
whole operation failed and leaves the creation form active. Retrying can collide
with the already-created employee. Upload feedback also collapses per-file
results into a sentence, making poor enrollment hard to correct.

## Current state

- `NewEmployeePage.onSubmit` performs metadata POST then optional multipart POST
  inside one try/catch at `apps/web/src/app/employees/new/page.tsx:18-51`.
- Navigation occurs only after upload succeeds; the success message is discarded
  immediately by navigation.
- Detail upload response typing drops filenames and renders only joined reasons
  at `apps/web/src/app/employees/[id]/page.tsx:31-50`.
- Existing detail image rows show numeric IDs/reasons without selected-file
  context at `:93-104`.
- Plan 006 preserves aggregate response fields and adds
  `results: [{filename, usable, reason}]` plus
  `gallery_reload_pending` for a committed upload awaiting in-memory
  convergence.
- `plans/07-frontend-ui.md` calls for 5–10 photos and usable/rejected feedback.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Focused UI tests | `cd apps/web && npm test -- --run employees` | exit 0 |
| Lint/typecheck | `cd apps/web && npm run lint && npm run typecheck` | exit 0 |
| Web suite | `cd apps/web && npm test -- --run` | exit 0 |
| Build | `cd apps/web && npm run build` | exit 0 |

## Scope

**In scope**:

- `apps/web/src/lib/types.ts`
- `apps/web/src/app/employees/new/page.tsx`
- `apps/web/src/app/employees/[id]/page.tsx`
- colocated focused tests
- `advisor-plans/README.md`

**Out of scope**: new API endpoint, transactionally coupling metadata/images,
drag-and-drop framework, persistent gallery viewer, image deletion, form/state
library, and `apps/api/**`.

## Git workflow

- Branch: `codex/012-enrollment-ux`
- Commit message: `Make employee enrollment recoverable`

## Steps

### Step 1: Share the exact upload response type

Add one `EnrollmentResult` type containing frozen aggregate fields, rejected
items, plan 006’s additive per-file results, and `gallery_reload_pending`. Use
it in both pages. Do not duplicate inline response types.

**Verify**: typecheck passes.

### Step 2: Separate creation from optional upload state

In `NewEmployeePage`, store the returned employee ID immediately. After that
state is set, disable/replace the metadata form so no retry can issue a second
POST. Run upload in a separate try/catch:

- create failure: stay on form and show blocking error;
- create success/no files: show created state and link to detail;
- create success/upload success: show structured results and detail link;
- create success/upload committed with gallery reload pending: keep the success
  state, explain that recognition is catching up, and never resubmit files;
- create success/upload failure: state explicitly that employee exists and
  images can be retried from the detail link.

Do not put raw error text in a URL or auto-resubmit. Staying on the completion
state is simpler than cross-route transient state.

**Verify**: test proves upload failure performs exactly one metadata POST,
preserves ID/link, and disables creation controls.

### Step 3: Preview and report selected files safely

For selected local files, create short-lived object URLs and render filename
plus small preview. Revoke every URL on selection change and unmount. After
upload, render each result beside the matching filename with explicit usable or
rejection reason text; do not rely on color alone.

Keep previews local in browser memory and never persist/log them.

**Verify**: test stubs `URL.createObjectURL/revokeObjectURL`, proves cleanup, and
renders accepted/rejected filenames/reasons.

### Step 4: Give detail-page retries the same feedback

Use `EnrollmentResult` in the detail upload. Add a visible label for the file
input, clear prior error on retry, render per-file results, and refresh employee
metadata only after a successful upload. Keep recompute as a separate action
and show its aggregate result without pretending it uploaded new files.

If `gallery_reload_pending` is true, present it as committed-but-converging
status, not an upload error or retry action. A later employee refresh may clear
the message when embedding readiness is visible; do not poll a new endpoint.

Use `role="status"` for success/progress and `role="alert"` for blocking errors.

**Verify**: detail tests cover successful refresh, rejected file display,
failure preserving current employee, and accessible feedback roles.

## Test plan

- Create failure; create-only success; create+upload success; upload failure
  after create; prevention of duplicate metadata POST.
- Per-file accepted/rejected rendering and object URL cleanup.
- Detail retry success/failure/recompute separation and committed-pending state.
- All API calls mocked at the existing `api` module boundary.

## Done criteria

- [x] A successful metadata POST can never be repeated by retrying photo upload.
- [x] Every state offers a working link to the created employee.
- [x] Selected photos and per-file usable/reason results are understandable.
- [x] Object URLs are revoked; no face preview is persisted or logged.
- [x] Feedback has appropriate status/alert semantics.
- [x] All four commands pass; plan 012 is marked `DONE`.

## STOP conditions

- Plan 006’s response lacks stable filenames/per-file results.
- Product now requires metadata and images to be one atomic endpoint.
- Previewing local files would violate an approved privacy requirement.
- Recovery requires storing server errors or image data in URL/localStorage.

## Maintenance notes

- If image deletion is later added, reuse the same result/recompute presentation.
- Keep the two API operations explicit; a new transactional endpoint is not
  justified at current scale.
