//! Face quality gate (+ optional blur/pose/exposure extensions).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityResult {
    pub ok: bool,
    pub reason: Option<String>,
}

/// `bbox_xyxy`: pixel coords unless `bbox_normalized` (then 0–1 relative to frame).
pub fn quality_gate(
    det_score: f32,
    bbox_xyxy: (f32, f32, f32, f32),
    min_det_score: f32,
    min_face_px: i32,
    frame_w: i32,
    frame_h: i32,
    bbox_normalized: bool,
) -> QualityResult {
    if !det_score.is_finite()
        || !bbox_xyxy.0.is_finite()
        || !bbox_xyxy.1.is_finite()
        || !bbox_xyxy.2.is_finite()
        || !bbox_xyxy.3.is_finite()
    {
        return QualityResult {
            ok: false,
            reason: Some("non_finite".into()),
        };
    }
    if det_score < min_det_score {
        return QualityResult {
            ok: false,
            reason: Some("low_det_score".into()),
        };
    }

    let (x1, y1, x2, y2) = bbox_xyxy;
    let (w, h) = if bbox_normalized {
        ((x2 - x1) * frame_w as f32, (y2 - y1) * frame_h as f32)
    } else {
        (x2 - x1, y2 - y1)
    };

    if w.min(h) < min_face_px as f32 {
        return QualityResult {
            ok: false,
            reason: Some("face_too_small".into()),
        };
    }
    if w <= 0.0 || h <= 0.0 {
        return QualityResult {
            ok: false,
            reason: Some("invalid_bbox".into()),
        };
    }
    let aspect = if h != 0.0 { w / h } else { 0.0 };
    if !(0.4..=2.5).contains(&aspect) {
        return QualityResult {
            ok: false,
            reason: Some("bad_aspect".into()),
        };
    }
    QualityResult {
        ok: true,
        reason: None,
    }
}

/// Laplacian variance of a grayscale face crop (row-major, width×height).
/// Higher = sharper. Typical reject threshold ~50–100 depending on resolution.
pub fn blur_variance(gray: &[u8], width: usize, height: usize) -> f32 {
    if width < 3 || height < 3 || gray.len() < width * height {
        return 0.0;
    }
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut n = 0.0f64;
    for y in 1..(height - 1) {
        for x in 1..(width - 1) {
            let i = y * width + x;
            let c = gray[i] as i32;
            let lap = gray[i - width] as i32
                + gray[i + width] as i32
                + gray[i - 1] as i32
                + gray[i + 1] as i32
                - 4 * c;
            let v = lap as f64;
            sum += v;
            sum_sq += v * v;
            n += 1.0;
        }
    }
    if n < 1.0 {
        return 0.0;
    }
    let mean = sum / n;
    ((sum_sq / n) - mean * mean).max(0.0) as f32
}

pub fn blur_ok(gray: &[u8], width: usize, height: usize, min_var: f32) -> bool {
    if min_var <= 0.0 {
        return true; // disabled
    }
    blur_variance(gray, width, height) >= min_var
}

/// Approximate yaw from 5-point landmarks: [left_eye, right_eye, nose, left_mouth, right_mouth].
/// Returns approximate |yaw| degrees (0 = frontal). Heuristic only — not a full 3D solve.
pub fn pose_yaw_approx(landmarks: &[[f32; 2]; 5]) -> f32 {
    let le = landmarks[0];
    let re = landmarks[1];
    let nose = landmarks[2];
    let eye_mid_x = (le[0] + re[0]) * 0.5;
    let eye_dist = (re[0] - le[0]).abs().max(1e-3);
    // nose offset relative to inter-ocular distance
    let offset = (nose[0] - eye_mid_x) / eye_dist;
    // map roughly: |offset| 0.5 ~ 45°
    (offset.abs() * 90.0).min(90.0)
}

pub fn pose_ok(landmarks: Option<&[[f32; 2]; 5]>, max_yaw_deg: f32) -> bool {
    if max_yaw_deg <= 0.0 {
        return true; // disabled
    }
    match landmarks {
        None => true, // no landmarks → don't reject
        Some(lm) => pose_yaw_approx(lm) <= max_yaw_deg,
    }
}

/// Mean luma in [0, 255] for exposure checks.
pub fn mean_luma(gray: &[u8]) -> f32 {
    if gray.is_empty() {
        return 0.0;
    }
    gray.iter().map(|&x| x as f32).sum::<f32>() / gray.len() as f32
}

pub fn exposure_ok(mean: f32, lo: f32, hi: f32) -> QualityResult {
    if mean < lo {
        return QualityResult {
            ok: false,
            reason: Some("low_light".into()),
        };
    }
    if mean > hi {
        return QualityResult {
            ok: false,
            reason: Some("high_glare".into()),
        };
    }
    QualityResult {
        ok: true,
        reason: None,
    }
}

