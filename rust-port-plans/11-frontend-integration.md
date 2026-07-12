# 11 — Frontend Integration (Next.js stays)

## 1. Decision

**Do not rewrite the web app in Rust.**  
`apps/web` remains Next.js 16 + Tailwind + BMW M design.

Rust port succeeds when this UI works against `pksp serve` with minimal env changes.

## 2. Environment variables

| Variable | Default today | Notes for Rust |
|---|---|---|
| `NEXT_PUBLIC_API_URL` | `http://localhost:8000` | Keep API on 8000 |
| `NEXT_PUBLIC_WS_URL` | `ws://localhost:8000/api/ws/live` | Same path |
| `NEXT_PUBLIC_WEBRTC_BASE` | `http://localhost:8889` | Media plane WHEP base |

If Rust media moves WHEP under another host/port, only `NEXT_PUBLIC_WEBRTC_BASE` changes.

## 3. Contract dependencies (must not break)

| Frontend | Backend contract |
|---|---|
| `lib/api.ts` | Bearer JSON; errors use `detail` |
| `lib/types.ts` | WS + Employee + DailyRow shapes |
| `hooks/useLiveWs.ts` | multiplexed events by `type` |
| `page.tsx` | `GET /api/health` → `webrtc_path` |
| `CameraTile.tsx` | WHEP + HLS fallback |
| `lib/whep.ts` | POST SDP to `{base}/{path}/whep` |
| login page | `POST /api/auth/login` → `access_token` |
| employees pages | CRUD + multipart |
| attendance page | daily JSON + CSV |

## 4. Optional frontend improvements (not required for port)

| Change | When |
|---|---|
| Show track `state` (approaching/walkby) | after smart scene ships |
| Codec badge from health | if API adds `video_codec` |
| Prefer unknown WS types no-op | harden switch |
| Serve web from Rust static | optional single-port deploy later |

Default: **zero frontend code changes** for M0–M3.

## 5. Static single-port option (later)

Axum can serve `apps/web` export (`next export` / standalone) from `/` and API under `/api`.

Pros: one port for operators.  
Cons: complicates Next dev workflow. Defer until edge appliance packaging.

## 6. Verification checklist (manual)

1. Login with `ADMIN_PASSWORD`  
2. Dashboard loads; health path drives WHEP  
3. Video plays (H264 path)  
4. HUD boxes move with mock or real faces  
5. Enroll employee with photos → embedding_ready  
6. Walk past camera → attendance ticker  
7. Attendance table + CSV export  

## 7. Acceptance criteria

- [ ] Documented env for pointing web at Rust  
- [ ] No required TS changes for parity  
- [ ] Smart-scene UI extensions optional and non-breaking  

## 8. Source map

| Path | Role |
|---|---|
| `apps/web/**` | keep |
| `DESIGN.md` | keep |
| Rust | consumer of same contracts only |
