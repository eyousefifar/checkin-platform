# PKSP Check-In — Plan Set

Implementation-ready design documents for the on-prem face check-in MVP.

**Constraints:** CEO demo · CPU / Apple Silicon · 1–2 RTSP cameras · &lt;50 employees · LAN only.

## Read order

| # | Document | Summary |
|---|---|---|
| 00 | [Overview](./00-overview.md) | Goals, non-goals, success criteria |
| 01 | [Architecture](./01-architecture.md) | Components, flows, process model |
| 02 | [Tech stack](./02-tech-stack.md) | Choices, licenses, rejections |
| 03 | [Vision pipeline](./03-vision-pipeline.md) | Models, gates, tracking, voting |
| 04 | [Attendance logic](./04-attendance-logic.md) | FSM, cameras, cooldown |
| 05 | [Data model](./05-data-model.md) | SQLite schema, embeddings |
| 06 | [API & realtime](./06-api-and-realtime.md) | REST + WebSocket contracts |
| 07 | [Frontend UI](./07-frontend-ui.md) | BMW M, routes, HUD |
| 08 | [Infra & deploy](./08-infra-and-deploy.md) | MediaMTX, Docker, Mac |
| 09 | [Security & privacy](./09-security-privacy.md) | Biometrics, retention |
| 10 | [Implementation roadmap](./10-implementation-roadmap.md) | Phases A–E, CEO script |
| 11 | [Verification](./11-verification.md) | Tests & dress rehearsal |

## Design system

Root [`DESIGN.md`](../DESIGN.md) — BMW M tokens from [getdesign.md/bmw-m](https://getdesign.md/bmw-m/design-md).

## First-principles deltas vs research guide

- **No FAISS** at this scale — NumPy cosine.
- **No person-detector stage** for 1–2 entrance cams.
- **No dual AdaFace stack** on CPU MVP.
- **buffalo_l** for demo accuracy; **commercial path** = AuraFace / license / YuNet+SFace.
- **Split video (MediaMTX WebRTC) from intelligence (WebSocket HUD)**.

## Implementation goal

Autonomous execution contract (TDD → implement → test → verify → refactor → optimize → Perfect, phases A→E):

→ **[../GOAL.md](../GOAL.md)**

## Next step

Start `/goal` using `GOAL.md`, or implement Phase A per [10-implementation-roadmap](./10-implementation-roadmap.md).
