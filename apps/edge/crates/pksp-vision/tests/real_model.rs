//! Operator-owned real-model gate for buffalo_l SCRFD + ArcFace.
//!
//! Set `PKSP_VISION_FIXTURE_DIR` to a local directory containing:
//! - `manifest.json`: identity-labelled enroll/query/unregistered frames (see deploy.md)
//! - image files referenced by the manifest
//! - optional `*.emb.json` float arrays for cosine≥0.99 checks
//!
//! Never commit fixture images or embeddings.

use pksp_core::{match_top1, mean_l2_embedding, quality_gate_extended};
use pksp_vision::{crop_gray_luma, FaceEngine, OrtFaceEngine};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct Manifest {
    images: Vec<ManifestImage>,
}

#[derive(Debug, Deserialize)]
struct ManifestImage {
    file: String,
    faces: usize,
    #[serde(default)]
    expect_embedding: bool,
    #[serde(default)]
    expected_emb: Option<String>,
    #[serde(default)]
    identity: Option<String>,
    #[serde(default)]
    role: Option<FixtureRole>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FixtureRole {
    Enroll,
    Query,
    Unregistered,
}

fn fixture_dir() -> Option<PathBuf> {
    std::env::var_os("PKSP_VISION_FIXTURE_DIR").map(PathBuf::from)
}

fn load_bgr(path: &Path) -> (u32, u32, Vec<u8>) {
    let img = image::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let mut bgr = Vec::with_capacity((w * h * 3) as usize);
    for p in rgb.pixels() {
        bgr.push(p[2]);
        bgr.push(p[1]);
        bgr.push(p[0]);
    }
    (w, h, bgr)
}

fn model_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../data/models/buffalo_l")
}

fn passes_production_quality(
    face: &pksp_vision::DetectedFace,
    bgr: &[u8],
    width: u32,
    height: u32,
) -> bool {
    let Some((gray, crop_w, crop_h)) = crop_gray_luma(bgr, width, height, face.bbox) else {
        return false;
    };
    quality_gate_extended(
        face.det_score,
        face.bbox,
        0.5,
        60,
        width as i32,
        height as i32,
        false,
        face.landmarks.as_ref(),
        Some((&gray, crop_w, crop_h)),
        30.0,
        75.0,
        0.0,
        255.0,
    )
    .ok
}

