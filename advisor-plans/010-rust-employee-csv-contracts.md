# Plan 010: Complete the Rust employee and CSV contracts

> **Executor instructions**: Keep employee deletion reversible and preserve the
> existing response shapes/CSV columns. Add focused contract tests before route
> or serialization changes. Update plan 010 in the index when complete.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/edge/crates/pksp-api apps/edge/crates/pksp-db apps/edge/crates/pksp-core apps/edge/migrations advisor-plans/README.md`

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: `advisor-plans/001-active-stack-verification.md` and
  `advisor-plans/003-security-containment.md`
- **Category**: bug / API contract
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

The frozen Rust employee surface includes authenticated DELETE, but the active
router registers only GET/PATCH. Active/name changes also do not reload the
recognition gallery. CSV export interpolates raw fields, so punctuation can
shift columns and spreadsheet formulas can execute when an operator opens an
export. Both fixes are small and have deterministic tests.

## Current state

- `apps/edge/crates/pksp-api/src/lib.rs:166-169` registers GET/PATCH only for
  `/api/employees/{id}`.
- `update_employee` issues separate updates and returns `employee_dict` at
  `apps/edge/crates/pksp-api/src/routes.rs:158-193`; it does not bump/reload the
  gallery after name or active-state changes.
- `employee_embeddings` and images cascade only on physical employee deletion
  in `apps/edge/migrations/001_init.sql`; this plan must not physically delete.
- `daily_csv` creates rows with one raw `format!` call at
  `apps/edge/crates/pksp-db/src/lib.rs:732-757`.
- Header order is defined by `daily_csv_headers` in
  `apps/edge/crates/pksp-core/src/daily.rs:122-135` and must remain unchanged.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Employee contracts | `cd apps/edge && cargo test -p pksp-api --locked employee` | exit 0 |
| DB CSV | `cd apps/edge && cargo test -p pksp-db --locked csv` | exit 0 |
| Core | `cd apps/edge && cargo test -p pksp-core --locked` | exit 0 |
| Full Rust | `cd apps/edge && cargo test --locked` | exit 0 |

## Scope

**In scope**:

- `apps/edge/crates/pksp-api/src/{lib.rs,routes.rs}` and
  `apps/edge/crates/pksp-api/tests/employee_contract.rs` (create)
- `apps/edge/crates/pksp-db/src/lib.rs` and tests
- `apps/edge/crates/pksp-core/src/daily.rs` only if the CSV field helper belongs
  beside the fixed header contract
- `advisor-plans/README.md`

**Out of scope**: physical deletion, image deletion, retention policy, UI,
column additions/reordering, import, a CSV library, schema migration, and
`apps/api/**`.

## Git workflow

- Branch: `codex/010-rust-employee-csv-contracts`
- Suggested commits: `Add reversible employee deletion`, then
  `Emit spreadsheet-safe attendance CSV`.

## Steps

### Step 1: Write route and gallery-invalidation tests

Using plan 001’s API harness, write failing cases for missing auth, existing
employee, repeat DELETE, unknown employee, and PATCH name/active changes. Assert
that deactivation preserves employee/image/embedding/attendance row counts and
that the live gallery no longer contains an inactive employee. Assert gallery
version changes only when persisted employee state actually changes.

Recommended contract: DELETE returns 200 with the normal employee JSON and
`is_active:false`; repeating it is idempotent 200; unknown ID is 404.

**Verify**: focused tests fail only because DELETE/gallery refresh are missing.

### Step 2: Make employee updates atomic and gallery-aware

Add one `pksp-db` operation for supported employee fields or, at minimum, one
transactional active-state operation. It must update `updated_at` and bump
gallery version in the same transaction when name or active state changes.
Unchanged values must not bump the version.

Have PATCH use the DB operation and reload the in-memory gallery after commit.
Name changes must refresh gallery labels; deactivate/reactivate must remove/add
matching eligibility. Preserve employee code immutability.

Treat DB commit and in-memory refresh as separate outcomes. After commit, make
one bounded immediate reload retry. If it still fails, emit a rate-limited
sanitized error and return the committed employee response—not a 500 that
misstates the PATCH and invites a duplicate retry. The transaction's gallery
version bump makes the existing vision-loop version poll converge, and process
startup also reloads from SQLite. A repeated DELETE/PATCH must attempt
convergence even when the requested persisted value is already current.

**Verify**: PATCH contract and gallery tests pass. A forced post-commit reload
failure returns the committed employee once, leaves the bumped version visible,
and converges on the next version poll.

### Step 3: Add idempotent soft DELETE

Register `.delete(routes::deactivate_employee)` on the existing route. Reuse
the transactional active-state operation with `false`; do not duplicate SQL.
Reload gallery only when the state changed. Preserve all rows/files/history.

**Verify**: API employee tests pass, including repeat-delete and row-count
assertions.

### Step 4: Encode every CSV field correctly

Add a private, fixed-purpose field encoder:

- double embedded quotes;
- quote fields containing comma, quote, CR, or LF;
- preserve ordinary UTF-8 and empty strings;
- for employee-controlled string fields (`employee_code`, `full_name`,
  `department`), prefix a literal apostrophe when the first non-whitespace
  character is `=`, `+`, `-`, or `@`.

Pass every string cell through it. Keep numeric count/duration cells numeric,
the exact ten-column order, content type, filename, and newline convention.
Do not add a general serialization layer or dependency.

**Verify**: CSV tests cover plain, Unicode, comma, quote, CR/LF, empty/null,
formula-leading, and whitespace-before-formula values.

## Test plan

- API: auth, existing/unknown/repeated DELETE, PATCH name/active, gallery
  version/reload—including post-commit reload failure—and preservation of
  related rows.
- CSV exact outputs for escaping/neutralization; assert ten logical fields and
  unchanged headers.
- Full Rust suite after focused tests.

## Done criteria

- [x] Authenticated DELETE is registered and reversible/idempotent.
- [x] PATCH/DELETE update gallery version transactionally and reload after
  commit; inactive identities cannot match.
- [x] No biometric/history row or file is physically deleted.
- [x] CSV fields are RFC-4180 escaped and spreadsheet formulas neutralized.
- [x] Header order and response metadata are unchanged.
- [x] All four commands pass; plan 010 is marked `DONE`.

## STOP conditions

- A currently approved contract specifies different DELETE status/body/repeat
  semantics.
- Any implementation physically deletes employee-related data.
- Active-state update and gallery-version bump cannot share one transaction.
- A consumer requires formula-leading strings to remain executable; report the
  security tradeoff rather than silently removing neutralization.

## Maintenance notes

- Any future image delete/offboarding workflow must call the same gallery
  invalidation path.
- Keep the small CSV helper until the schema becomes dynamic or import is added;
  only then reconsider a dependency.
