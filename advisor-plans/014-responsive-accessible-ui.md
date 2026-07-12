# Plan 014: Restore responsive and accessible operational UI

> **Executor instructions**: Preserve the BMW M visual language and desktop/TV
> information architecture. Use native HTML/CSS first. Schedule this after
> functional web plans to avoid repeated conflicts. Update plan 014 in the index.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/web/src/components apps/web/src/app apps/web/src/styles apps/web/src/app/globals.css DESIGN.md advisor-plans/README.md`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: `advisor-plans/001-active-stack-verification.md`,
  `advisor-plans/012-enrollment-ux.md`,
  `advisor-plans/013-browser-video-state.md`,
  `advisor-plans/015-low-cost-hotspots.md`,
  `advisor-plans/016-dynamic-camera-wall.md`,
  `advisor-plans/017-attendance-explainability.md`, and
  `advisor-plans/018-employee-edit-deactivate.md`; execute after all functional
  web plans
- **Category**: accessibility / UX
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

Rendered at 390 px, the page is 586 px wide because the fixed header cannot
collapse. Important camera/table labels are frequently 9–10 px, several muted/
blue text combinations are hard to read, and controls/live feedback lack names
or announcement semantics. The design explicitly requires basic mobile
usability and readable desktop/TV operations.

## Current state

- `AppShell` uses one fixed `h-16` non-wrapping row with brand, three links,
  millisecond clock, and admin link at
  `apps/web/src/components/AppShell.tsx:19-57`.
- Camera telemetry/status uses 9–10 px text throughout
  `apps/web/src/components/CameraTile.tsx:144-189`.
- Metric/table headings and filters use 10 px muted text in
  `MetricPill.tsx`, `employees/page.tsx`, and `attendance/page.tsx`.
- Employee search and detail file input lack visible labels; `EventTicker` and
  async form outcomes have no live-region semantics.
- Error states can render alongside “No employees/rows” empty states.
- `DESIGN.md` specifies 14 px labels, 12 px captions, sharp geometry, white/body
  text, and restrained M-color accents. `plans/07-frontend-ui.md` targets
  desktop/TV first but requires a basic stacked mobile layout.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Accessibility tests | `cd apps/web && npm test -- --run accessibility` | exit 0 |
| Web tests | `cd apps/web && npm test -- --run` | exit 0 |
| Lint/typecheck | `cd apps/web && npm run lint && npm run typecheck` | exit 0 |
| Build | `cd apps/web && npm run build` | exit 0 |

## Scope

**In scope**:

- `apps/web/src/components/{AppShell,CameraTile,MetricPill,EventTicker,StatusBadge}.tsx`
- employee, attendance, dashboard, and login page markup/classes
- `apps/web/src/test/accessibility.test.tsx` (create) for the focused shared
  accessibility and state-branch cases
- `apps/web/src/app/globals.css` only for an existing shared token/class that
  cannot be corrected locally
- `advisor-plans/README.md`

**Out of scope**: redesign, new brand assets/fonts, hamburger/menu library, icon
package, global component framework, axe dependency, mobile app, charts, feature
behavior, and `apps/api/**`.

## Git workflow

- Branch: `codex/014-responsive-accessible-ui`
- Suggested commits: `Make app shell responsive`, then
  `Improve operational accessibility`.

## Steps

### Step 1: Collapse navigation with native markup

Keep the existing desktop nav at `md` and above. Below `md`, hide the decorative
millisecond clock and use native `<details>/<summary>` for the same nav links and
admin action. Give summary an accessible label; closing on navigation is nice
only if it requires no global state. Keep M stripe, 64 px desktop header, and
sharp geometry.

Ensure page containers use `min-w-0`, tables own their horizontal scrolling,
and no fixed gap/min-width creates document-level horizontal overflow.

**Verify**: keyboard test opens/navigates mobile details; rendered 390×844 has
`document.documentElement.scrollWidth <= 390`.

### Step 2: Restore operational type and contrast

Raise essential telemetry, filters, headings, and control labels to at least
12 px; use 14 px for nav/form/button labels where space allows. Use `text-ink`,
`text-body-strong`, or `text-body` for information. Reserve M blues/red for
borders, stripes, icons, or sufficiently large/accent text. Preserve explicit
IN/OUT/status words so color is never the only signal.

Do not globally enlarge decorative scanline text if it is not operational.

**Verify**: static/component assertions find no `text-[9px]` on operational
components; manual 1440×900/TV-distance check keeps overlays readable.

### Step 3: Name controls and states

Add visible labels for employee search and every file input. Add `aria-pressed`
to attendance filter buttons, `aria-expanded` where rows/details expand, and
descriptive names for mobile navigation. Ensure native date/checkbox controls
retain their labels.

Use `role="status"`/`aria-live="polite"` for connection, upload, save, and event
ticker updates; use `role="alert"` for blocking errors. Do not announce every
detection frame.

**Verify**: component tests query controls by accessible name and assert pressed,
expanded, status, and alert semantics.

### Step 4: Make error, loading, and empty states exclusive

Employees and attendance must render exactly one of loading, blocking error,
empty, or rows. Preserve existing login links where useful, but never pair an
error with “No employees yet” or “No rows for this day.” Apply the same rule to
camera health/video states.

**Verify**: tests assert error suppresses empty state and loading suppresses both.

### Step 5: Perform two viewport dress rehearsals

With the active local web/API:

- 390×844: open nav, reach every top-level page/action, verify no document
  horizontal overflow and native table scroll remains local;
- 1440×900: verify live dashboard hierarchy, camera overlay readability, focus
  indicators, 200% zoom, and no clipped controls.

Record pass/fail notes in the implementation PR, not in source code. Do not add
screenshots containing employee/camera data.

**Verify**: all four automated commands pass after manual checks.

## Test plan

- Mobile nav keyboard/open/name/link coverage.
- Labels and states for search, files, filters, expandable rows, async outcomes.
- Mutually exclusive loading/error/empty/data branches.
- Camera/event live region does not announce high-frequency detections.
- Manual mobile, desktop, zoom, and focus check with non-private demo data.

## Done criteria

- [ ] 390 px viewport has no document-level horizontal overflow.
- [ ] Desktop/TV hierarchy and BMW M tokens remain intact.
- [ ] Operational text is at least 12 px with readable foreground colors.
- [ ] Interactive controls have accessible names/states and visible focus.
- [ ] Async outcomes/errors use appropriate live semantics.
- [ ] Empty/error/loading states are mutually exclusive.
- [ ] All commands and viewport checks pass; plan 014 is marked `DONE`.

## STOP conditions

- Fixing overflow requires removing a documented desktop/TV capability.
- A change requires licensed fonts/assets or a design-system replacement.
- A live region would announce every detection frame.
- The current markup has drifted enough that functional plan behavior would be
  overwritten; rebase after those plans instead.

## Maintenance notes

- Re-run the two viewport checks whenever shared shell or camera overlays change.
- Keep mobile deliberately basic; add a custom drawer only if mobile becomes a
  named product target.
