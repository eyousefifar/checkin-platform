# Plan 018: Add employee editing and reversible deactivation

> **Executor instructions**: Reuse the existing PATCH contract. Deactivation is
> reversible, so use native controls and one confirmation—no modal/form library
> and no destructive UI. Update plan 018 in the index.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/web/src/app/employees/[id]/page.tsx apps/web/src/app/employees/[id]/employee-detail.test.tsx apps/web/src/lib/types.ts apps/edge/crates/pksp-api/src/routes.rs advisor-plans/README.md`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `advisor-plans/010-rust-employee-csv-contracts.md` and
  `advisor-plans/012-enrollment-ux.md`
- **Category**: direction / UX
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

The Rust endpoint already updates full name, department, and active state, but
the employee detail page exposes only enrollment actions. Operators currently
need direct API/DB access to correct a typo or deactivate a departed employee.

## Current state

- `EmployeeUpdate` accepts optional `full_name`, `department`, and `is_active`
  at `apps/edge/crates/pksp-api/src/routes.rs:151-193`.
- The route returns the normal employee JSON after update.
- `EmployeeDetailPage` displays metadata as text and provides only upload/
  recompute controls at
  `apps/web/src/app/employees/[id]/page.tsx:9-138`.
- Employee code is unique and not accepted by PATCH; preserve immutability.
- Plan 010 ensures active/name changes invalidate/reload the gallery.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Detail tests | `cd apps/web && npm test -- --run employee-detail` | exit 0 |
| Web tests | `cd apps/web && npm test -- --run` | exit 0 |
| Web gates | `cd apps/web && npm run lint && npm run typecheck` | exit 0 |
| Build | `cd apps/web && npm run build` | exit 0 |

## Scope

**In scope**:

- `apps/web/src/app/employees/[id]/page.tsx` and
  `apps/web/src/app/employees/[id]/employee-detail.test.tsx` (create)
- `apps/web/src/lib/types.ts` only if an update type is useful
- `advisor-plans/README.md`

**Out of scope**: employee-code editing, hard delete, image delete, retention,
bulk actions, optimistic updates, form/modal library, new API route, and
`apps/api/**`.

## Git workflow

- Branch: `codex/018-employee-edit-deactivate`
- Commit message: `Add employee edit and deactivate controls`

## Steps

### Step 1: Add a small native edit form

Initialize controlled full-name, department, and active values whenever the
loaded employee changes. Render employee code read-only. On save, PATCH only
the three supported fields and replace local employee state with the returned
object. Disable save while pending and prevent upload/recompute collision through
the page’s existing `busy` state or a minimal separate operation flag.

Do not add schema/form libraries or optimistic state.

**Verify**: test edits name/department and asserts the exact PATCH body and
returned values rendered.

### Step 2: Confirm only active-to-inactive transitions

If save changes active from true to false, call native `window.confirm` with
plain language that recognition stops but records/images remain. Cancel sends no
request and restores the active control. Reactivation and metadata-only edits do
not prompt.

Do not use DELETE from the UI; PATCH makes the reversible intent explicit.

**Verify**: tests cover canceled/confirmed deactivation, reactivation, and no
confirmation for name-only edit.

### Step 3: Announce and preserve outcomes

Use `role="status"` for save success and `role="alert"` for failure. On error,
retain the last server-confirmed employee data and the user’s form values so
they can retry; do not discard enrollment results or navigate away.

**Verify**: failure test asserts current metadata remains visible, form values
remain editable, and error is announced.

## Test plan

- Exact successful PATCH body/returned-state replacement.
- Cancel/confirm deactivate and reactivate.
- Metadata edit without confirmation.
- API error preserves data/form and exposes alert.
- Employee code has no editable control.

## Done criteria

- [ ] Name and department are editable through existing PATCH.
- [ ] Active state is reversible and deactivation requires native confirmation.
- [ ] Employee code, images, embeddings, and history are not destructively
  modified by the UI.
- [ ] Success/failure is accessible and retryable.
- [ ] All four commands pass; plan 018 is marked `DONE`.

## STOP conditions

- PATCH no longer returns a complete employee object.
- Deactivation acquires business rules not represented by a boolean active
  field.
- Product requires editable employee codes or destructive offboarding.
- Detail-page drift conflicts with plan 012; rebase rather than duplicating
  enrollment state.

## Maintenance notes

- Keep destructive retention/offboarding separate until policy defines what
  must be deleted and when.
- If role-based permissions arrive, gate edit/deactivate server-side first; UI
  hiding is not authorization.
