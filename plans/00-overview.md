# 00 — Overview

## Purpose

PKSP Check-In is an **on-prem employee attendance platform** that uses office IP cameras and face recognition to record entrance and exit times. This document set defines the architecture, stack, data model, UI, and verification path for a **CEO-demo MVP** that can later harden into production.

## Goals

1. **Live ops dashboard** — connect to 1–2 RTSP cameras, show live video, overlay real-time face detections (sci-fi, BMW M aesthetic).
2. **Employee enrollment** — admin form to register employees with multiple face photos and build a recognition gallery.
3. **Daily attendance** — reliable check-in / check-out results per day, exportable, simple and professional.

## Confirmed constraints

| Constraint | Value |
|---|---|
| Primary goal | CEO demo / local technical MVP |
| Compute | CPU only / Apple Silicon Mac |
| Cameras | 1–2 IP cameras (RTSP) |
| Employee scale | &lt; 50 |
| Deployment | Fully on-prem / local LAN |
| Repo state | Greenfield (`pksp-checkin`) |

## Non-goals (MVP)

- Cloud face APIs or off-site video storage
- Multi-tenant SaaS / mobile app
- KYC-grade liveness (challenge-response, depth, IR)
- Payroll integration, HRIS, access-control door unlock
- Kubernetes, distributed vector DBs, multi-region HA
- Silent production use of non-commercial model weights without license review

## Design principles

1. **Gates over magic** — attendance events require quality + confidence margin + multi-frame agreement + cooldown, not a single lucky frame.
2. **Split video from intelligence** — browser gets smooth WebRTC video; overlays arrive as JSON over WebSocket.
3. **Right-size complexity** — no FAISS, no person-detector stage, no dual recognizers for &lt;50 people on CPU.
4. **Honest licensing** — demo may use InsightFace `buffalo_l` with explicit non-commercial banner; commercial path documented.
5. **Demo-first polish** — dashboard must impress; enrollment and attendance stay clean and professional.

## Success criteria (CEO walkthrough)

- [ ] Dark motorsport-grade live board with working feed(s)
- [ ] Known employee gets name + box within ~2s of facing camera
- [ ] Check-in appears on daily attendance without manual entry
- [ ] New employee enrollable mid-session without process restart
- [ ] Entire stack runs on one Mac / LAN with no cloud dependency
- [ ] License / research-model disclaimer visible in admin UI

## Document map

| Doc | Topic |
|---|---|
| [01-architecture](./01-architecture.md) | Components, flows, failure modes |
| [02-tech-stack](./02-tech-stack.md) | Stack, licenses, rejected alternatives |
| [03-vision-pipeline](./03-vision-pipeline.md) | Models, thresholds, tracking, voting |
| [04-attendance-logic](./04-attendance-logic.md) | FSM, cameras, cooldowns |
| [05-data-model](./05-data-model.md) | Schema, embeddings storage |
| [06-api-and-realtime](./06-api-and-realtime.md) | REST + WebSocket contracts |
| [07-frontend-ui](./07-frontend-ui.md) | Routes, BMW M, HUD |
| [08-infra-and-deploy](./08-infra-and-deploy.md) | Docker, MediaMTX, Mac notes |
| [09-security-privacy](./09-security-privacy.md) | Biometrics, retention, consent |
| [10-implementation-roadmap](./10-implementation-roadmap.md) | Phases, demo script |
| [11-verification](./11-verification.md) | E2E tests without a crowd |

## Stakeholders

| Role | Interest |
|---|---|
| CEO | Visual impact, “it works,” attendance truth sample |
| Ops / admin | Enroll people, read daily sheet |
| Engineering | Maintainable on-prem stack, clear upgrade path |
| Legal (later) | Model licenses, biometric consent, data retention |

## Glossary

| Term | Meaning |
|---|---|
| **Embedding** | Fixed-length face vector (512-d for buffalo_l) |
| **Gallery** | In-memory set of enrolled employee vectors |
| **Track** | Same face across consecutive frames |
| **Vote** | Require identity agreement over N frames |
| **Cooldown** | Suppress duplicate events after a commit |
| **HUD** | On-screen detection overlays (boxes, labels) |
| **PAD** | Presentation attack detection (anti-spoof) |
