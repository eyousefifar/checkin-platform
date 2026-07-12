# Plan 017: Add attendance explainability before analytics

> **Executor instructions**: Reuse the existing raw-events endpoint and native
> table expansion. Fetch once per selected day, group locally, and add no charts
> or reporting framework. Update plan 017 in the index.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/web/src/app/attendance/page.tsx apps/web/src/app/page.tsx apps/web/src/components/EventTicker.tsx apps/web/src/lib/types.ts apps/web/src/lib/dateTime.ts apps/edge/crates/pksp-api/src/routes.rs advisor-plans/README.md`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: `advisor-plans/009-truthful-metrics-scene.md`,
  `advisor-plans/011-app-timezone.md`, and
  `advisor-plans/016-dynamic-camera-wall.md`
- **Category**: direction / UX
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

Operators currently see only daily aggregates, so “incomplete” or “anomaly”
rows cannot be explained without querying the database. The existing API already
returns raw events with timestamp, camera, kind, and score. A compact expandable
row adds auditability with much more product value than generic charts.

## Current state

- `AttendancePage` fetches only `/api/attendance/daily` and renders aggregate
  columns at `apps/web/src/app/attendance/page.tsx:12-168`.
- The existing authenticated endpoint supports
  `/api/attendance/events?date=&employee_id=` and returns up to 500 rows at
  `apps/edge/crates/pksp-api/src/routes.rs:336-375`.
- `DailyRow` exists in `apps/web/src/lib/types.ts`; there is no raw-event type.
- `EventTicker` receives timestamp/camera in `AttendanceMsg` but displays only
  name, kind, and score at
  `apps/web/src/components/EventTicker.tsx:12-41`.
- Plan 011 supplies configured-timezone helpers; plan 009 makes metrics/outcome
  labels truthful.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Attendance tests | `cd apps/web && npm test -- --run attendance` | exit 0 |
| Ticker tests | `cd apps/web && npm test -- --run EventTicker` | exit 0 |
| Web gates | `cd apps/web && npm run lint && npm run typecheck` | exit 0 |
| Build | `cd apps/web && npm run build` | exit 0 |

## Scope

**In scope**:

- `apps/web/src/lib/types.ts`
- `apps/web/src/app/attendance/page.tsx` and focused tests
- `apps/web/src/app/page.tsx` only to pass the already-loaded health timezone to
  the ticker after plan 016's dashboard changes
- `apps/web/src/components/EventTicker.tsx` and focused tests
- `apps/web/src/lib/dateTime.ts` only for numeric WS timestamp formatting
- Rust events route/test only if its current typed shape is missing a documented
  field already selected by SQL
- `advisor-plans/README.md`

**Out of scope**: charts, dashboards, reports, pagination, manual override,
review workflow, event editing/deletion, a table library, and `apps/api/**`.

## Git workflow

- Branch: `codex/017-attendance-explainability`
- Commit message: `Add attendance event explainability`

## Steps

### Step 1: Type the raw-event contract

Add `RawAttendanceEvent` with ID, nullable employee ID, camera ID, kind, optional
score, UTC timestamp, and local date. Keep field names identical to the existing
endpoint. Do not widen it with employee/camera objects.

**Verify**: typecheck passes and a fixture satisfies the type without casts.

### Step 2: Fetch daily rows and raw events once per date

On selected-date load, request daily rows and all raw events for that date in
parallel. At the existing 500-row cap and MVP scale, group events by employee ID
with `useMemo`; do not issue one request per expanded row. Treat failures
separately so daily rows can remain visible if raw events fail.

Discard/ignore stale responses when the selected date changes before requests
complete, using `AbortController` or a request generation counter. No data-fetch
library.

**Verify**: tests assert exactly two requests per date, no per-row request,
correct grouping, and stale date response cannot overwrite current state.

### Step 3: Add an accessible expandable detail row

Add an explicit button per aggregate row with `aria-expanded` and an associated
detail row/region. When open, show events oldest-to-newest with configured local
time, kind text, camera ID, and score or em dash. Show a precise per-row empty or
raw-event-error state; never pair an error with “no events.”

Use existing table/Tailwind primitives. Do not add a generic DataTable or modal.

**Verify**: tests open/close by accessible name and assert order, fields,
timezone formatting, empty, and error states.

### Step 4: Improve the live ticker’s audit value

Display configured local time and camera ID beside each live attendance event.
After plan 016, the dashboard already owns plan 011's retrying `useHealth`
result; pass `health.data?.timezone` into `EventTicker` as an explicit optional
prop. Do not call `useHealth` again inside the ticker. Before health succeeds,
render the event's time as an em dash while retaining its other fields; never
fall back to browser local time. Extend `dateTime.ts` to accept epoch seconds
through a tiny wrapper if needed. Make the list a polite live region for new
attendance events only; do not announce detection updates.

**Verify**: dashboard/ticker tests prove the health timezone prop is passed,
unavailable timezone renders an em dash, a valid timezone renders the expected
local time, and name/kind/camera/score plus live-region semantics remain.

## Test plan

- Parallel load/grouping and stale response protection.
- Expand/collapse semantics and raw-event field/order/time rendering.
- Partial raw-event failure while daily aggregate remains usable.
- Ticker time/camera and polite event announcement.

## Done criteria

- [ ] One raw-event request per selected date; no N+1 row fetching.
- [ ] Every daily row can explain its underlying events on demand.
- [ ] Times use configured timezone; raw UTC values remain unchanged.
- [ ] Empty/error/detail states are explicit and accessible.
- [ ] Live ticker includes event time and camera without announcing detections.
- [ ] All four commands pass; plan 017 is marked `DONE`.

## STOP conditions

- Raw event timestamps are not valid explicit UTC values.
- A normal accepted day can exceed the endpoint’s 500-row cap; pagination must
  be designed before claiming complete explainability.
- Product requires editing/overriding events rather than viewing them.

## Maintenance notes

- Add charts only after operators use raw details and name a repeated question
  that a chart answers.
- If event volume grows, paginate the existing endpoint before changing client
  grouping.