#[test]
#[ignore = "operator-owned fixtures; set PKSP_VISION_FIXTURE_DIR"]
fn real_model_fixture_gate() {
    let dir = fixture_dir().expect("PKSP_VISION_FIXTURE_DIR must be set");
    let manifest_path = dir.join("manifest.json");
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(&manifest_path).expect("read manifest"))
            .expect("parse manifest");

    let engine = OrtFaceEngine::try_load_with(&model_dir(), 640, "CPUExecutionProvider");
    assert!(
        engine.ready(),
        "buffalo_l models must load from {}",
        model_dir().display()
    );

    let mut enrollments: HashMap<String, Vec<Vec<f32>>> = HashMap::new();
    let mut queries: Vec<(String, Vec<f32>)> = Vec::new();
    let mut unregistered: Vec<Vec<f32>> = Vec::new();

    for entry in &manifest.images {
        let path = dir.join(&entry.file);
        let (w, h, bgr) = load_bgr(&path);
        let faces = engine
            .detect_and_embed(w, h, &bgr)
            .unwrap_or_else(|e| panic!("{}: {e}", entry.file));
        assert_eq!(faces.len(), entry.faces, "{} face count", entry.file);
        for f in &faces {
            assert!(f.det_score.is_finite());
            assert!(f.bbox.0.is_finite() && f.bbox.2 > f.bbox.0);
            let lm = f.landmarks.expect("landmarks required");
            assert!(lm.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
            if entry.expect_embedding {
                assert_eq!(f.embedding.len(), 512);
                assert!(f.embedding.iter().all(|x| x.is_finite()));
                let n: f32 = f.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                assert!((n - 1.0).abs() < 1e-3, "unit norm {n}");
            }
        }
        if entry.expect_embedding && entry.faces > 0 {
            // Determinism
            let again = engine.detect_and_embed(w, h, &bgr).unwrap();
            let cos: f32 = faces[0]
                .embedding
                .iter()
                .zip(again[0].embedding.iter())
                .map(|(a, b)| a * b)
                .sum();
            assert!(cos >= 0.9999, "repeat cosine {cos}");
            if let Some(ref emb_file) = entry.expected_emb {
                let expected: Vec<f32> = serde_json::from_slice(
                    &std::fs::read(dir.join(emb_file)).expect("read expected emb"),
                )
                .expect("parse emb");
                assert_eq!(expected.len(), 512);
                let cos: f32 = faces[0]
                    .embedding
                    .iter()
                    .zip(expected.iter())
                    .map(|(a, b)| a * b)
                    .sum();
                assert!(cos >= 0.99, "reference cosine {cos}");
            }
        }

        if let Some(role) = entry.role {
            assert_eq!(
                faces.len(),
                1,
                "{} identity gate needs one face",
                entry.file
            );
            let face = &faces[0];
            if !passes_production_quality(face, &bgr, w, h) {
                continue;
            }
            match role {
                FixtureRole::Enroll => {
                    let identity = entry
                        .identity
                        .clone()
                        .expect("enroll frame requires identity");
                    enrollments
                        .entry(identity)
                        .or_default()
                        .push(face.embedding.clone());
                }
                FixtureRole::Query => {
                    let identity = entry
                        .identity
                        .clone()
                        .expect("query frame requires identity");
                    queries.push((identity, face.embedding.clone()));
                }
                FixtureRole::Unregistered => unregistered.push(face.embedding.clone()),
            }
        }
    }

    if !enrollments.is_empty() || !queries.is_empty() || !unregistered.is_empty() {
        assert!(!enrollments.is_empty(), "identity gate needs enroll frames");
        assert!(
            !queries.is_empty(),
            "identity gate needs known-person query frames"
        );
        assert!(
            !unregistered.is_empty(),
            "identity gate needs unregistered-person query frames"
        );

        let mut identities: Vec<String> = enrollments.keys().cloned().collect();
        identities.sort();
        let ids: Vec<i64> = (1..=identities.len() as i64).collect();
        let gallery: Vec<Vec<f32>> = identities
            .iter()
            .map(|identity| {
                let vectors = &enrollments[identity];
                assert!(
                    vectors.len() >= 5,
                    "{identity} needs five quality-passing enrollment frames"
                );
                mean_l2_embedding(vectors, 512).unwrap()
            })
            .collect();

        let mut correct = 0usize;
        for (identity, embedding) in &queries {
            let result = match_top1(embedding, &gallery, &ids, &identities, 0.75, 0.10);
            if result.employee_id.is_some() {
                assert_eq!(
                    &result.label, identity,
                    "false accept: {identity} matched {} ({:.3}, margin {:.3})",
                    result.label, result.score, result.margin
                );
                correct += 1;
            }
        }
        assert!(
            correct * 10 >= queries.len() * 9,
            "known-person recall below 90%: {correct}/{}",
            queries.len()
        );

        for embedding in &unregistered {
            let result = match_top1(embedding, &gallery, &ids, &identities, 0.75, 0.10);
            assert!(
                result.employee_id.is_none(),
                "unregistered false accept: {} ({:.3}, margin {:.3})",
                result.label,
                result.score,
                result.margin
            );
        }
    }
}

/// Zero-face smoke when models exist but fixtures are optional (CI-friendly smoke).
#[test]
fn real_model_blank_frame_zero_faces() {
    let md = model_dir();
    if !md.join("det_10g.onnx").is_file() || !md.join("w600k_r50.onnx").is_file() {
        eprintln!("skip: models not present at {}", md.display());
        return;
    }
    let engine = OrtFaceEngine::try_load_with(&md, 640, "CPUExecutionProvider");
    if !engine.ready() {
        eprintln!("skip: ort engine not ready");
        return;
    }
    let w = 320u32;
    let h = 240u32;
    let bgr = vec![0u8; (w * h * 3) as usize];
    let faces = engine.detect_and_embed(w, h, &bgr).expect("blank frame ok");
    assert!(faces.is_empty(), "blank frame should have zero faces");
}
