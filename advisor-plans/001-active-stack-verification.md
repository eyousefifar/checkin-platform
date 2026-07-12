# Plan 001: Establish the active-stack verification baseline

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before continuing. If a
> STOP condition occurs, stop and report; do not improvise. When done, update
> plan 001 in `advisor-plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- apps/edge/crates/pksp-api apps/web .github README.md plans/07-frontend-ui.md rust-port-plans/11-frontend-integration.md advisor-plans/README.md`
> If an in-scope file changed, compare the current-state excerpts below to the
> live code before proceeding.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: tests / DX
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

The active Rust API has no route-level tests, the web package has no runnable
tests, and its lint command/configuration are incompatible with installed
Next.js 16. Risky vision and data changes currently have no trustworthy outer
gate. This plan creates the smallest reusable router and browser-test harness,
then runs only active Rust/web checks in CI.

## Current state

- `apps/web/package.json:5-9` contains only `dev`, `build`, `start`, and
  `"lint": "next lint"`; it has no `typecheck` or `test` script.
- `apps/web/eslint.config.mjs:1-14` wraps old-style configs with `FlatCompat`.
  Direct ESLint currently fails, and Next.js 16 removed `next lint`.
- `apps/edge/crates/pksp-api/src/lib.rs:142-188` constructs the router inside
  `serve`, so route tests cannot instantiate the app without also binding a
  socket and starting workers.
- `apps/edge/crates/pksp-api` currently reports zero tests. Existing Rust tests
  use inline `#[cfg(test)] mod tests` modules, for example
  `apps/edge/crates/pksp-core/src/track.rs:183`.
- There is no `.github/workflows` directory.
- Verified baseline: `cargo test --locked` passes 52 tests and
  `npx tsc --noEmit --incremental false` passes; both web lint paths fail.

Repository conventions to preserve:

- Rust uses a Cargo workspace under `apps/edge`, inline tests for pure code,
  and `anyhow`/typed API errors at I/O boundaries.
- Web code uses TypeScript strict mode, native fetch, React state, Tailwind
  classes, and npm with the committed `package-lock.json`.
- Git messages are short imperative sentences, e.g. `Add Rust edge runtime...`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Rust tests | `cd apps/edge && cargo test --locked` | exit 0 |
| Rust all-features compile | `cd apps/edge && cargo check --all-features --locked` | exit 0 |
| Web lint | `cd apps/web && npm run lint` | exit 0 |
| Web typecheck | `cd apps/web && npm run typecheck` | exit 0 |
| Web tests | `cd apps/web && npm test -- --run` | exit 0 |
| Web production build | `cd apps/web && npm run build` | exit 0 |

## Scope

**In scope**:

- `apps/web/package.json`
- `apps/web/package-lock.json`
- `apps/web/eslint.config.mjs`
- `apps/web/vitest.config.ts` (create)
- `apps/web/src/test/setup.ts` (create)
- `apps/web/src/test/harness.test.tsx` (create)
- `apps/web/src/lib/whep.test.ts` (replace the non-test support file)
- `apps/web/src/lib/whep.test-support.ts` (delete)
- `apps/edge/crates/pksp-api/Cargo.toml`
- `apps/edge/crates/pksp-api/src/lib.rs`
- `apps/edge/crates/pksp-api/tests/api_contract.rs` (create)
- `.github/workflows/ci.yml` (create)
- `README.md` only for the exact verification commands and active framework
  version
- `plans/07-frontend-ui.md` and
  `rust-port-plans/11-frontend-integration.md` only to replace stale active
  Next.js 15 references with the installed Next.js 16 baseline
- `advisor-plans/README.md` status row

**Out of scope**:

- `apps/api/**` and every legacy-backend command.
- Product behavior changes, real model files, camera/network integration, and
  broad source formatting or Clippy cleanup; plan 002 owns the latter.
- Playwright, snapshots, coverage quotas, pre-commit frameworks, and test
  abstractions beyond one shared API-state builder if required.

## Git workflow

- Branch: `codex/001-active-stack-verification`
- Commit message: `Add active stack verification baseline`
- Do not push or open a PR unless instructed.

## Steps

### Step 1: Repair web scripts and flat ESLint configuration

Change `lint` to `eslint .`; add `typecheck` using
`tsc --noEmit --incremental false`; add `"test": "vitest"`. Replace
`FlatCompat` with the native flat exports from the installed
`eslint-config-next` package. Remove `@eslint/eslintrc`.

Add only the minimum dev dependencies needed for jsdom component/hook tests:
Vitest, jsdom, and React Testing Library. Do not add Jest, Babel, Playwright,
or duplicate assertion libraries.

Create `vitest.config.ts` unconditionally. Set the test environment to `jsdom`,
load `src/test/setup.ts`, clear/restore mocks between tests, and map `@` to the
absolute `src` directory with `fileURLToPath(new URL("./src", import.meta.url))`.
The setup file performs React Testing Library cleanup after each test; do not
add browser polyfills until a focused test demonstrates one is required.

