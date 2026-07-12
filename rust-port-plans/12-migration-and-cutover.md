# 12 — Migration & Cutover

## 1. Principles

1. Prefer **reuse** of `data/pksp.db` and `data/enroll/`.  
2. Keep Python tree until Rust passes verification (dual-run possible).  
3. Cut over service-by-service if needed (API first, media later).  
4. Always have a rollback path to `uvicorn` + MediaMTX.

## 2. Phased cutover

### Phase 0 — Docs (this folder)

No runtime change.

### Phase 1 — Core + tests only

`pksp-core` published in-repo; no production traffic.

### Phase 2 — Rust API + mock vision

- Point `NEXT_PUBLIC_API_URL` to Rust `:8000`  
- Media still MediaMTX + existing transcoder  
- Python stopped  

### Phase 3 — Real ONNX vision

- Validate embedding cosine vs Python fixtures  
- Re-enroll only if cosine mismatch (avoid if spike succeeds)  

### Phase 4 — Media supervised by Rust

- `pksp-media` spawns MediaMTX + transcoder  
- Remove manual scripts  

### Phase 5 — Smart scene on

- Enable zones/trajectory  
- Tune on live door  

### Phase 6 — Optional GStreamer WHEP

- Drop MediaMTX if stable  

## 3. Database migration

### Empty install

```
pksp migrate
pksp serve
```

Creates schema + camera upsert.

### Existing Python DB

1. Stop Python API (SQLite single writer).  
2. Backup: `cp data/pksp.db data/pksp.db.bak-$(date +%Y%m%d)`.  
3. Run `pksp migrate` with baseline:
   - If tables exist and match, mark migration applied.  
   - If drift, write `002_fix_*.sql` or import path.  
4. Start Rust; verify employee count + embeddings length 2048.  
5. Spot-check match scores against known person.

### Embedding compatibility gate

Before cutover:

```
cosine(python_emb, rust_emb) on same enroll image ≥ 0.99
```

If not met → **do not** reuse gallery; re-enroll all users under Rust engine.

## 4. Dual-run (optional)

| Service | Port |
|---|---|
| Python API | 8000 |
| Rust API | 8001 |
| Web → toggle env | |

Use for contract comparison (record health/employees/daily JSON).

Media stays shared (one MediaMTX). **Do not** dual-run two vision workers on same camera commits or attendance doubles — run vision only on one side.

## 5. Rollback

1. Stop `pksp`.  
2. Restore `pksp.db.bak` if schema changed.  
3. Start Python `uvicorn` + previous MediaMTX.  
4. Point web env back if needed.  

Keep Python code in repo until M5 exit criteria green for ≥1 week of use (or demo success).

## 6. Config migration

| Python env | Rust |
|---|---|
| same names | prefer **identical env var names** for drop-in |
| `.env` | dotenvy load |

Document any renames in a table if unavoidable.

## 7. Acceptance criteria

- [ ] Backup/restore procedure written and tested once  
- [ ] Embedding compatibility gate defined  
- [ ] Dual-run vision conflict warned  
- [ ] Rollback under 10 minutes  

## 8. Source map

| Artifact | Role |
|---|---|
| `data/pksp.db` | shared state |
| `data/enroll/` | shared images |
| `.env` | shared config |
| Python apps/api | rollback binary |
| Rust apps/edge | new binary |
