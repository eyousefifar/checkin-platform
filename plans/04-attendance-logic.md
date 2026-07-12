# 04 — Attendance Logic

## Goal

Turn multi-frame identity commits into **check-in / check-out events** that are demo-stable: few duplicates, few false accepts, readable daily summary.

## Camera roles

Each camera has a `direction` mode:

| Mode | Behavior |
|---|---|
| `in` | Commits produce **check_in** (subject to cooldown & day rules) |
| `out` | Commits produce **check_out** |
| `bidirectional` | Infer from day state: no open IN → check_in; has open IN → check_out |

**MVP recommendation for 1–2 cams:**

- 2 cameras: `cam_in` = `in`, `cam_out` = `out`
- 1 camera: `bidirectional`

## Event kinds

| `kind` | Meaning |
|---|---|
| `check_in` | Entrance recorded |
| `check_out` | Exit recorded |
| `unrecognized` | Face committed as unknown (optional logging) |
| `rejected_spoof` | Blocked by anti-spoof (audit) |
| `rejected_low_conf` | Below threshold / ambiguous (optional) |

Payroll-facing summary uses only `check_in` / `check_out` for known employees.

## Finite state (per employee per local day)

Local day = timezone-configured calendar date (default host TZ, e.g. Asia/Tehran if company is IR — **make configurable**).

```
              check_in
   ABSENT ──────────────► PRESENT_OPEN
      ▲                      │
      │         check_out    │
      └──────────────────────┘
              PRESENT_CLOSED (has both; further outs update last_out)
```

Simplified storage: **append-only events**; derive state from events for the day.

### Derived daily row

| Field | Derivation |
|---|---|
| `first_in` | earliest `check_in` that day |
| `last_out` | latest `check_out` that day |
| `status` | `absent` / `present` / `incomplete` (in without out) / `anomaly` |

## Cooldown

Prevent double punches when someone stands in front of the camera.

| Param | Default | Scope |
|---|---|---|
| `COOLDOWN_SECONDS` | 90 | Same `employee_id` + same `camera_id` |
| Optional global | 60 | Same employee any camera (stricter) |

On commit:

```
if now - last_event_ts(employee, camera, kind_compatible) < COOLDOWN:
    ignore (emit HUD "COOLDOWN" only)
else:
    write attendance event
```

Cooldown should **not** block a legitimate OUT on a different camera shortly after IN (use per-camera cooldown; allow cross-camera transitions after `MIN_DWELL_SECONDS` e.g. 30s).

## Bidirectional rules (single camera)

1. If no `check_in` today → next commit = `check_in`
2. Else if last event is `check_in` and dwell ≥ `MIN_DWELL_SECONDS` → `check_out`
3. Else if last event is `check_out` and cooldown passed → treat as new `check_in` (return to office)
4. Else → ignore / cooldown

## Explicit IN/OUT cameras

- `in` camera never writes `check_out`
- `out` camera never writes `check_in`
- If employee triggers OUT without IN → still record `check_out`, mark day `anomaly` or `incomplete_reverse` for review
- If double IN (left building without OUT) → second IN after long gap can be second session; for MVP keep simple: allow multiple IN/OUT pairs ordered by time

## Confidence handling

| Case | Action |
|---|---|
| High score + margin + vote | Auto event |
| High score, low margin | `needs_review` flag optional; no auto event in strict mode |
| Unknown face vote | Optional `unrecognized` event (cap rate: 1 / person-track / cooldown) |
| Spoof fail | `rejected_spoof` audit only |

**CEO demo default:** auto-commit on strong votes; strict margin to avoid wrong names.

## Manual review (post-MVP UI stub)

Fields reserved in schema:

- `needs_review: bool`
- `reviewed_by`, `reviewed_at`
- `override_kind`

MVP can omit UI but keep columns nullable.

## Duplicate & edge cases

| Scenario | Handling |
|---|---|
| Person walks past twice quickly | Cooldown absorbs |
| Person loiters facing cam | One event + cooldown |
| Two people side by side | Separate tracks / commits |
| Night cleaner unknown | Unrecognized; no employee row |
| Clock skew | Use server monotonic + server local time for events |
| Midnight crossover | Day bucket by local date of event ts |
| Re-enrollment mid-day | Gallery reload; events already written stay |

## Daily report logic

Query for date `D`:

1. All active employees (left join events on `D`).
2. Aggregate first_in, last_out, count events.
3. Status:

```
if no check_in and no check_out: absent
elif check_in and not check_out: incomplete
elif check_in and check_out: present
elif check_out and not check_in: anomaly
```

Duration = `last_out - first_in` when both exist.

## CSV export columns

```
date, employee_code, name, department, first_in, last_out, duration_minutes, status, check_in_count, check_out_count
```

## Timezone

- Store all timestamps in **UTC ISO-8601** in DB.
- Display in configured `APP_TIMEZONE`.
- “Per day” grouping uses APP_TIMEZONE calendar date.

## Configuration defaults

```yaml
attendance:
  timezone: "UTC"   # override per deploy
  cooldown_seconds: 90
  min_dwell_seconds: 30
  allow_unrecognized_events: true
  strict_margin: true
```

## Pseudocode

```python
def on_identity_commit(employee_id, camera, score, ts):
    cam = get_camera(camera)
    if in_cooldown(employee_id, camera, ts):
        return Skip("cooldown")

    kind = resolve_kind(employee_id, cam.direction, ts)
    if kind is None:
        return Skip("no_transition")

    event = insert_event(...)
    broadcast_ws(event)
    return event

def resolve_kind(employee_id, direction, ts):
    if direction == "in":
        return "check_in"
    if direction == "out":
        return "check_out"
    # bidirectional
    last = last_event_today(employee_id)
    if last is None or last.kind == "check_out":
        return "check_in"
    if last.kind == "check_in" and dwell_ok(last.ts, ts):
        return "check_out"
    return None
```

## Metrics for dashboard

- Check-ins today
- Currently present (IN without later OUT)
- Unrecognized count today
- Last event age (seconds)