**Verify**: `cd apps/web && npm run lint && npm run typecheck` → exit 0.

### Step 2: Prove both URL logic and the component harness

Replace `whep.test-support.ts` with `whep.test.ts`. Assert that `whepUrl` trims
one trailing base slash, trims one leading path slash, and produces the exact
`/<path>/whep` endpoint. This proves test discovery without coupling to UI.

Add `src/test/harness.test.tsx`: import `whepUrl` through the `@/` alias, render
one inline link whose `href` is produced by it, query that link by accessible
name, and compare its `href` with standard Vitest assertions. This single test
proves TSX transform, jsdom, the configured alias, React Testing Library, and
cleanup without snapshotting or coupling the baseline to a product component.

**Verify**: `cd apps/web && npm test -- --run` → both test files pass; temporarily
removing the jsdom environment or alias makes `harness.test.tsx` fail.

### Step 3: Make the Axum router constructible in tests

Extract only router assembly from `serve` into a function such as
`pub fn app(state: AppState) -> Router`. Keep engine/media/worker construction
and listener binding in `serve`; do not invent an application factory trait.
Have `serve` call the extracted function.

Add `api_contract.rs` using `tower::ServiceExt::oneshot` and a temporary
file-backed SQLite database. Build an `AppState` with `MockFaceEngine`, an empty
gallery, a broadcast channel, no vision handle, and default media status.
Cover:

1. public health returns 200 and contains `status`, `cameras`, and `media`;
2. a protected route without a bearer token returns 401 with `detail`;
3. invalid login returns 401;
4. an unknown route returns 404;
5. route construction starts no listener, worker, or child process.

Use `std::env::temp_dir()` plus a UUID and explicit cleanup; do not add a
temporary-file crate solely for this fixture.

**Verify**: `cd apps/edge && cargo test -p pksp-api --locked` → all new tests pass.

### Step 4: Add minimal active-stack CI

Create one workflow with two jobs or two clear groups:

- Rust: stable toolchain, Cargo cache, `cargo test --locked`, then
  `cargo check --all-features --locked`.
- Web: Node `20.x` (Next 16 requires at least 20.9), `npm ci`, `npm run lint`, `npm run typecheck`,
  `npm test -- --run`, and `npm run build`.

Do not add deployment, release, Docker, model download, camera, or legacy jobs.

**Verify**: inspect the YAML locally, then rerun all six commands in the table
above; every command must exit 0.

### Step 5: Document the one-command gates

Update `README.md` so contributors see the Rust and web verification commands
that now actually work. Replace active Next.js 15 statements in `README.md`,
`plans/07-frontend-ui.md`, and
`rust-port-plans/11-frontend-integration.md` with the installed Next.js 16
baseline. Do not rewrite historical decisions or unrelated plan prose.

**Verify**:
`rg -n "next lint|Next\.js 15|Next 15" README.md plans/07-frontend-ui.md rust-port-plans/11-frontend-integration.md apps/web/package.json apps/web/eslint.config.mjs`
→ no stale active instruction remains.

## Test plan

- One pure URL test in `apps/web/src/lib/whep.test.ts` and one jsdom/alias/RTL
  harness test in `apps/web/src/test/harness.test.tsx`.
- Five API boundary cases in `apps/edge/crates/pksp-api/tests/api_contract.rs`.
- No network, model, camera, external process, real biometric, or wall-clock
  dependency.
- Full verification is the six-command table above.

## Done criteria

- [ ] All six commands in “Commands you will need” exit 0.
- [ ] `cargo test -p pksp-api --locked -- --list` lists the new route tests.
- [ ] `npm test -- --run` discovers and passes a real assertion-based test.
- [ ] `rg -n 'next lint|FlatCompat' apps/web` returns no matches outside lockfile metadata.
- [ ] CI contains no legacy backend, model download, camera, or deployment step.
- [ ] No file outside the in-scope list changed.
- [ ] Plan 001 is marked `DONE` in `advisor-plans/README.md`.

## STOP conditions

- Installed `eslint-config-next` does not expose a usable native flat config;
  report the installed exports instead of downgrading Next.js.
- Router extraction requires starting media/vision side effects; report the
  coupling before creating a larger factory abstraction.
- A test needs a real model, camera, internet connection, or private data.
- `package-lock.json` is not the active npm lockfile.
- Any in-scope current-state excerpt has materially drifted.

## Maintenance notes

- Plans 007–018 should add focused tests to this harness, not create another.
- Plan 002 adds fmt/Clippy to CI only after the existing workspace is clean.
- Reviewers should reject broad router architecture or test-framework growth;
  this plan exists only to make active behavior cheaply verifiable.