/// Base gate AND optional extensions. Pass `None` / disabled thresholds to skip.
#[allow(clippy::too_many_arguments)] // ponytail: orchestration boundary; group only when another caller exists
pub fn quality_gate_extended(
    det_score: f32,
    bbox_xyxy: (f32, f32, f32, f32),
    min_det_score: f32,
    min_face_px: i32,
    frame_w: i32,
    frame_h: i32,
    bbox_normalized: bool,
    landmarks: Option<&[[f32; 2]; 5]>,
    gray_crop: Option<(&[u8], usize, usize)>,
    max_yaw_deg: f32,
    blur_min_var: f32,
    luma_lo: f32,
    luma_hi: f32,
) -> QualityResult {
    let base = quality_gate(
        det_score,
        bbox_xyxy,
        min_det_score,
        min_face_px,
        frame_w,
        frame_h,
        bbox_normalized,
    );
    if !base.ok {
        return base;
    }
    if !pose_ok(landmarks, max_yaw_deg) {
        return QualityResult {
            ok: false,
            reason: Some("low_pose".into()),
        };
    }
    if let Some((gray, w, h)) = gray_crop {
        if !blur_ok(gray, w, h, blur_min_var) {
            return QualityResult {
                ok: false,
                reason: Some("low_blur".into()),
            };
        }
        if luma_lo > 0.0 || luma_hi < 255.0 {
            let m = mean_luma(gray);
            let exp = exposure_ok(m, luma_lo, luma_hi);
            if !exp.ok {
                return exp;
            }
        }
    }
    QualityResult {
        ok: true,
        reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_low_score() {
        let r = quality_gate(
            0.2,
            (100.0, 100.0, 200.0, 200.0),
            0.5,
            60,
            1920,
            1080,
            false,
        );
        assert!(!r.ok);
        assert_eq!(r.reason.as_deref(), Some("low_det_score"));
    }

    #[test]
    fn rejects_small_face() {
        let r = quality_gate(0.9, (10.0, 10.0, 40.0, 40.0), 0.5, 60, 1920, 1080, false);
        assert!(!r.ok);
        assert_eq!(r.reason.as_deref(), Some("face_too_small"));
    }

    #[test]
    fn accepts_good_face() {
        let r = quality_gate(
            0.9,
            (100.0, 100.0, 220.0, 240.0),
            0.5,
            60,
            1920,
            1080,
            false,
        );
        assert!(r.ok);
    }

    #[test]
    fn normalized_bbox() {
        let r = quality_gate(0.9, (0.4, 0.3, 0.55, 0.55), 0.5, 60, 1920, 1080, true);
        assert!(r.ok);
    }

    #[test]
    fn sharp_patch_high_variance() {
        // checkerboard-ish
        let mut g = vec![0u8; 32 * 32];
        for y in 0..32 {
            for x in 0..32 {
                g[y * 32 + x] = if (x + y) % 2 == 0 { 0 } else { 255 };
            }
        }
        assert!(blur_variance(&g, 32, 32) > 100.0);
        assert!(blur_ok(&g, 32, 32, 50.0));
    }

    #[test]
    fn flat_patch_low_variance() {
        let g = vec![128u8; 32 * 32];
        assert!(blur_variance(&g, 32, 32) < 1.0);
        assert!(!blur_ok(&g, 32, 32, 50.0));
    }

    #[test]
    fn pose_frontal_ok() {
        let lm = [
            [40.0, 40.0],
            [80.0, 40.0],
            [60.0, 60.0],
            [45.0, 80.0],
            [75.0, 80.0],
        ];
        assert!(pose_ok(Some(&lm), 45.0));
        assert!(pose_yaw_approx(&lm) < 20.0);
    }

    #[test]
    fn pose_profile_rejects() {
        let lm = [
            [20.0, 40.0],
            [50.0, 40.0],
            [55.0, 60.0], // nose far right of eye mid
            [25.0, 80.0],
            [50.0, 80.0],
        ];
        assert!(!pose_ok(Some(&lm), 30.0));
    }

    #[test]
    fn exposure_bounds() {
        assert!(!exposure_ok(5.0, 20.0, 230.0).ok);
        assert!(!exposure_ok(250.0, 20.0, 230.0).ok);
        assert!(exposure_ok(100.0, 20.0, 230.0).ok);
    }

    #[test]
    fn extended_rejects_blur() {
        let flat = vec![100u8; 64 * 64];
        let r = quality_gate_extended(
            0.9,
            (100.0, 100.0, 220.0, 240.0),
            0.5,
            60,
            1920,
            1080,
            false,
            None,
            Some((&flat, 64, 64)),
            0.0,
            50.0,
            0.0,
            255.0,
        );
        assert!(!r.ok);
        assert_eq!(r.reason.as_deref(), Some("low_blur"));
    }
}

#[test]
fn non_finite_rejected() {
    let r = quality_gate(
        f32::NAN,
        (10.0, 10.0, 100.0, 100.0),
        0.5,
        60,
        1920,
        1080,
        false,
    );
    assert!(!r.ok);
    assert_eq!(r.reason.as_deref(), Some("non_finite"));
}
