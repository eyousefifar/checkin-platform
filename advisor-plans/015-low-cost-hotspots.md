# Plan 015: Remove low-cost frontend and employee-list hot spots

> **Executor instructions**: These are measured/code-evident hot paths with
> small fixes. Do not add caching, pagination, debounce infrastructure, or a
> canvas abstraction. Update plan 015 in the index when complete.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/web/src/components/FaceHudCanvas.tsx apps/web/src/app/employees/page.tsx apps/edge/crates/pksp-db/src/lib.rs advisor-plans/README.md`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `advisor-plans/001-active-stack-verification.md`
- **Category**: performance
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

Every detections message resets/reallocates the HUD canvas backing store, and
employee search makes an uncancelled request per keystroke. The Rust employee
list also issues `1 + 3N` SQLite statements by calling the detail query for each
row. At the documented sub-50 scale, the simplest fixes are resize-on-change,
one client fetch/local filter, and three batched DB queries.

## Current state

- `FaceHudCanvas` assigns `canvas.width` and `canvas.height` on every effect at
  `apps/web/src/components/FaceHudCanvas.tsx:17-24`; `faces` is an effect
  dependency at `:65`.
- `EmployeesPage.load` depends on `q` and requests `/api/employees?q=...` on
  every change at `apps/web/src/app/employees/page.tsx:14-30`.
- `pksp_db::list_employees` fetches employee rows then calls
  `employee_dict(pool,id)` in its loop at
  `apps/edge/crates/pksp-db/src/lib.rs:438-458`.
- `employee_dict` performs employee, images, and embedding queries at
  `apps/edge/crates/pksp-db/src/lib.rs:460-507`.
- Product scale is fewer than 50 employees; preserve the existing unpaginated
  API contract.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Web hotspot tests | `cd apps/web && npm test -- --run performance` | exit 0 |
| Web gates | `cd apps/web && npm run lint && npm run typecheck` | exit 0 |
| DB list tests | `cd apps/edge && cargo test -p pksp-db --locked list_employees` | exit 0 |
| Full active tests | `cd apps/edge && cargo test --locked && cd ../web && npm test -- --run` | exit 0 |

## Scope

**In scope**:

- `apps/web/src/components/FaceHudCanvas.tsx` and
  `apps/web/src/components/FaceHudCanvas.performance.test.tsx` (create)
- `apps/web/src/app/employees/page.tsx` and
  `apps/web/src/app/employees/page.performance.test.tsx` (create)
- `apps/edge/crates/pksp-db/src/lib.rs` and focused tests
- `advisor-plans/README.md`

**Out of scope**: pagination, cache/SWR/React Query, debounce/cancellation
framework, canvas library/RAF redesign, schema/index migration, API response
changes, and `apps/api/**`.

## Git workflow

- Branch: `codex/015-low-cost-hotspots`
- Commit message: `Remove avoidable UI and employee query work`

## Steps

### Step 1: Resize the canvas only when physical size changes

Compute integer physical width/height from CSS size and device-pixel ratio. Set
`canvas.width/height` only when either differs from the current values; keep
style dimensions synchronized. Continue clearing/drawing on every faces update
with the existing immediate canvas path.

Do not split drawing into a new renderer or add animation state.

**Verify**: component test rerenders different faces at unchanged dimensions
and observes no backing-size write; changing size or DPR writes once and still
draws.

### Step 2: Fetch employees once and filter locally

Load `/api/employees` on mount only. Derive filtered rows with `useMemo` from
query plus the loaded list, case-insensitively matching employee code, full
name, and department. Add a visible search label while touching the input.
Preserve loading/error/empty behavior and do not refetch on keystrokes.

**Verify**: test types multiple characters, observes one API request, and finds
matches in all three fields with stable result ordering.

### Step 3: Batch employee-list metadata in Rust

Keep `employee_dict` unchanged for detail routes. In `list_employees`:

1. fetch employees once and apply existing optional query filtering in memory;
2. fetch image rows for the selected employees in one query;
3. fetch embedding rows for the selected employees in one query;
4. group the latter two by employee ID in `HashMap`s and build the same JSON
   shape/order.

For the sub-50 bound, selecting all image/embedding rows then grouping is
acceptable and clearer than dynamic placeholder SQL. Add
`// ponytail: bounded <50 employee list; paginate/batch IDs only if scope grows`.
Do not call `employee_dict` from the list loop.

**Verify**: DB tests with zero, one, and multiple employees/images/embeddings
assert exact response counts/shape/search ordering; static search confirms the
list function no longer calls `employee_dict`.

## Test plan

- Canvas resize/no-resize plus draw behavior.
- Employee page one fetch and local code/name/department filters.
- DB list response parity for missing/multiple images and embeddings, inactive
  values, query filtering, and alphabetical order.

## Done criteria

- [x] Detection updates at unchanged dimensions do not reallocate canvas.
- [x] Employee typing performs no network request after initial load.
- [x] Rust list path performs a fixed three-query shape, not `1+3N`.
- [x] Public JSON and ordering remain unchanged.
- [x] All four commands pass; plan 015 is marked `DONE`.

## STOP conditions

- The employee endpoint is now paginated/capped or supported scale materially
  exceeds 50; local filtering/full-row grouping must be reconsidered.
- A canvas optimization changes box alignment or DPR scaling.
- Query-count reduction requires a public response or schema change.

## Maintenance notes

- Measure before adding more caching/rendering machinery.
- If employee scale grows, server pagination is the upgrade path; do not bolt a
  cache onto the current full-list contract.
