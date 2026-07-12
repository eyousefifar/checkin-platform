# Plan 003: Contain credentials, JWTs, and biometric files

> **Executor instructions**: Never print, paste, or commit any credential or
> token value while executing this plan. Refer only to credential type and
> file location. External credential rotation belongs to the operator. Update
> plan 003 in `advisor-plans/README.md` only after every done criterion holds.
>
> **Drift check (run first)**:
> `git diff --stat d6ef664..HEAD -- .env.example README.md camera_issue_fix.md plans/08-infra-and-deploy.md configs apps/edge/crates/pksp-db apps/edge/crates/pksp-api apps/edge/docs/deploy.md apps/web/src/lib/api.ts apps/web/src/lib/api.test.ts advisor-plans/README.md`

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MED
- **Depends on**: `advisor-plans/001-active-stack-verification.md`
- **Category**: security
- **Planned at**: commit `d6ef664`, 2026-07-12

## Why this matters

Credential-bearing RTSP configuration appears in tracked documentation and
configuration, browser JWTs are placed in a query string that the default HTTP
trace span can record, and biometric files are currently readable beyond the
service user. The Rust server also accepts built-in demo secrets while binding
to all interfaces. These are direct LAN privacy and account-compromise risks.

## Current state

Do not copy the values at these locations:

- `README.md:76`, `.env.example:21-23`, `configs/mediamtx.yml:29`, and
  `camera_issue_fix.md:121,161` contain camera credential material or
  credential-bearing examples.
- `plans/08-infra-and-deploy.md` also contains user-info-shaped RTSP examples;
  historical planning prose is not exempt from the tracked-file audit.
- `apps/edge/crates/pksp-db/src/lib.rs:80-82` supplies built-in admin/JWT
  fallback values; `:137` defaults `BIND_ADDR` to all interfaces.
- `apps/edge/crates/pksp-api/src/lib.rs:188-192` silently converts malformed
  bind text into another all-interface bind.
- `apps/web/src/lib/api.ts:47-53` appends the JWT as `?token=` for WebSocket
  authentication.
- `apps/edge/crates/pksp-api/src/lib.rs:185` installs
  `TraceLayer::new_for_http()`, whose default span includes the full URI.
- `apps/edge/crates/pksp-db/src/lib.rs:212-219` and
  `apps/edge/crates/pksp-vision/src/lib.rs:1116-1178` create/write DB and
  enrollment paths without restrictive Unix modes.
- The local audit observed `.env` and SQLite files as `0644` and data/enrollment
  directories as `0755`.
- `apps/edge/docs/deploy.md:70-87` has a systemd sketch without `UMask`.

Security constraints from `plans/09-security-privacy.md`: secrets stay in env,
biometric data stays local and restricted to the operator, raw video is not
recorded, and logs never contain RTSP passwords, embeddings, or faces.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| API/security tests | `cd apps/edge && cargo test -p pksp-api -p pksp-db --locked` | exit 0 |
| Rust suite | `cd apps/edge && cargo test --locked` | exit 0 |
| Web typecheck | `cd apps/web && npm run typecheck` | exit 0 |
| RTSP user-info audit | `! git grep -IlE 'rtsp://[^/@:[:space:]]+:[^/@[:space:]]+@' -- ':!advisor-plans/**'` | exit 0, no output |
| Sensitive assignment audit | `git grep -IlE '(ADMIN_PASSWORD|JWT_SECRET|CAM_[A-Z_]*RTSP)[[:space:]]*[:=][[:space:]]*[^$<{[:space:]#]' -- ':!advisor-plans/**'` | filenames only; every match is removed, variable-only, or explicitly redacted |

## Scope

**In scope**:

- `.env.example`, `README.md`, `camera_issue_fix.md`,
  `plans/08-infra-and-deploy.md`
- `configs/mediamtx.yml`, `apps/edge/configs/mediamtx.yml`
- `apps/edge/crates/pksp-db/src/lib.rs`, including its inline settings tests
- `apps/edge/crates/pksp-api/src/lib.rs`, `src/auth.rs`, and
  `apps/edge/crates/pksp-api/tests/security.rs` (create)
- `apps/web/src/lib/api.ts` and `apps/web/src/lib/api.test.ts` (create)
- `apps/edge/docs/deploy.md`
- `advisor-plans/README.md`

**Out of scope**:

- Any real `.env` value, camera administration, secret-manager product,
  enterprise RBAC/SSO/TLS, disk encryption, and `apps/api/**`.
- Git-history rewriting without explicit operator approval.
- Removing server-side optional query-token compatibility; the active browser
  stops sending it, but frozen external clients remain accepted.

## Git workflow

- Branch: `codex/003-security-containment`
- Commit message: `Harden edge credential and biometric handling`
- Do not deploy the branch until the operator confirms credential rotation and
  the rollout checklist below. Repository tests and review may proceed without
  camera or host access.

## Steps

### Step 1: Remove tracked credential material and rotate externally

Replace every tracked credential-in-URL user-info shape, including examples in
`plans/08-infra-and-deploy.md`, with variable-only or visibly non-URL
placeholders. No tracked document may teach readers to place credentials in URL
user-info. Document the required variable names without values.

The operator must rotate the camera credential because deletion from the tip
does not invalidate historical exposure. Record only “rotation confirmed” in
the plan status/PR, never the value.

**Verify**: run both audit commands in the table. The RTSP command must return
no file. The assignment command deliberately prints filenames only; inspect
each named file without copying values and classify every match as an empty,
environment-substituted, or visibly redacted example. If `gitleaks` is already
installed, `gitleaks detect --no-git --redact` is an optional additional check,
not a required dependency or substitute for the targeted audit.

