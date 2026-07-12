# Plan 008: Keep WebSocket delivery and camera state live

> **Executor instructions**: Treat transport connectivity, camera capture, and
> detection freshness as separate states. Use the existing broadcast channel
> and native browser WebSocket. Update plan 008 in the index when complete.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/edge/crates/pksp-api/Cargo.toml apps/edge/crates/pksp-api/src/routes.rs apps/edge/crates/pksp-api/tests/websocket.rs apps/web/src/hooks apps/web/src/app/page.tsx apps/web/src/app/dashboard.test.tsx apps/web/src/lib/types.ts advisor-plans/README.md`

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: `advisor-plans/001-active-stack-verification.md` and
  `advisor-plans/005-frame-inference-scheduling.md`
- **Category**: bug / realtime
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

A lagged Rust broadcast receiver exits its send loop but can leave the socket
open, so the UI still says “WS linked” while receiving nothing. Frontend
unmount can schedule a new reconnect, and stale detections survive offline or
disconnect events. Operators therefore cannot trust the live HUD state.

## Current state

- `handle_ws` uses `while let Ok(ev) = rx.recv().await` at
  `apps/edge/crates/pksp-api/src/routes.rs:437`; `Lagged` exits the send task,
  while the receive half continues at `:449`.
- `useLiveWs` schedules reconnect from every `onclose` at
  `apps/web/src/hooks/useLiveWs.ts:26-31`; cleanup closes the socket after
  clearing the current timer at `:58-63`, allowing a new timer to be created.
- Detections are stored as face arrays without receipt time and are never
  cleared on close/offline at `useLiveWs.ts:36-47`.
- Dashboard converts an unknown camera status to online whenever WS is connected
  at `apps/web/src/app/page.tsx:11-12`.
- The WS message discriminator/types already live in
  `apps/web/src/lib/types.ts`; extend them rather than adding state management.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| API WS tests | `cd apps/edge && cargo test -p pksp-api --locked websocket` | exit 0 |
| Web tests | `cd apps/web && npm test -- --run useLiveWs` | exit 0 |
| Web gates | `cd apps/web && npm run lint && npm run typecheck` | exit 0 |
| Full active tests | `cd apps/edge && cargo test --locked && cd ../web && npm test -- --run` | exit 0 |

## Scope

**In scope**:

- `apps/edge/crates/pksp-api/Cargo.toml`,
  `apps/edge/crates/pksp-api/src/routes.rs`, and
  `apps/edge/crates/pksp-api/tests/websocket.rs` (create)
- `apps/web/src/hooks/useLiveWs.ts` and
  `apps/web/src/hooks/useLiveWs.test.ts` (create)
- `apps/web/src/app/page.tsx` and
  `apps/web/src/app/dashboard.test.tsx` (create)
- `apps/web/src/lib/types.ts`
- `advisor-plans/README.md`

**Out of scope**: auth contract, broadcast capacity tuning, durable event replay,
SSE, Redux/state libraries, media playback, metrics semantics, and
`apps/api/**`.

## Git workflow

- Branch: `codex/008-live-websocket-state`
- Commit message: `Keep live WebSocket state fresh`

## Steps

### Step 1: Continue after broadcast lag

Replace the `while let` with an explicit match:

- `Ok(event)`: send it; break on socket send failure;
- `RecvError::Lagged(skipped)`: record a counter or sanitized warning and
  continue from the receiver’s advanced cursor;
- `RecvError::Closed`: break and let the connection close.

Do not replay skipped detections or add a queue. Live state should catch up to
the newest broadcast.

Add an actual WebSocket test using the plan-001 router harness and the already
workspace-pinned Tokio/Tungstenite tools: overrun a tiny test broadcast channel,
then assert the client receives a later event or the connection closes—never an
open silent socket.

Add `tokio-tungstenite = { workspace = true }` under `pksp-api` dev
dependencies. In `tests/websocket.rs`, bind the extracted app to
`127.0.0.1:0`, read the assigned port from `local_addr`, serve it under a
cancellation token/task guard, and connect the real client to that loopback
address. The test must close the client, cancel the server, and await both tasks
on every assertion path. Do not hardcode a port or add an HTTP/WS test library.

**Verify**: API WebSocket tests pass.

### Step 2: Make browser teardown terminal

Add a closure-local `stopped` flag. On effect cleanup, set it before detaching
or closing the socket and clearing the timer. `onclose` and synchronous
construction failure may schedule reconnect only when not stopped. Close and
replace any prior socket before assigning a new one.

Tests with fake timers and a minimal fake WebSocket must prove active close
reconnects with capped exponential backoff, unmount never reconnects, and only
one socket/timer exists.

**Verify**: `npm test -- --run useLiveWs` → pass.

### Step 3: Track camera and detection freshness explicitly

Do not infer camera online from WS connectivity. Preserve unknown until a
`camera_status` message arrives. Store a monotonic receipt time beside each
camera’s latest faces. Clear that camera’s faces immediately on explicit
offline, clear all faces on socket close, and expire a face set after 500 ms
without another detections message, matching the documented HUD freshness rule.

Use one native interval/timer owned by the hook; no animation/state library.
Expose camera status as `boolean | undefined` so callers can render
online/offline/unknown.

**Verify**: fake-timer tests prove expiry, refresh, offline clear, close clear,
and WS-only state remaining unknown.

### Step 4: Update dashboard consumers

Remove `cameraOnline["cam_in"] ?? connected`. Pass explicit camera status to the
tile and keep the WS label transport-only. Do not change metrics or media
playback here.

**Verify**: dashboard test renders WS linked plus camera unknown without a green
online state; lint/typecheck pass.

## Test plan

- Rust: lagged receiver continues or closes; closed channel terminates; client
  close aborts send task.
- Web: open/close/backoff, unmount, construction failure, malformed message,
  explicit status, detection refresh/expiry, and state clear.
- Use fake timers and benign JSON only; no real network.

## Done criteria

- [x] `Lagged` cannot leave an open socket with a dead send loop.
- [x] Unmount leaves no socket or retry timer.
- [x] WS connectivity never implies camera online.
- [x] Offline/close/stale detections disappear deterministically.
- [x] All four commands pass; plan 008 is marked `DONE`.

## STOP conditions

- The router harness cannot establish a real in-process WebSocket even with an
  isolated `127.0.0.1:0` listener; report the concrete limitation before
  substituting a mock-only transport test.
- Product requires replay of every skipped event; that is a different durable
  messaging design.
- A proposed state change conflates camera capture with browser video playback.

## Maintenance notes

- The 500 ms HUD expiry is a display freshness rule, not camera-offline logic.
  Revisit only if configured inference runs below 2 FPS and the UX requirement
  changes.
- Keep broadcast lag observable so capacity can be measured before tuning.
