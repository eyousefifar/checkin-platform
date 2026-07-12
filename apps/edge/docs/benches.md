# Benchmarks (M6)

Recorded on local dev hardware. Re-run after major vision changes:

```bash
cd apps/edge
cargo test -p pksp-core --release -- --nocapture
# optional:
# cargo bench  # if benches added later
```

## Core (pure, no I/O)

| Op | Target | Notes |
|---|---|---|
| cosine match N≤50 | ≪ 0.05 ms | `match_top1` |
| quality + track 5 faces | ≪ 0.05 ms | |
| vote evaluate | negligible | |

Default unit tests exercise these paths; wall-clock infer depends on ONNX EP and resolution.

## Vision (when models present)

| Op | Target |
|---|---|
| process rate cam_in | ≥ 5 FPS sustained |
| enroll single image | interactive |

## Media

| Op | Target |
|---|---|
| WHEP startup | < 3 s typical LAN |
| transcoder restart | video returns after kill |

Capture results under `artifacts/verify-rust/` (gitignored) when verifying a release.