### Step 2: Default to loopback and fail closed on unsafe binds

Change the default bind to `127.0.0.1:8000`. Add one pure settings validation
function called before `connect_pool`, model load, or child-process startup.
Malformed bind text must return an error, never fall back to a wildcard.
Loopback-only demo mode may retain development fallbacks. Any non-loopback IPv4
or IPv6 bind must require an explicitly supplied, non-demo `ADMIN_PASSWORD` and
an explicitly supplied JWT secret of at least 32 bytes, returning an actionable
error that names only the invalid variable.

Do not add a configuration framework. Reuse `Settings`, `SocketAddr`, and
`std::env`.

Tests must cover loopback demo acceptance, IPv4/IPv6 wildcard rejection with
either missing secret, non-loopback acceptance with explicit non-secret
fixtures, and malformed bind rejection.

**Verify**: `cd apps/edge && cargo test -p pksp-api -p pksp-db --locked` → pass.

### Step 3: Remove browser query tokens and redact request spans

Simplify `wsUrl()` to return the configured/base WebSocket URL without reading
or appending the browser token. Keep the Rust handler’s optional query-token
support for frozen-client compatibility. Replace the default trace span fields
with a small closure that records method and `uri().path()` only. It is valid to
build this from `TraceLayer::new_for_http().make_span_with(...)`. Do not record
`uri().query()`, authorization headers, or request bodies.

Add a focused unit test around a pure path-label helper or span-field input so
a benign query fixture never appears in the label.

**Verify**: `security.rs` proves a request containing a benign query marker
records only the path, and `api.test.ts` proves `wsUrl()` does not read or append
a token. API tests and web typecheck pass;
`rg -n 'encodeURIComponent\(token\)|[?&]token=' apps/web/src/lib/api.ts`
returns no match. Do not reject `new_for_http()` itself; reject any span field
derived from a full URI, query, headers, or body.

### Step 4: Enforce owner-only modes at the process/deployment boundary

Use the standard Unix process umask rather than partial per-file chmod code:

- direct-run instructions begin with `umask 077`;
- `.env` is `0600` and `DATA_DIR` is owner-only before startup;
- systemd uses a dedicated `User`/`Group`, `UMask=0077`, and an owner-readable
  environment file outside the working tree;
- the one-time repair stops the service, sets directories `0700` and existing
  DB/WAL/SHM/enrollment files `0600`, then restarts.

SQLite creates WAL/SHM internally, so a Rust chmod helper would cover only part
of the data. Do not add one. On non-Unix systems, document reliance on the OS
ACL/disk controls instead of inventing a portability abstraction.

Repository implementation stops at the runbook and service definition. It must
not run `chmod`, change ownership, stop a service, or inspect a real host unless
the operator separately authorizes deployment work. The operator rollout gate
is: stop the service, confirm the service account owns the tree, apply the
documented modes, restart, then verify process umask `0077`; a find for
group/other permission bits under `DATA_DIR` prints nothing; explicit stat shows
the environment file `0600`, data directory `0700`, and DB/WAL/SHM/enrollment
files `0600`.

### Step 5: Harden deployment notes

Add the dedicated user/group and `UMask=0077` to the systemd service sketch,
document the stopped-service one-time permission repair, and state that `.env`
must be service-user-only.
Do not include any credential value or a public deployment recipe.

**Verify**: `rg -n 'UMask=0077|chmod 700|chmod 600' apps/edge/docs/deploy.md` →
the service and repair guidance are present.

## Test plan

- Pure settings-validation matrix: loopback/non-loopback × explicit/missing
  secrets.
- Request path-label test proving query text is omitted.
- Web URL test proving `wsUrl()` never reads/appends a token.
- Runbook review plus, only when separately authorized, deployment-host
  umask/mode verification for DB, WAL/SHM, and enrollment data.
- Full active Rust suite after focused tests.
- No security test may contain a real credential, token, biometric, or private
  address.

## Done criteria

- [ ] No tracked credential-bearing RTSP URL remains.
- [ ] Non-loopback startup fails before DB/media startup when required secrets
  are not explicit.
- [ ] Default bind is loopback and malformed bind text is rejected.
- [ ] Browser-generated WebSocket URLs contain no JWT.
- [ ] HTTP trace spans contain path but not query text.
- [ ] Systemd example includes `UMask=0077`.
- [ ] Focused and full Rust tests pass.
- [ ] The PR records the two still-external rollout gates—credential rotation
  and deployed mode verification—without claiming either happened.
- [ ] No file outside scope changed; plan 003 is marked `DONE` for repository
  implementation. Deployment remains prohibited until both rollout gates pass.

## STOP conditions

- The operator asks for a Git history rewrite without separately approving the
  destructive coordination, force-push, and collaborator re-clone impact.
- Rotation requires camera/network access the executor does not have.
- Real-host permission repair or service restart is requested without explicit
  deployment authority; finish the repository work and report the rollout gate
  instead of touching the host.
- Tightening permissions would make the configured service user unable to read
  existing data; report ownership/mode facts without changing them further.
- Live WebSocket access is now required to be private; stop and plan a secure
  cookie or first-message authentication contract instead of putting JWTs back
  in URLs.
- Any command output would expose a secret; stop and redact before reporting.

## Maintenance notes

- Review logs whenever request tracing changes; full URIs and headers are
  sensitive by default.
- Backup/restore tooling must preserve or reapply restrictive modes.
- TLS, SSO, audit trails, retention automation, and encrypted backups remain
  explicitly deferred until the product leaves LAN demo scope.
