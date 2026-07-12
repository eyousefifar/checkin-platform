# 09 — Security & Privacy

## Context

Face recognition for attendance processes **biometric personal data**. Even a CEO demo must not be careless. This MVP is **LAN-only** and **not** a compliance-certified system.

## Data classification

| Data | Sensitivity | Storage |
|---|---|---|
| Face images (enrollment) | High (biometric) | Local disk `data/enroll/` |
| Face embeddings | High (biometric derivative) | SQLite BLOB |
| Attendance timestamps | Medium (HR) | SQLite |
| Live video | High | Ephemeral — **do not record by default** |
| Admin password | High | Env / hashed if stored |

## MVP security controls

1. **No public internet exposure** — bind services to LAN; do not port-forward casually.
2. **Admin authentication** — password login for UI/API mutations.
3. **Disclaimer** — UI states research models + not official HR system of record.
4. **Minimal retention** — no continuous video archive in MVP.
5. **Local processing** — frames never sent to cloud APIs.
6. **Secrets in env** — not in git.
7. **File permissions** — restrict `data/` to operator user.

## Explicit non-controls (honest gaps)

- No enterprise SSO / RBAC
- No disk encryption enforced by app (rely on OS FileVault)
- No audit log of every admin action (add before production)
- No formal DPIA / legal review completed by this plan
- Anti-spoof is **demo-grade**, not PAD certified
- WebRTC/API may be plain HTTP on LAN (TLS later)

## Biometric policy recommendations (for company counsel)

Before real employee rollout:

1. Written purpose limitation (attendance only)
2. Employee notice / consent where required by law
3. Retention schedule (e.g. embeddings while employed + N days; attendance M months)
4. Access control list (who can view faces / exports)
5. Deletion workflow on employee exit
6. Model license compliance for commercial operation
7. Works council / labor rules if applicable in jurisdiction

## Retention defaults (configure)

| Asset | Demo default | Production suggestion |
|---|---|---|
| Enrollment images | Until deleted | Until employee offboarded |
| Embeddings | Until deleted | Rebuild or delete on offboard |
| Attendance events | Keep | Policy-driven (e.g. 12–24 months) |
| Unrecognized events | 7 days purge optional | Shorter |
| Raw video | Not stored | Only if legal basis + secure NVR |

## Threat model (lightweight)

| Threat | Impact | MVP mitigation |
|---|---|---|
| LAN attacker opens UI | Privacy / fake attendance | Password; later TLS+SSO |
| Printed photo spoof | False attendance | Optional MiniFASNet; physical camera placement |
| Model extraction | IP / privacy | Local only; no public API |
| Wrong identity match | Wrong attendance | Threshold + margin + voting |
| Insider export CSV | HR leak | Limit admin accounts |
| Camera RTSP brute force | Video peek | Camera passwords; VLAN |

## Logging hygiene

- Do not log embedding vectors
- Do not log base64 faces
- OK: employee_id, event kind, score, camera_id, timestamps
- Redact RTSP passwords in logs (URL sanitizer)

## License compliance

- Display non-commercial model notice when using buffalo_l
- Track third-party notices (InsightFace, ONNX, OpenCV, MediaMTX, MiniFASNet)
- Before production: switch to licensed/commercial-safe weights

## Secure development notes

- Validate image uploads (type, size, decode with Pillow/OpenCV safely)
- Path traversal guard on `file_path`
- SQL via ORM only
- CORS locked to known origins
- Disable `/docs` or protect when not developing

## Incident response (demo scale)

If laptop lost:

1. Assume biometrics exposed
2. Rotate admin password / JWT secret on rebuild
3. Revoke VPN/LAN access if any
4. Re-enroll employees on clean machine if needed
5. Notify stakeholders per company policy

## Production hardening backlog

- [ ] TLS reverse proxy
- [ ] SSO / OIDC
- [ ] Role: admin vs viewer
- [ ] Encrypted backups
- [ ] Audit trail
- [ ] Automated retention jobs
- [ ] Commercial model license
- [ ] Security review + legal sign-off
