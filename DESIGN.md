---
version: alpha
name: PKSP-aerospace-ops
description: >
  On-prem vision attendance command center. Space Black canvas, Spectral White
  ink, graphite instrument surfaces, restrained operational cyan/green/amber/red.
  Industrial uppercase labels, zero decorative shadows, mostly sharp geometry,
  8px rhythm. Live camera wall and event snapshots are the cinematic imagery —
  not stock hero photography or marketing feature grids.
colors:
  primary: "#f4f4f5"
  ink: "#f4f4f5"
  body: "#a1a1aa"
  body-strong: "#e4e4e7"
  muted: "#85858f"
  hairline: "#3f3f46"
  hairline-strong: "#27272a"
  canvas: "#050505"
  surface-card: "#111113"
  surface-elevated: "#1a1a1e"
  surface-soft: "#0a0a0b"
  on-primary: "#050505"
  on-dark: "#f4f4f5"
  cyan: "#22d3ee"
  signal: "#34d399"
  danger: "#f87171"
  warning: "#fbbf24"
  graphite: "#2a2a2e"
typography:
  display-xl:
    fontFamily: "IBM Plex Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: 48px
    fontWeight: 600
    lineHeight: 1.05
    letterSpacing: 0.02em
  display-lg:
    fontFamily: "IBM Plex Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: 36px
    fontWeight: 600
    lineHeight: 1.1
    letterSpacing: 0.02em
  display-md:
    fontFamily: "IBM Plex Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: 28px
    fontWeight: 600
    lineHeight: 1.15
    letterSpacing: 0.02em
  title-lg:
    fontFamily: "IBM Plex Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: 20px
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: 0.04em
  label-uppercase:
    fontFamily: "IBM Plex Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: 11px
    fontWeight: 700
    lineHeight: 1.3
    letterSpacing: 0.12em
  body-md:
    fontFamily: "IBM Plex Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: 14px
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: 0
  mono:
    fontFamily: "IBM Plex Mono, ui-monospace, monospace"
    fontSize: 12px
    fontWeight: 400
    lineHeight: 1.4
    letterSpacing: 0
spacing:
  unit: 8px
  scale: [4, 8, 12, 16, 24, 32, 48, 64]
rounded:
  none: 0px
  sm: 2px
  md: 0px
shadow:
  none: none
motion:
  duration-fast: 120ms
  duration-med: 280ms
  easing: cubic-bezier(0.22, 1, 0.36, 1)
  reduced-motion: disable-nonessential
components:
  mission-header:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.ink}"
    typography: "{typography.label-uppercase}"
    rounded: "{rounded.none}"
    height: 56px
    padding: 0 24px
  monitor-surface:
    backgroundColor: "{colors.surface-soft}"
    textColor: "{colors.body-strong}"
    typography: "{typography.body-md}"
    rounded: "{rounded.none}"
    padding: 0
  configure-surface:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.body}"
    typography: "{typography.body-md}"
    rounded: "{rounded.none}"
    padding: 24px
  inspect-dialog:
    backgroundColor: "{colors.surface-card}"
    textColor: "{colors.on-dark}"
    typography: "{typography.mono}"
    rounded: "{rounded.none}"
    padding: 0
  telemetry-cell:
    backgroundColor: "{colors.surface-elevated}"
    textColor: "{colors.body}"
    typography: "{typography.mono}"
    rounded: "{rounded.none}"
    padding: 8px 16px
  primary-action:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    typography: "{typography.label-uppercase}"
    rounded: "{rounded.none}"
    height: 44px
    padding: 0 16px
  technical-divider:
    backgroundColor: "{colors.hairline}"
    textColor: "{colors.body-strong}"
    typography: "{typography.mono}"
    rounded: "{rounded.none}"
    height: 1px
  secondary-divider:
    backgroundColor: "{colors.hairline-strong}"
    textColor: "{colors.body}"
    typography: "{typography.mono}"
    rounded: "{rounded.none}"
    height: 1px
  muted-label:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.muted}"
    typography: "{typography.label-uppercase}"
    rounded: "{rounded.none}"
    padding: 4px 0
  acquisition-state:
    backgroundColor: "{colors.graphite}"
    textColor: "{colors.cyan}"
    typography: "{typography.label-uppercase}"
    rounded: "{rounded.sm}"
    padding: 8px
  commit-state:
    backgroundColor: "{colors.surface-card}"
    textColor: "{colors.signal}"
    typography: "{typography.label-uppercase}"
    rounded: "{rounded.none}"
    padding: 8px
  warning-state:
    backgroundColor: "{colors.surface-soft}"
    textColor: "{colors.warning}"
    typography: "{typography.label-uppercase}"
    rounded: "{rounded.none}"
    padding: 8px
  danger-state:
    backgroundColor: "{colors.surface-soft}"
    textColor: "{colors.danger}"
    typography: "{typography.label-uppercase}"
    rounded: "{rounded.none}"
    padding: 8px
---

# PKSP Aerospace Operations Design

## Intent

PKSP Check-In is an **on-prem vision command center**, not a SaaS marketing site.
Operators monitor door cameras, inspect attendance commits, and enroll faces under
model quality gates. The aesthetic is industrial aerospace ops: stark black/white,
spectral instrumentation accents, full-bleed live imagery, zero chrome noise.

## Surfaces

| Surface | Route | Role |
|---------|-------|------|
| **Monitor** | `/` | Live camera wall + telemetry + mission log |
| **Configure** | `/employees/*` | Employee enrollment and profile |
| **Inspect** | Event match reveal | Cinematic snapshot of a committed event |
| **Records** | `/attendance` | Daily attendance table |

## Color

- **Space Black** (`#050505`) canvas — full page
- **Spectral White** (`#f4f4f5`) primary ink
- **Graphite** surfaces (`#111113`, `#1a1a1e`) for panels
- **Operational cyan** (`#22d3ee`) for focus, scan, acquisition
- **Signal green** (`#34d399`) for commit/success
- **Amber** (`#fbbf24`) for retry/degraded
- **Danger red** (`#f87171`) for failures/spoof

No indigo gradients. No glassmorphism. No decorative card grid of equal feature tiles.

## Typography

- Industrial **uppercase** labels with wide tracking for chrome
- **IBM Plex Sans** for UI, **IBM Plex Mono** for telemetry
- Avoid Inter as a brand statement; system fallbacks only when Plex unavailable

## Geometry & motion

- Mostly **sharp** corners (0–2px)
- **No drop shadows** on product chrome
- **8px** spacing rhythm
- Scan sweeps and acquisition stages animate only when motion is allowed
- `prefers-reduced-motion: reduce` disables nonessential animation; status text remains meaningful

## Imagery

The live camera wall and stored event snapshots **are** the cinematic imagery.
Do not add stock hero photography, decorative icon toppers, or centered marketing stacks.

## Anti-patterns (slop audit = 0/10)

1. Wrong surface (marketing hero on Monitor)
2. Center-stack landing composition
3. Generic equal feature-tile grid
4. Unearned backdrop blur / glass
5. Tech-purple/indigo gradients
6. Icon-topper feature cards
