# Plan 011: Apply `APP_TIMEZONE` to dates, times, and CSV

> **Executor instructions**: Keep UTC timestamps on the wire and use the
> configured IANA timezone for calendar selection and human display. Use native
> chrono/Intl support; do not add a date library. Update plan 011 in the index.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/edge/crates/pksp-api/src/lib.rs apps/edge/crates/pksp-api/src/routes.rs apps/edge/crates/pksp-api/tests/timezone.rs apps/edge/crates/pksp-db/src/lib.rs apps/web/src/app/attendance apps/web/src/lib apps/web/src/hooks/useHealth.ts apps/web/src/hooks/useHealth.test.ts advisor-plans/README.md`

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: `advisor-plans/001-active-stack-verification.md` and
  `advisor-plans/010-rust-employee-csv-contracts.md`
- **Category**: bug / timezone
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

The attendance page chooses “today” in browser UTC and displays UTC strings by
slicing them, while attendance days are configured by `APP_TIMEZONE`. Around
midnight, operators can open the wrong day and see shifted punch times. CSV and
backend defaults must follow the same configured calendar.

## Current state

- `todayISO()` is `new Date().toISOString().slice(0,10)` at
  `apps/web/src/app/attendance/page.tsx:8-10`.
- First/last times use `value.slice(11,19)` at
  `apps/web/src/app/attendance/page.tsx:146-151`.
- Rust `Settings` already parses `APP_TIMEZONE` at
  `apps/edge/crates/pksp-db/src/lib.rs:91` and attendance commit uses
  `local_date_str`.
- `/api/health` does not expose timezone at
  `apps/edge/crates/pksp-api/src/routes.rs:22-71`.
- Daily/CSV routes default omitted dates with `Utc::now().date_naive()` at
  `routes.rs:300-320`.
- `build_daily` returns UTC ISO strings with `Z`; preserve that wire contract.
- `plans/04-attendance-logic.md` requires UTC storage/wire values, display in
  `APP_TIMEZONE`, and local-date grouping.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Rust timezone tests | `cd apps/edge && cargo test -p pksp-db -p pksp-api --locked timezone` | exit 0 |
| Web date/health tests | `cd apps/web && npm test -- --run dateTime useHealth attendance-timezone` | exit 0 |
| Web gates | `cd apps/web && npm run lint && npm run typecheck` | exit 0 |
| Full active tests | `cd apps/edge && cargo test --locked && cd ../web && npm test -- --run` | exit 0 |

## Scope

**In scope**:

- `apps/edge/crates/pksp-api/src/{lib.rs,routes.rs}` and
  `apps/edge/crates/pksp-api/tests/timezone.rs` (create)
- `apps/edge/crates/pksp-db/src/lib.rs` and tests
- `apps/web/src/lib/{types.ts,dateTime.ts,dateTime.test.ts}` (`dateTime.ts` and
  `dateTime.test.ts` create)
- `apps/web/src/hooks/useHealth.ts` and
  `apps/web/src/hooks/useHealth.test.ts` (create)
- `apps/web/src/app/attendance/page.tsx` and
  `apps/web/src/app/attendance/attendance-timezone.test.tsx` (create)
- `advisor-plans/README.md`

**Out of scope**: timestamp migration, changing UTC storage/wire shape, locale
preferences beyond an explicit stable display locale, a date library, charts,
raw-event UI, and `apps/api/**`.

## Git workflow

- Branch: `codex/011-app-timezone`
- Commit message: `Apply configured attendance timezone`

## Steps

### Step 1: Expose one safe public setting

Make the current DB helper public and fallible as
`pksp_db::local_date_str(now, tz_name) -> Result<String>`: a valid IANA name
returns its local calendar date; an invalid name returns an error and never
falls back to UTC. At the first line of `pksp_api::serve`, before `Arc`
construction, `connect_pool`, model load, or media startup, call the helper once
with `settings.app_timezone` to validate the server configuration. Keep
`Settings::from_env() -> Settings` and the CLI migrate path unchanged; this is
serve-time validation, not a workspace-wide settings API redesign.

Add `timezone` to `/api/health`, sourced from that validated
`APP_TIMEZONE`. Expose no other settings or source URLs. Add a typed
`HealthResponse`/`HealthCamera` in `apps/web/src/lib/types.ts`.

Create `useHealth` as the one shared public-health fetch used by later media and
camera-wall plans. It should use native fetch, retry with a capped interval,
replace data atomically on success, expose loading/error/data, and cancel its
timer/request on unmount. No SWR/React Query/context.

**Verify**: `timezone.rs` proves invalid configuration fails before a test DB
file, model, listener, or child is created; the health test returns the valid
configured timezone. The hook fake-timer test recovers after an initial failure
and stops on unmount.

### Step 2: Add native timezone helpers

In `dateTime.ts`, implement pure helpers using `Intl.DateTimeFormat` and
`formatToParts`:

- `calendarDateInZone(Date, timeZone) -> YYYY-MM-DD`;
- `timeInZone(isoUtc, timeZone) -> HH:MM:SS`;
- optional combined formatter for plan 017.

Do not depend on locale string ordering; build output from named parts. Invalid
timestamps/timezones return a controlled error or em dash, never silently use
browser local time.

Tests must cover UTC, Asia/Tehran, a negative-offset zone, and instants on both
sides of local midnight.

**Verify**: dateTime tests pass under a fixed test clock.

### Step 3: Select the first attendance date only after health

Remove the UTC `todayISO`. While health is loading/retrying, render a clear
attendance-loading state and do not issue a daily query with a guessed date.
Once timezone is available, initialize date exactly once from the current
instant in that zone; user-selected dates must not be overwritten by later
health refreshes.

Show the active timezone beside the native date input.

**Verify**: component test asserts the first daily request uses the configured
calendar date and a later health refresh does not reset user selection.

### Step 4: Format human times in the configured zone

Replace string slicing with `timeInZone` for first/last values. Keep raw values
as UTC ISO strings in state and types. Invalid/null values render em dash and do
not crash the table.

**Verify**: component test renders known UTC instants as expected in two zones.

### Step 5: Align backend omitted dates and CSV display

For omitted daily/CSV `date`, call the now-public fallible
`local_date_str(Utc::now(), APP_TIMEZONE)` rather than UTC date and propagate
its typed error. Change the internal CSV call to accept the configured timezone
and convert first/last UTC strings to local human times before field encoding.
Preserve API JSON UTC values and the requested local date column.

Add Rust tests around a timezone boundary where UTC and local calendar dates
differ, plus invalid configured timezone validation.

**Verify**: focused Rust timezone tests pass; CSV escaping tests from plan 010
remain green.

## Test plan

- Rust health projection, omitted-date behavior, CSV local time, invalid zone.
- Pure JS date/time helpers at positive/negative offsets and midnight.
- Attendance initial load waits for health, uses configured date, preserves
  manual selection, and formats UTC wire values.
- Fake time must be restored after each test.

## Done criteria

- [x] Health exposes only the validated timezone addition.
- [x] Default day comes from `APP_TIMEZONE` in backend and browser.
- [x] UTC wire values remain unchanged; UI/CSV human times use configured zone.
- [x] No string slicing or browser-local fallback formats attendance time.
- [x] All four commands pass; plan 011 is marked `DONE`.

## STOP conditions

- Existing event values are not valid UTC ISO strings with an explicit zone.
- `APP_TIMEZONE` is not a valid IANA timezone.
- A consumer requires a different CSV time contract; report before changing
  headers or wire values.
- Health becomes protected or omits camera/timezone public projection.

## Maintenance notes

- Reuse `useHealth` and `dateTime.ts` in plans 013, 016, and 017.
- Never reintroduce `toISOString().slice(...)` for a business calendar date.
