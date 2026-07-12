//! SCRFD decoder for InsightFace buffalo_l `det_10g.onnx`.
//!
//! # Observed ORT contract (det_10g.onnx)
//! - Input: `input.1`, NCHW `1×3×S×S` (S = configured det size, typically 640)
//! - Outputs (order, shape for S=640, num_anchors=2, strides 8/16/32):
//!   - scores: `[12800,1]`, `[3200,1]`, `[800,1]`  (H×W×2)
//!   - bboxes: `[12800,4]`, `[3200,4]`, `[800,4]`  (l,t,r,b distances)
//!   - kps:    `[12800,10]`, `[3200,10]`, `[800,10]` (5×(dx,dy))
//! - Names are numeric export ids (`448`…); match by **shape**, not name.
//! - Preprocess: aspect-preserving bilinear resize, **top-left** zero pad,
//!   BGR→RGB, `(x-127.5)/128`.

use thiserror::Error;

pub const STRIDES: [i32; 3] = [8, 16, 32];
pub const NUM_ANCHORS: usize = 2;
pub const NMS_IOU: f32 = 0.4;
pub const DEFAULT_SCORE_THRESH: f32 = 0.5;

#[derive(Debug, Error, PartialEq)]
pub enum ScrfdError {
    #[error("missing SCRFD head: {0}")]
    MissingHead(&'static str),
    #[error("wrong-shaped SCRFD head: {0}")]
    WrongShape(String),
    #[error("non-finite SCRFD tensor value")]
    NonFinite,
    #[error("ambiguous SCRFD heads for stride group")]
    AmbiguousHeads,
    #[error("degenerate SCRFD detection")]
    Degenerate,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LetterboxMeta {
    pub scale: f32,
    pub pad_x: f32,
    pub pad_y: f32,
    pub det_size: u32,
    pub orig_w: u32,
    pub orig_h: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScrfdDetection {
    pub bbox: (f32, f32, f32, f32),
    pub score: f32,
    pub landmarks: [[f32; 2]; 5],
}

/// Bilinear aspect-preserving resize into top-left of a zero square; BGR→RGB NCHW.
pub fn letterbox_bgr_to_nchw(
    bgr: &[u8],
    w: u32,
    h: u32,
    det_size: u32,
) -> Result<(Vec<f32>, LetterboxMeta), ScrfdError> {
    if w == 0 || h == 0 || det_size == 0 {
        return Err(ScrfdError::Degenerate);
    }
    if bgr.len() < (w * h * 3) as usize {
        return Err(ScrfdError::Degenerate);
    }
    let scale = (det_size as f32 / w as f32).min(det_size as f32 / h as f32);
    let nw = (w as f32 * scale).round().max(1.0) as u32;
    let nh = (h as f32 * scale).round().max(1.0) as u32;
    let nw = nw.min(det_size);
    let nh = nh.min(det_size);
    // Top-left pad (zeros fill the rest)
    let pad_x = 0.0f32;
    let pad_y = 0.0f32;
    let plane = (det_size * det_size) as usize;
    let mut out = vec![0.0f32; 3 * plane];

    for y in 0..nh {
        for x in 0..nw {
            let src_x = (x as f32 + 0.5) / scale - 0.5;
            let src_y = (y as f32 + 0.5) / scale - 0.5;
            let (r, g, b) = sample_bgr_bilinear(bgr, w, h, src_x, src_y);
            let di = (y * det_size + x) as usize;
            out[di] = (r - 127.5) / 128.0;
            out[plane + di] = (g - 127.5) / 128.0;
            out[2 * plane + di] = (b - 127.5) / 128.0;
        }
    }

    let meta = LetterboxMeta {
        scale,
        pad_x,
        pad_y,
        det_size,
        orig_w: w,
        orig_h: h,
    };
    Ok((out, meta))
}

fn sample_bgr_bilinear(bgr: &[u8], w: u32, h: u32, x: f32, y: f32) -> (f32, f32, f32) {
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let dx = x - x0 as f32;
    let dy = y - y0 as f32;
    let pix = |xx: u32, yy: u32| {
        let i = ((yy * w + xx) * 3) as usize;
        (bgr[i] as f32, bgr[i + 1] as f32, bgr[i + 2] as f32)
    };
    let (b00, g00, r00) = pix(x0, y0);
    let (b10, g10, r10) = pix(x1, y0);
    let (b01, g01, r01) = pix(x0, y1);
    let (b11, g11, r11) = pix(x1, y1);
    let lerp = |a, b, t| a + (b - a) * t;
    let b0 = lerp(b00, b10, dx);
    let b1 = lerp(b01, b11, dx);
    let g0 = lerp(g00, g10, dx);
    let g1 = lerp(g01, g11, dx);
    let r0 = lerp(r00, r10, dx);
    let r1 = lerp(r01, r11, dx);
    (lerp(r0, r1, dy), lerp(g0, g1, dy), lerp(b0, b1, dy))
}

/// One FPN level tensors (already flattened to [N, C]).
#[derive(Debug, Clone)]
pub struct ScrfdLevel<'a> {
    pub stride: i32,
    pub scores: &'a [f32], // len = H*W*num_anchors
    pub bboxes: &'a [f32], // len = N*4
    pub kps: &'a [f32],    // len = N*10
}

fn ensure_finite(data: &[f32]) -> Result<(), ScrfdError> {
    if data.iter().any(|v| !v.is_finite()) {
        Err(ScrfdError::NonFinite)
    } else {
        Ok(())
    }
}

/// Expected number of anchors locations for det_size and stride.
pub fn expected_locations(det_size: u32, stride: i32) -> usize {
    let side = (det_size as i32 / stride) as usize;
    side * side * NUM_ANCHORS
}

/// Classify a flat score tensor length into stride for a given det_size.
pub fn stride_for_score_len(det_size: u32, len: usize) -> Option<i32> {
    STRIDES
        .iter()
        .find(|&&s| expected_locations(det_size, s) == len)
        .copied()
}

/// Decode all levels → original image coordinates, then NMS.
pub fn decode_scrfd(
    levels: &[ScrfdLevel<'_>],
    meta: &LetterboxMeta,
    score_thresh: f32,
    nms_iou: f32,
) -> Result<Vec<ScrfdDetection>, ScrfdError> {
    if levels.is_empty() {
        return Err(ScrfdError::MissingHead("no levels"));
    }
    let mut dets = Vec::new();
    for level in levels {
        ensure_finite(level.scores)?;
        ensure_finite(level.bboxes)?;
        ensure_finite(level.kps)?;
        let n = expected_locations(meta.det_size, level.stride);
        if level.scores.len() != n {
            return Err(ScrfdError::WrongShape(format!(
                "scores len {} want {}",
                level.scores.len(),
                n
            )));
        }
        if level.bboxes.len() != n * 4 {
            return Err(ScrfdError::WrongShape(format!(
                "bboxes len {} want {}",
                level.bboxes.len(),
                n * 4
            )));
        }
        if level.kps.len() != n * 10 {
            return Err(ScrfdError::WrongShape(format!(
                "kps len {} want {}",
                level.kps.len(),
                n * 10
            )));
        }
        let side = (meta.det_size as i32 / level.stride) as usize;
        let stride = level.stride as f32;
        for y in 0..side {
            for x in 0..side {
                for a in 0..NUM_ANCHORS {
                    let idx = (y * side + x) * NUM_ANCHORS + a;
                    let score = level.scores[idx];
                    if score < score_thresh {
                        continue;
                    }
                    let cx = x as f32 * stride;
                    let cy = y as f32 * stride;
                    let base = idx * 4;
                    let l = level.bboxes[base] * stride;
                    let t = level.bboxes[base + 1] * stride;
                    let r = level.bboxes[base + 2] * stride;
                    let b = level.bboxes[base + 3] * stride;
                    let x1 = cx - l;
                    let y1 = cy - t;
                    let x2 = cx + r;
                    let y2 = cy + b;
                    let mut kps = [[0.0f32; 2]; 5];
                    let kb = idx * 10;
                    for (k, kp) in kps.iter_mut().enumerate() {
                        kp[0] = cx + level.kps[kb + k * 2] * stride;
                        kp[1] = cy + level.kps[kb + k * 2 + 1] * stride;
                    }
                    // Inverse letterbox → original pixels
                    let inv = |px: f32, py: f32| -> (f32, f32) {
                        let ox = ((px - meta.pad_x) / meta.scale).clamp(0.0, meta.orig_w as f32);
                        let oy = ((py - meta.pad_y) / meta.scale).clamp(0.0, meta.orig_h as f32);
                        (ox, oy)
                    };
                    let (bx1, by1) = inv(x1, y1);
                    let (bx2, by2) = inv(x2, y2);
                    if bx2 - bx1 < 1.0 || by2 - by1 < 1.0 {
                        continue;
                    }
                    if !bx1.is_finite() || !by1.is_finite() || !bx2.is_finite() || !by2.is_finite()
                    {
                        return Err(ScrfdError::NonFinite);
                    }
                    let mut lm = [[0.0f32; 2]; 5];
                    for k in 0..5 {
                        let (lx, ly) = inv(kps[k][0], kps[k][1]);
                        if !lx.is_finite() || !ly.is_finite() {
                            return Err(ScrfdError::NonFinite);
                        }
                        lm[k] = [lx, ly];
                    }
                    dets.push(ScrfdDetection {
                        bbox: (bx1, by1, bx2, by2),
                        score,
                        landmarks: lm,
                    });
                }
            }
        }
    }
    Ok(nms(dets, nms_iou))
}

fn iou(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> f32 {
    let xx1 = a.0.max(b.0);
    let yy1 = a.1.max(b.1);
    let xx2 = a.2.min(b.2);
    let yy2 = a.3.min(b.3);
    let w = (xx2 - xx1).max(0.0);
    let h = (yy2 - yy1).max(0.0);
    let inter = w * h;
    let area_a = (a.2 - a.0).max(0.0) * (a.3 - a.1).max(0.0);
    let area_b = (b.2 - b.0).max(0.0) * (b.3 - b.1).max(0.0);
    let uni = area_a + area_b - inter;
    if uni <= 0.0 {
        0.0
    } else {
        inter / uni
    }
}

/// Descending-score IoU NMS.
pub fn nms(mut dets: Vec<ScrfdDetection>, iou_thresh: f32) -> Vec<ScrfdDetection> {
    dets.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut keep = Vec::new();
    let mut suppressed = vec![false; dets.len()];
    for i in 0..dets.len() {
        if suppressed[i] {
            continue;
        }
        keep.push(dets[i].clone());
        for j in (i + 1)..dets.len() {
            if suppressed[j] {
                continue;
            }
            if iou(dets[i].bbox, dets[j].bbox) > iou_thresh {
                suppressed[j] = true;
            }
        }
    }
    keep
}

/// Group nine flat heads (scores×3, bboxes×3, kps×3) by length into levels.
pub fn levels_from_heads<'a>(
    scores: &[&'a [f32]],
    bboxes: &[&'a [f32]],
    kps: &[&'a [f32]],
    det_size: u32,
) -> Result<Vec<ScrfdLevel<'a>>, ScrfdError> {
    if scores.len() != 3 || bboxes.len() != 3 || kps.len() != 3 {
        return Err(ScrfdError::MissingHead("need 3 score/bbox/kps heads"));
    }
    let mut levels = Vec::with_capacity(3);
    for &s in &STRIDES {
        let n = expected_locations(det_size, s);
        let score = scores
            .iter()
            .find(|t| t.len() == n)
            .ok_or(ScrfdError::MissingHead("score"))?;
        let bbox = bboxes
            .iter()
            .find(|t| t.len() == n * 4)
            .ok_or(ScrfdError::MissingHead("bbox"))?;
        let kp = kps
            .iter()
            .find(|t| t.len() == n * 10)
            .ok_or(ScrfdError::MissingHead("kps"))?;
        // Ambiguity: more than one head with same length
        if scores.iter().filter(|t| t.len() == n).count() != 1
            || bboxes.iter().filter(|t| t.len() == n * 4).count() != 1
            || kps.iter().filter(|t| t.len() == n * 10).count() != 1
        {
            return Err(ScrfdError::AmbiguousHeads);
        }
        levels.push(ScrfdLevel {
            stride: s,
            scores: score,
            bboxes: bbox,
            kps: kp,
        });
    }
    Ok(levels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta_sq(size: u32) -> LetterboxMeta {
        LetterboxMeta {
            scale: 1.0,
            pad_x: 0.0,
            pad_y: 0.0,
            det_size: size,
            orig_w: size,
            orig_h: size,
        }
    }

    #[test]
    fn expected_locations_640() {
        assert_eq!(expected_locations(640, 8), 12800);
        assert_eq!(expected_locations(640, 16), 3200);
        assert_eq!(expected_locations(640, 32), 800);
    }

    #[test]
    fn letterbox_top_left_and_nonzero() {
        // 2x2 BGR solid colors
        let mut bgr = vec![0u8; 2 * 2 * 3];
        bgr[0] = 0;
        bgr[1] = 0;
        bgr[2] = 255; // red at (0,0)
        let (tensor, meta) = letterbox_bgr_to_nchw(&bgr, 2, 2, 4).unwrap();
        assert!(meta.scale > 0.0);
        assert_eq!(meta.pad_x, 0.0);
        assert_eq!(meta.pad_y, 0.0);
        assert_eq!(tensor.len(), 3 * 16);
        // top-left cell should be ~ red channel high in plane 0
        assert!(tensor[0] > 0.5, "R channel at (0,0) = {}", tensor[0]);
    }

    #[test]
    fn zero_faces_when_scores_low() {
        let n8 = expected_locations(64, 8);
        let scores = vec![0.01f32; n8];
        let bboxes = vec![0.1f32; n8 * 4];
        let kps = vec![0.0f32; n8 * 10];
        // only one level for tiny test — inject via decode_scrfd single level
        let level = ScrfdLevel {
            stride: 8,
            scores: &scores,
            bboxes: &bboxes,
            kps: &kps,
        };
        let dets = decode_scrfd(&[level], &meta_sq(64), 0.5, 0.4).unwrap();
        assert!(dets.is_empty());
    }

    #[test]
    fn single_high_score_decodes_box_and_kps() {
        let n = expected_locations(64, 8);
        let mut scores = vec![0.0f32; n];
        let mut bboxes = vec![0.0f32; n * 4];
        let mut kps = vec![0.0f32; n * 10];
        // place at (x=2,y=2,a=0) → idx = (2*8+2)*2+0 = 36
        let idx = (2 * 8 + 2) * 2;
        scores[idx] = 0.95;
        // distances in stride units → after *stride become pixels
        bboxes[idx * 4] = 1.0;
        bboxes[idx * 4 + 1] = 1.0;
        bboxes[idx * 4 + 2] = 1.0;
        bboxes[idx * 4 + 3] = 1.0;
        for k in 0..5 {
            kps[idx * 10 + k * 2] = 0.1 * k as f32;
            kps[idx * 10 + k * 2 + 1] = 0.2;
        }
        let level = ScrfdLevel {
            stride: 8,
            scores: &scores,
            bboxes: &bboxes,
            kps: &kps,
        };
        let dets = decode_scrfd(&[level], &meta_sq(64), 0.5, 0.4).unwrap();
        assert_eq!(dets.len(), 1);
        let d = &dets[0];
        assert!((d.score - 0.95).abs() < 1e-5);
        assert!(d.bbox.2 > d.bbox.0 && d.bbox.3 > d.bbox.1);
        assert!(d
            .landmarks
            .iter()
            .all(|p| p[0].is_finite() && p[1].is_finite()));
    }

    #[test]
    fn nms_suppresses_overlap() {
        let a = ScrfdDetection {
            bbox: (0.0, 0.0, 10.0, 10.0),
            score: 0.9,
            landmarks: [[0.0; 2]; 5],
        };
        let b = ScrfdDetection {
            bbox: (1.0, 1.0, 11.0, 11.0),
            score: 0.8,
            landmarks: [[0.0; 2]; 5],
        };
        let kept = nms(vec![a, b], 0.4);
        assert_eq!(kept.len(), 1);
        assert!((kept[0].score - 0.9).abs() < 1e-5);
    }

    #[test]
    fn rejects_nan_scores() {
        let n = expected_locations(64, 8);
        let scores = vec![f32::NAN; n];
        let bboxes = vec![0.1f32; n * 4];
        let kps = vec![0.0f32; n * 10];
        let level = ScrfdLevel {
            stride: 8,
            scores: &scores,
            bboxes: &bboxes,
            kps: &kps,
        };
        let err = decode_scrfd(&[level], &meta_sq(64), 0.5, 0.4).unwrap_err();
        assert_eq!(err, ScrfdError::NonFinite);
    }

    #[test]
    fn levels_from_heads_matches_shape() {
        let s8 = vec![0.0f32; 12800];
        let s16 = vec![0.0f32; 3200];
        let s32 = vec![0.0f32; 800];
        let b8 = vec![0.0f32; 12800 * 4];
        let b16 = vec![0.0f32; 3200 * 4];
        let b32 = vec![0.0f32; 800 * 4];
        let k8 = vec![0.0f32; 12800 * 10];
        let k16 = vec![0.0f32; 3200 * 10];
        let k32 = vec![0.0f32; 800 * 10];
        let levels = levels_from_heads(
            &[&s8, &s16, &s32],
            &[&b8, &b16, &b32],
            &[&k8, &k16, &k32],
            640,
        )
        .unwrap();
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0].stride, 8);
    }

    #[test]
    fn levels_missing_head_errors() {
        let s8 = vec![0.0f32; 12800];
        let err = levels_from_heads(&[&s8, &s8, &s8], &[&s8], &[&s8], 640).unwrap_err();
        assert!(matches!(err, ScrfdError::MissingHead(_)));
    }

    #[test]
    fn non_square_letterbox_preserves_aspect() {
        let bgr = vec![128u8; 100 * 50 * 3];
        let (_, meta) = letterbox_bgr_to_nchw(&bgr, 100, 50, 64).unwrap();
        assert!((meta.scale - 64.0 / 100.0).abs() < 1e-4);
        assert_eq!(meta.orig_w, 100);
        assert_eq!(meta.orig_h, 50);
    }
}
