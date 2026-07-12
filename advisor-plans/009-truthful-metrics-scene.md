# Plan 009: Publish truthful attendance metrics and scene outcomes

> **Executor instructions**: Derive attendance truth from persisted events and
> preserve typed skip reasons. Do not patch misleading labels only in React.
> Update plan 009 in `advisor-plans/README.md` when complete.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/edge/crates/pksp-core apps/edge/crates/pksp-db apps/edge/crates/pksp-vision apps/web/src/app/page.tsx apps/web/src/components/EventTicker.tsx advisor-plans/README.md`

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: `advisor-plans/001-active-stack-verification.md`,
  `advisor-plans/005-frame-inference-scheduling.md`,
  `advisor-plans/008-live-websocket-state.md`, and
  `advisor-plans/011-app-timezone.md`
- **Category**: bug / data truth
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

The dashboard’s “Present” value is hardcoded to zero and “Events today” is only
an in-memory process counter. Scene/FSM outcomes are also collapsed into
`walkby` or `cooldown` even when those predicates are false. These are
CEO-facing claims about attendance and decision explainability; they must come
from persisted state and typed outcomes.

## Current state

- `VisionMetrics::default` starts `events_today` at zero at
  `apps/edge/crates/pksp-vision/src/lib.rs:521-534`.
- Every metrics message hardcodes `present_count: 0` at
  `apps/edge/crates/pksp-vision/src/lib.rs:796-809`.
- `commit_identity` returns `Option<(id,name,kind)>` and discards
  `SkipReason` at `apps/edge/crates/pksp-db/src/lib.rs:559-618`.
- `infer_frame` maps every `Ok(None)` to `cooldown` and every ineligible commit
  to `walkby` at `apps/edge/crates/pksp-vision/src/lib.rs:1078-1089`.
- `commit_eligible` returns false for approach, ignore, no zone, and actual
  trajectory walk-by at `apps/edge/crates/pksp-core/src/scene.rs:113-129`.
- `hud_state` already distinguishes approaching/tracking/ready/walkby at
  `apps/edge/crates/pksp-core/src/scene.rs:208-236`.
- `apps/web/src/app/page.tsx:53-57` renders both metrics as authoritative.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Core scene/FSM | `cd apps/edge && cargo test -p pksp-core --locked` | exit 0 |
| DB metrics/outcomes | `cd apps/edge && cargo test -p pksp-db --locked` | exit 0 |
| Vision projection | `cd apps/edge && cargo test -p pksp-vision --locked` | exit 0 |
| Web gates | `cd apps/web && npm run lint && npm run typecheck && npm test -- --run` | exit 0 |

## Scope

**In scope**:

- `apps/edge/crates/pksp-core/src/{fsm.rs,scene.rs}` and tests
- `apps/edge/crates/pksp-db/src/lib.rs` and tests
- `apps/edge/crates/pksp-vision/src/lib.rs` and tests
- `apps/web/src/app/page.tsx`, `apps/web/src/components/EventTicker.tsx`, and
  focused tests only if rendering must change
- `advisor-plans/README.md`

**Out of scope**: analytics/charts, manual review workflow, attendance FSM rule
changes, new WS fields, historical migration, and `apps/api/**`.

## Git workflow

- Branch: `codex/009-truthful-metrics-scene`
- Suggested commits: `Derive dashboard attendance metrics`, then
  `Preserve scene and FSM outcomes`.

## Steps

### Step 1: Add a persisted daily-metrics query

Add one `pksp-db` function returning `{events_today, present_count}` for a
configured local date. `events_today` is the count of persisted attendance
events for that date. `present_count` is the count of active employees whose
latest event that date is `check_in`; use deterministic ordering by event ID
when timestamps tie.

Use one clear SQL query/CTE or two simple queries. At 1–2 cameras and fewer than
50 employees, no cache or metrics service is justified.

Tests must cover empty day, in only, in then out, out then in, inactive employee,
and same-timestamp events ordered by ID.

**Verify**: DB tests pass.

### Step 2: Fill the existing WS metrics contract from SQLite

At the existing two-second metrics emission, derive current local date from
`APP_TIMEZONE` through plan 011's public fallible `local_date_str`, call the DB
metric function, and fill the existing fields. Do not duplicate timezone
parsing or maintain `events_today` as an authoritative in-memory counter;
remove it or treat it only as non-authoritative transient state. Restart and
midnight must naturally produce correct values.

Both camera loops may make this small query at accepted scale. Add
`// ponytail: 1-2 cameras; centralize only if measured DB load matters` rather
than adding a scheduler now.

On DB/query error, emit one rate-limited sanitized warning and skip that
two-second metrics message entirely. Do not broadcast a zero-filled fallback.
The browser retains the last successfully received metrics; before the first
success, both values are unavailable and render as an em dash. A later
successful interval clears the unavailable state without reconnecting.

**Verify**: a temp-DB vision test seeds events, creates a metrics message, and
asserts persisted values; restart/reset simulation returns the same result. A
forced query failure broadcasts no metrics object, never an invented zero, and
the next successful interval recovers.

### Step 3: Preserve typed commit/skip outcomes

Replace `Option` from `commit_identity` with a public typed outcome such as:

- `Committed { event_id, name, kind }`
- `Skipped(SkipReason)`

Return specific reasons for missing camera/employee, inactive employee,
cooldown, and no transition. Reuse `pksp-core::fsm::SkipReason`; add only missing
variants that affect displayed truth. Do not encode reasons as strings at the
DB boundary.

**Verify**: DB tests assert committed, cooldown, no-transition, inactive, and
missing-camera outcomes.

### Step 4: Map scene/FSM state accurately

In `infer_frame`:

- set `walkby` only when `trajectory_is_walkby` is true;
- retain `approaching`, `tracking`, or ready state when a track is merely not
  commit-eligible;
- map only `SkipReason::Cooldown` to `cooldown`;
- leave no-transition/inactive/missing outcomes as their truthful prior HUD
  state, optionally logging a sanitized reason for operators;
- keep committed state only after a persisted event.

Extend pure scene tests and one vision projection test for every branch.

**Verify**: core and vision tests pass; no false `walkby`/`cooldown` mapping
remains in source.

### Step 5: Keep UI wording literal

The dashboard may continue rendering the same metric fields once truthful. In
the event ticker, show state/event text only from actual messages; do not infer
attendance from detections. If a metric query errors, retain the prior value;
if no successful value has ever arrived, show an em dash rather than zero. A
real persisted zero remains visually distinct from unavailable.

**Verify**: web test covers unknown/unavailable metrics and real zero separately.

## Test plan

- DB daily metric state matrix and deterministic ties.
- Typed commit outcome matrix.
- Pure scene eligibility versus HUD state matrix.
- Vision mapping tests for approach, actual walk-by, cooldown, no transition,
  and commit.
- Web real-zero versus unavailable rendering.

## Done criteria

- [x] Dashboard metric fields are derived from persisted events/local date.
- [x] Restart and midnight cannot reset today’s truth to an invented zero.
- [x] Commit skip reason is typed across DB/vision boundary.
- [x] Only actual trajectory rejection is `walkby`; only cooldown is `cooldown`.
- [x] All four commands pass; plan 009 is marked `DONE`.

## STOP conditions

- “Present” has a different approved business definition than latest event is
  check-in; obtain the exact rule before coding.
- Correct ordering cannot be made deterministic with existing event IDs.
- The change requires altering frozen WS field names or FSM transition rules.

## Maintenance notes

- Add metrics through existing persisted queries until scale proves otherwise.
- Future manual overrides must feed the same source of attendance truth rather
  than adding a second counter.
