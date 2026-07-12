# Plan 002: Make Rust formatting and Clippy gates green

> **Executor instructions**: Run the drift check and baseline commands before
> editing. Keep this mechanical. Do not change behavior to satisfy style
> warnings. Update plan 002 in `advisor-plans/README.md` when complete.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/edge .github/workflows/ci.yml advisor-plans/README.md`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `advisor-plans/001-active-stack-verification.md`
- **Category**: DX / tech debt
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

`cargo fmt --all -- --check` and Clippy with warnings denied currently fail, so
review diffs contain avoidable noise and CI cannot enforce the repository’s
normal Rust quality gates. A single mechanical cleanup before the large vision
changes makes later review substantially safer.

## Current state

- `cargo test --locked` passes.
- `cargo fmt --all -- --check` exits nonzero with formatting diffs across the
  Rust workspace.
- `cargo clippy --all-targets --locked -- -D warnings` exits nonzero, including
  warnings on intentionally wide orchestration functions such as
  `process_loop` and `infer_frame` in
  `apps/edge/crates/pksp-vision/src/lib.rs:623-1104`.
- Rust uses standard rustfmt; there is no custom `rustfmt.toml` or Clippy config.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Format | `cd apps/edge && cargo fmt --all -- --check` | exit 0, no diff |
| Clippy | `cd apps/edge && cargo clippy --all-targets --locked -- -D warnings` | exit 0 |
| Tests | `cd apps/edge && cargo test --locked` | exit 0 |
| All features | `cd apps/edge && cargo check --all-features --locked` | exit 0 |

## Scope

**In scope**: existing `apps/edge/**/*.rs`, `.github/workflows/ci.yml`, and the
status row in `advisor-plans/README.md`.

**Out of scope**: behavior changes, public API changes, module moves, new
dependencies, generated files, model/camera work, and `apps/api/**`.

## Git workflow

- Branch: `codex/002-rust-quality-gates`
- Commit message: `Make Rust quality gates green`
- One mechanical commit; do not mix product work.

## Steps

### Step 1: Apply standard rustfmt once

Run `cargo fmt --all`. Do not hand-format around rustfmt or introduce a custom
configuration.

**Verify**: `cd apps/edge && cargo fmt --all -- --check` → exit 0.

### Step 2: Resolve actionable Clippy warnings minimally

Fix correctness/readability warnings with the smallest local edit. For a wide
orchestration function whose parameters correspond directly to existing state,
prefer a narrow `#[allow(clippy::too_many_arguments)]` plus a short
`// ponytail: orchestration boundary; group only when another caller exists`
comment over inventing a one-use options struct. Do not apply crate-wide allows.

**Verify**: `cd apps/edge && cargo clippy --all-targets --locked -- -D warnings`
→ exit 0.

### Step 3: Add the two gates to CI

After both commands are green, add `cargo fmt --all -- --check` and the exact
Clippy command to the Rust CI job created by plan 001.

**Verify**: run format, Clippy, tests, and all-features check in that order; all
exit 0.

## Test plan

No new behavior test is required for pure formatting. Any non-format Clippy
edit must remain covered by the existing crate tests; if it changes a branch or
loop, add one focused test beside that module before making the edit.

## Done criteria

- [ ] All four commands in the table exit 0.
- [ ] `git diff --check` exits 0.
- [ ] No new dependency or public type exists solely to silence Clippy.
- [ ] No crate-wide Clippy allow was added.
- [ ] CI runs format and Clippy.
- [ ] Plan 002 is marked `DONE`.

## STOP conditions

- A warning can only be removed through a behavior or public-contract change.
- Formatting touches generated, vendored, model, or non-Rust files.
- An executor is tempted to refactor `process_loop`/`infer_frame`; plan 005 owns
  their behavioral restructuring.

## Maintenance notes

Run rustfmt before reviewing later plan diffs. Keep targeted lint allows next to
the reason; delete an allow when a later plan naturally narrows the function.

