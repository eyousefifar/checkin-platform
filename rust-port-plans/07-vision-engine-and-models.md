# 07 — Vision Engine & Models

## 1. Goals

- Run the same **buffalo_l** ONNX pair (SCRFD det + ArcFace rec) as Python InsightFace for embedding compatibility.
- Provide **MockFaceEngine** for CI/demo without weights.
- Share one engine path for **live worker** and **enrollment**.
- Optimize with ONNX Runtime execution providers; never require Python.

## 2. Python baseline

`apps/api/app/services/vision/engine.py`:

- `FaceEngine` protocol: `ready`, `model_name`, `get(image_bgr) -> [DetectedFace]`
- `DetectedFace`: `bbox` xyxy pixels, `det_score`, `embedding` L2-normalized 512-d
- InsightFace `FaceAnalysis(name=buffalo_l, allowed_modules=["detection","recognition"])`
- Providers from `ONNX_PROVIDERS`
- Mock: intensity-bucket synthetic embeddings; centered pseudo-face

## 3. Rust `FaceEngine` trait

```rust
pub struct DetectedFace {
    pub bbox: [f32; 4],      // x1,y1,x2,y2 pixels
    pub det_score: f32,
    pub embedding: Array1<f32>, // dim 512 L2
    pub landmarks: Option<[[f32;2];5]>, // for pose (extension)
}

pub trait FaceEngine: Send + Sync {
    fn ready(&self) -> bool;
    fn model_name(&self) -> &str;
    fn execution_provider(&self) -> &str;
    fn detect_and_embed(&self, image_bgr: &ImageBgr) -> Result<Vec<DetectedFace>, VisionError>;
}
```

Implementations:

| Impl | When |
|---|---|
| `MockFaceEngine` | `MOCK_VISION=true` or tests |
| `OrtBuffaloEngine` | real models |
| (optional) `FaceIdEngine` | if crate spike succeeds |

## 4. buffalo_l pipeline (custom ort)

### 4.1 Detection (SCRFD)

1. Letterbox / resize to `det_size` (default 640) keeping aspect  
2. BGR→RGB, normalize per InsightFace SCRFD convention (document exact mean/std from reference impl)  
3. Run `det_10g.onnx`  
4. Decode scores, bboxes, **5 landmarks**; NMS  
5. Map boxes back to original pixel coords  

### 4.2 Alignment

1. For each face, use 5 landmarks  
2. Similarity transform to ArcFace template (112×112)  
3. Warp with bilinear sampling  

Reference implementations: InsightFace Python align, DefTruth lite.ai.toolkit ArcFace C++, `face_id` crate internals.

### 4.3 Recognition

1. Input 112×112 RGB normalized (ArcFace standard)  
2. Run `w600k_r50.onnx`  
3. L2-normalize 512-d output  

### 4.4 Order of operations for quality (faster)

```
detect → cheap quality (score, size, aspect) 
       → [optional pose/blur] 
       → align+embed only if quality_ok OR need label for HUD low-quality path
```

Python currently embeds all faces then quality-filters for voting. **Improvement:** skip embedding for faces that fail hard size/score gates (still draw LOW QUALITY if det exists — may use det-only for HUD).

## 5. Execution providers

| EP | Platform | Config |
|---|---|---|
| `CPUExecutionProvider` | all | default |
| `OpenVINOExecutionProvider` | Intel Linux | feature `openvino` |
| `CoreMLExecutionProvider` | macOS | feature `coreml` |
| `CUDAExecutionProvider` | NVIDIA | optional |

Settings: `ONNX_PROVIDERS` comma list; fall back to CPU if unavailable (mirror Python selection logic).

Thread env:

- `ORT_INTRA_OP_NUM_THREADS` / settings fields  
- Avoid oversubscription vs tokio workers (e.g. leave 2 cores for runtime)

## 6. Model packaging

| Strategy | Pros | Cons |
|---|---|---|
| Download script (like `scripts/download_models.sh`) | small git | first-run dep |
| Models dir under `data/models/buffalo_l/` | explicit | large |
| HuggingFace path via `face_id` | convenient | network, license clarity |

Document non-commercial banner forever for buffalo_l weights.

Commercial path (later): AuraFace Apache weights or YuNet+SFace — same engine trait.

## 7. Mock engine parity

Port behavior for tests:

- Dark/tiny image → no faces  
- Else one centered face, high det_score  
- Embedding from deterministic RNG seeded by mean intensity bucket  
- Small per-image noise so same bucket still matches  

Used when `MOCK_VISION=true` with synthetic frames or enroll of solid-color images.

## 8. Enrollment path

Share `FaceEngine` with live:

1. Decode bytes → BGR (`image` crate → ndarray)  
2. `detect_and_embed`  
3. Largest face if multiple (or reject multi_face in strict mode)  
4. quality_gate  
5. pack mean after ≥ `min_enroll_images`  

**Improvement:** store per-image embeddings optional for future hard-negative mining (not required for parity).

## 9. Gallery integration

`Gallery` holds:

```rust
employee_ids: Vec<i64>,
names: Vec<String>,
matrix: Array2<f32>, // (N, 512) row-normalized
version: u64,
threshold: f32,
margin: f32,
```

`match_embedding(&self, q: &Array1<f32>) -> MatchResult` calls `pksp-core::match`.

Reload on version bump via `Notify` or atomic version check (cleaner than Python poll every frame + DB).

## 10. Faster / simpler / cleaner

| Opportunity | Detail |
|---|---|
| Skip genderage | already disabled in Python |
| Embed only quality-ok | CPU win |
| One session per model | reuse Ort Session |
| Shared engine Arc | enroll + live no double load |
| det_size adaptive | 320 under load |
| ROI crop from zones | smaller det input (smart scene) |
| Batch faces | optional later if multi-face common |

## 11. Spike plan (M1)

1. Load ONNX files with ort on target machine  
2. Run det on sample enroll image; compare box count to Python InsightFace  
3. Compare cosine(emb_rust, emb_python) ≥ 0.99 on same image (alignment correctness)  
4. If cosine ≪ 0.99 → fix normalize/align before any DB use  

**Critical:** embedding space must match or all existing enrollments break.

## 12. Acceptance criteria

- [ ] Trait + Mock + Ort implementations  
- [ ] Cross-check cosine vs Python ≥ 0.99 on fixtures  
- [ ] EP fallback works  
- [ ] Genderage not loaded  
- [ ] License/disclaimer documented  
- [ ] Enroll and live use same engine instance path  

## 13. Source map

| Python | Rust |
|---|---|
| `engine.py` | `pksp-vision/src/engine/{mod,mock,ort_buffalo}.rs` |
| `gallery/service.py` | `pksp-vision/src/gallery.rs` |
| `enroll.py` extract pieces | `pksp-vision/src/enroll.rs` + api handlers |
| `scripts/download_models.sh` | `pksp-cli models download` or keep script |
