# Plan 013: Make browser video state truthful and retryable

> **Executor instructions**: Delete the non-working HLS branch and make WHEP
> success depend on actual playback. Keep health, WS, camera capture, and browser
> video as separate states. Update plan 013 in the index.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/web/src/components/CameraTile.tsx apps/web/src/components/CameraTile.test.tsx apps/web/src/app/page.tsx apps/web/src/app/dashboard.test.tsx apps/web/src/lib/whep.ts apps/web/src/lib/whep.test.ts apps/web/src/hooks/useHealth.ts apps/web/src/lib/types.ts advisor-plans/README.md`

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: `advisor-plans/001-active-stack-verification.md`,
  `advisor-plans/007-mediamtx-pipeline.md`,
  `advisor-plans/008-live-websocket-state.md`, and
  `advisor-plans/011-app-timezone.md`
- **Category**: bug / UX
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

The current fallback marks HLS active without proving playback and its effect
cleanup clears the source during its own state transition. WHEP setup success is
also treated as video success before a frame plays, while one-shot health lookup
can pin a startup fallback path forever. The dashboard’s hero surface can
therefore be blank while claiming success.

## Current state

- `CameraTile` derives HLS URL/state at
  `apps/web/src/components/CameraTile.tsx:32-37`.
- `tryHls` assigns `video.src`, suppresses `play()` rejection, and sets the
  fallback badge at `:60-69`; the effect depends on that state and cleanup clears
  the source at `:104-114`.
- `showVideo` treats intent as success at `:116`.
- `connectWhep` resolves after remote SDP, while actual playback occurs later in
  `ontrack` at `apps/web/src/lib/whep.ts:41-48`.
- Dashboard fetches health once and retains `"demo"` on any failure at
  `apps/web/src/app/page.tsx:18-31`.
- Plans 007/008/011 provide real publisher state, explicit camera status, and a
  retrying typed `useHealth` hook.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Media component tests | `cd apps/web && npm test -- --run CameraTile` | exit 0 |
| Dashboard tests | `cd apps/web && npm test -- --run dashboard` | exit 0 |
| Lint/typecheck | `cd apps/web && npm run lint && npm run typecheck` | exit 0 |
| Build | `cd apps/web && npm run build` | exit 0 |

## Scope

**In scope**:

- `apps/web/src/components/CameraTile.tsx` and
  `apps/web/src/components/CameraTile.test.tsx` (create)
- `apps/web/src/lib/whep.ts` and `apps/web/src/lib/whep.test.ts`
- `apps/web/src/app/page.tsx` and
  `apps/web/src/app/dashboard.test.tsx` (create if plan 008 did not already)
- `apps/web/src/hooks/useHealth.ts`, `apps/web/src/lib/types.ts` only as needed
- `advisor-plans/README.md`

**Out of scope**: HLS/hls.js, alternate media protocol, media-server changes,
camera-wall expansion, state library, UI redesign, and `apps/api/**`.

## Git workflow

- Branch: `codex/013-browser-video-state`
- Commit message: `Make browser video state truthful`

## Steps

### Step 1: Delete the pseudo-HLS path

Remove `useHlsFallback`, HLS URL/source assignment, fallback badge, fallback
branches, and related effect dependencies/comments. On WHEP failure, retain the
actionable error and retry WHEP. Do not add a replacement library.

**Verify**: `rg -n 'HLS|hls|useHlsFallback' apps/web/src/components/CameraTile.tsx`
returns no match; typecheck passes.

### Step 2: Model one WHEP playback lifecycle

Use a small local state union: `connecting`, `playing`, or `error`. SDP success
does not set `playing`; the `<video onPlaying>` event does. `onWaiting`,
`onStalled`, track-ended, or connection failure must leave playing and show an
honest reconnect/error state. Keep a capped retry delay and exactly one timer/
WHEP handle; cleanup closes both and clears `srcObject`.

Do not add an event emitter or media state machine library.

**Verify**: tests cover SDP-without-playing, playing, WHEP failure/retry,
stalled/ended, path change, and unmount cleanup.

### Step 3: Consume retrying health data

Replace the dashboard’s one-shot health fetch/default path with plan 011’s
`useHealth`. While health is unavailable, show “health retrying” and do not
hammer an assumed WHEP path. When a camera path changes, let the existing effect
cleanly close the old handle and connect the new endpoint.

Never fetch the authenticated camera endpoint or expose RTSP URLs.

**Verify**: dashboard test fails health once, succeeds later, and asserts WHEP
uses the returned path without page reload.

### Step 4: Separate capture and playback labels

Use plan 008’s `online?: boolean` only for camera capture: online, offline, or
unknown. Render browser video state separately (`VIDEO LIVE`, `CONNECTING`, or
error). A playing video may coexist briefly with unknown camera status, but it
must not rewrite camera state.

**Verify**: component matrix covers every camera × video state without a false
green camera badge.

## Test plan

- WHEP handle/timer lifecycle and actual `playing` event.
- Health failure/recovery/path change.
- Explicit camera unknown/offline/online independent of video.
- No actual WebRTC/network; mock `connectWhep` at module boundary.

## Done criteria

- [x] No HLS code/dependency/claim remains.
- [x] Video is live only after `playing`.
- [x] WHEP errors remain visible and retry through one owned timer.
- [x] Late API/media startup and path changes recover without reload.
- [x] Camera and browser-video status remain separate.
- [x] All four commands pass; plan 013 is marked `DONE`.

## STOP conditions

- A named supported browser has an approved HLS requirement; write a separate
  hls.js plan with playback-event acceptance.
- MediaMTX no longer exposes WHEP at the typed health path.
- Autoplay policy requires user interaction; report the target browser and add
  one explicit play control rather than suppressing the error.

## Maintenance notes

- Keep WHEP as the only browser path until an observed browser constraint says
  otherwise.
- Any new playback status must be derived from browser/media events, not intent.
