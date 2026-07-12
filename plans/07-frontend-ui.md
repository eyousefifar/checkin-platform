# 07 — Frontend UI

## Stack

- Next.js 15 App Router + TypeScript
- Tailwind CSS mapped to `DESIGN.md` tokens
- WebSocket client for live events
- WebRTC player (MediaMTX WHEP/WHIP or MediaMTX reader API)

## Design system (BMW M)

Source: root [`DESIGN.md`](../DESIGN.md) installed from [getdesign.md/bmw-m](https://getdesign.md/bmw-m/design-md).

### Principles for this product

| Rule | Application |
|---|---|
| Near-black canvas | App background `#000000` |
| M tricolor sparingly | 3px top stripe; active nav indicator; event accent |
| UPPERCASE display | Page titles, section labels |
| Light/engineered type | Body light weight; avoid chunky SaaS blobs |
| Sharp geometry | `rounded-none` / `rounded-sm` — not pill-heavy |
| Full-bleed photography energy | Live video is the “hero image” |
| Sci-fi without cartoon | Corner brackets, mono telemetry, thin HUD lines — no neon overload |

### Token mapping (Tailwind)

```css
--canvas: #000000;
--surface-card: #1a1a1a;
--surface-elevated: #262626;
--ink: #ffffff;
--body: #bbbbbb;
--muted: #7e7e7e;
--hairline: #3c3c3c;
--m-blue-light: #0066b1;
--m-blue-dark: #1c69d4;
--m-red: #e22718;
--success: #0fa336;
--warning: #f4b400;
```

### Fonts

- Prefer BMW Type Next if licensed; else **Inter** or **Geist** with uppercase tracking for display.
- Mono: **Geist Mono** / `ui-monospace` for scores, FPS, timestamps.

### License banner

Persistent slim bar or footer:

> RESEARCH DEMO — Face recognition models may be non-commercial. Not a production payroll system.

## Information architecture

| Route | Purpose | Tone |
|---|---|---|
| `/` | Live command dashboard | Sci-fi ops |
| `/employees` | List employees | Simple professional |
| `/employees/new` | Create + upload faces | Simple professional |
| `/employees/[id]` | Edit + re-enroll | Simple professional |
| `/attendance` | Daily results | Simple professional |
| `/login` | Admin password | Minimal |

Shared shell: top M-stripe, brand wordmark `PKSP CHECK-IN`, nav links, camera health dots.

## Page specs

### 1. Live dashboard `/`

**Layout**

```
┌─ M STRIPE ─────────────────────────────────────────────┐
│ PKSP CHECK-IN          DASHBOARD  EMPLOYEES  ATTENDANCE │
├──────────────────────────────┬──────────────────────────┤
│  METRICS: cams | present |   │                          │
│  events | vision fps         │   EVENT TICKER           │
├──────────────────────────────┤   (newest first)         │
│                              │                          │
│   CAMERA TILE(S)             │   John · CHECK-IN · 0.72 │
│   video + canvas HUD         │   Sara · TRACKING · …    │
│   corner brackets            │                          │
│   offline state              │                          │
│                              │                          │
└──────────────────────────────┴──────────────────────────┘
```

**Camera tile**

- Aspect 16:9 container, carbon border `#3c3c3c`
- Video element (WebRTC)
- Absolutely positioned canvas for boxes
- Overlay chrome: camera name, direction badge (IN/OUT), ONLINE/OFFLINE, local FPS of vision
- Face box: thin white/blue stroke; label plate with name + score bar
- UNKNOWN: muted label; COOLDOWN: warning tint

**Event ticker**

- Auto-scroll list
- Kind colored: check_in success green accent, check_out blue, unknown muted, spoof red

**Empty / error**

- No cameras configured → CTA to env docs
- WS disconnected → reconnecting pulse
- Vision degraded → amber banner

### 2. Employees `/employees`

- Search input
- Table: code, name, department, images usable, embedding ready, active, actions
- Primary button: `ADD EMPLOYEE` (uppercase tracking)
- Row click → detail

### 3. Employee form

Fields:

- Employee code *
- Full name *
- Department
- Active toggle
- Multi-file image upload (drag-drop)
- Live feedback chips: usable / rejected reasons
- Save + “Recompute embedding”

Guidance callout with enrollment photo tips (door angle, 5–10 photos).

### 4. Attendance `/attendance`

- Date picker (default today)
- Status filter chips: all / present / incomplete / absent / anomaly
- Table with duration + status badges
- `EXPORT CSV` button
- Optional expand row → raw events that day

## Components (build list)

| Component | Notes |
|---|---|
| `AppShell` | Stripe, nav, disclaimer |
| `MetricPill` | Dashboard counters |
| `CameraTile` | Video + canvas + status |
| `FaceHudCanvas` | Draw detections; resize observer |
| `EventTicker` | WS attendance + notable tracks |
| `StatusBadge` | present/absent/… |
| `DataTable` | Employees / attendance |
| `FileDropzone` | Multi image |
| `EmptyState` | Professional, not cute |
| `LicenseBanner` | Non-commercial notice |

## WebRTC integration

- MediaMTX exposes WebRTC reader endpoints (see current MediaMTX docs for WHEP path pattern).
- Config per camera: `NEXT_PUBLIC_WEBRTC_BASE` + path `cam_in` / `cam_out`.
- If WebRTC fails in corporate browser, document HLS fallback (`hls.js`) as backup — higher latency OK for contingency.

## WebSocket client

- Connect on dashboard mount; exponential backoff reconnect
- Parse message `type` discriminator
- Store latest detections per `camera_id` in React state/ref
- RAF draw loop on canvas when detections update or video resizes

### Overlay alignment

- Use normalized bbox × `video.clientWidth/Height`
- Accept 100–300ms lag between video and boxes for CPU demo
- Optional: hide boxes older than 500ms without refresh

## Responsive

- CEO demo target: **desktop / TV 1080p+**
- Employees/attendance usable on laptop
- Mobile not a priority; basic stack layout OK

## Accessibility (pragmatic)

- Keyboard nav for forms/tables
- Sufficient contrast on body text (`#bbbbbb` on black is borderline — use `#e6e6e6` for primary content)
- Do not rely on color alone for IN/OUT (include text)

## State management

- React state + SWR/React Query for REST
- No Redux required
- Small context for auth token

## Sci-fi micro-details (tasteful)

- 1px grid or vignette on dashboard only
- Corner L-brackets on camera tiles
- Mono clock `HH:MM:SS.mmm` in header
- Soft scanline CSS optional at ≤5% opacity
- Avoid: particle storms, rainbow glows, fake “AI brain” animations
