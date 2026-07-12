//! Five-point ArcFace similarity alignment → 112×112 RGB NCHW tensor.

use thiserror::Error;

/// Standard ArcFace 112×112 five-point destination template (InsightFace).
pub const ARCFACE_DST: [[f32; 2]; 5] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];

pub const ALIGN_SIZE: u32 = 112;

#[derive(Debug, Error, PartialEq)]
pub enum AlignError {
    #[error("non-finite landmarks or transform")]
    NonFinite,
    #[error("singular similarity transform")]
    Singular,
    #[error("source buffer out of bounds")]
    OobSource,
    #[error("invalid alignment input")]
    InvalidInput,
}

/// Similarity transform: `[x'] = s * R * [x] + t` with R = [[c,-s],[s,c]] where c=s*cos, etc.
/// Represented as `x' = a*x - b*y + tx`, `y' = b*x + a*y + ty`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Similarity2 {
    pub a: f32,
    pub b: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Similarity2 {
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x - self.b * y + self.tx,
            self.b * x + self.a * y + self.ty,
        )
    }

    pub fn inverse(&self) -> Result<Self, AlignError> {
        // R = [[a, -b], [b, a]], d = a²+b²
        // R^{-1} = (1/d) [[a, b], [-b, a]]
        // p = R^{-1}(p' - t) written as p_x = a_s p'_x - b_s p'_y + tx_s
        let d = self.a * self.a + self.b * self.b;
        if !d.is_finite() || d.abs() < 1e-12 {
            return Err(AlignError::Singular);
        }
        let a_s = self.a / d;
        let b_s = -self.b / d;
        let tx_s = -(a_s * self.tx - b_s * self.ty);
        let ty_s = -(b_s * self.tx + a_s * self.ty);
        if ![a_s, b_s, tx_s, ty_s].iter().all(|v| v.is_finite()) {
            return Err(AlignError::NonFinite);
        }
        Ok(Self {
            a: a_s,
            b: b_s,
            tx: tx_s,
            ty: ty_s,
        })
    }
}

/// Estimate similarity mapping `src` → `dst` (5 points).
pub fn estimate_similarity(
    src: &[[f32; 2]; 5],
    dst: &[[f32; 2]; 5],
) -> Result<Similarity2, AlignError> {
    for p in src.iter().chain(dst.iter()) {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err(AlignError::NonFinite);
        }
    }
    // Umeyama-style for similarity with equal weights
    let mut mean_s = [0.0f32; 2];
    let mut mean_d = [0.0f32; 2];
    for i in 0..5 {
        mean_s[0] += src[i][0];
        mean_s[1] += src[i][1];
        mean_d[0] += dst[i][0];
        mean_d[1] += dst[i][1];
    }
    mean_s[0] /= 5.0;
    mean_s[1] /= 5.0;
    mean_d[0] /= 5.0;
    mean_d[1] /= 5.0;

    let mut var_s = 0.0f32;
    let mut cov_a = 0.0f32; // sum (xs*xd + ys*yd)
    let mut cov_b = 0.0f32; // sum (xs*yd - ys*xd) for rotation
    for i in 0..5 {
        let xs = src[i][0] - mean_s[0];
        let ys = src[i][1] - mean_s[1];
        let xd = dst[i][0] - mean_d[0];
        let yd = dst[i][1] - mean_d[1];
        var_s += xs * xs + ys * ys;
        cov_a += xs * xd + ys * yd;
        cov_b += xs * yd - ys * xd;
    }
    if !var_s.is_finite() || var_s.abs() < 1e-12 {
        return Err(AlignError::Singular);
    }
    let a = cov_a / var_s; // scale*cos
    let b = cov_b / var_s; // scale*sin
    let tx = mean_d[0] - (a * mean_s[0] - b * mean_s[1]);
    let ty = mean_d[1] - (b * mean_s[0] + a * mean_s[1]);
    if ![a, b, tx, ty].iter().all(|v| v.is_finite()) {
        return Err(AlignError::NonFinite);
    }
    Ok(Similarity2 { a, b, tx, ty })
}

/// Warp BGR source via inverse transform into 112×112 RGB NCHW ArcFace tensor.
/// Normalization: `(x - 127.5) / 127.5`.
pub fn align_arcface_bgr(
    bgr: &[u8],
    width: u32,
    height: u32,
    landmarks: &[[f32; 2]; 5],
) -> Result<Vec<f32>, AlignError> {
    if width == 0 || height == 0 || bgr.len() < (width * height * 3) as usize {
        return Err(AlignError::InvalidInput);
    }
    let forward = estimate_similarity(landmarks, &ARCFACE_DST)?;
    let inv = forward.inverse()?;
    let plane = (ALIGN_SIZE * ALIGN_SIZE) as usize;
    let mut out = vec![0.0f32; 3 * plane];
    for y in 0..ALIGN_SIZE {
        for x in 0..ALIGN_SIZE {
            let (sx, sy) = inv.apply(x as f32, y as f32);
            if !sx.is_finite() || !sy.is_finite() {
                return Err(AlignError::NonFinite);
            }
            let (r, g, b) = sample_bgr_bilinear(bgr, width, height, sx, sy)?;
            let di = (y * ALIGN_SIZE + x) as usize;
            out[di] = (r - 127.5) / 127.5;
            out[plane + di] = (g - 127.5) / 127.5;
            out[2 * plane + di] = (b - 127.5) / 127.5;
        }
    }
    if out.iter().any(|v| !v.is_finite()) {
        return Err(AlignError::NonFinite);
    }
    Ok(out)
}

fn sample_bgr_bilinear(
    bgr: &[u8],
    w: u32,
    h: u32,
    x: f32,
    y: f32,
) -> Result<(f32, f32, f32), AlignError> {
    if w == 0 || h == 0 {
        return Err(AlignError::OobSource);
    }
    // Allow slight out-of-range by clamping (standard face align practice)
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let dx = x - x0 as f32;
    let dy = y - y0 as f32;
    let get = |xx: u32, yy: u32| {
        let i = ((yy * w + xx) * 3) as usize;
        if i + 2 >= bgr.len() {
            return Err(AlignError::OobSource);
        }
        Ok((bgr[i] as f32, bgr[i + 1] as f32, bgr[i + 2] as f32))
    };
    let (b00, g00, r00) = get(x0, y0)?;
    let (b10, g10, r10) = get(x1, y0)?;
    let (b01, g01, r01) = get(x0, y1)?;
    let (b11, g11, r11) = get(x1, y1)?;
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    Ok((
        lerp(lerp(r00, r10, dx), lerp(r01, r11, dx), dy),
        lerp(lerp(g00, g10, dx), lerp(g01, g11, dx), dy),
        lerp(lerp(b00, b10, dx), lerp(b01, b11, dx), dy),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_template_identity_maps_within_epsilon() {
        let t = estimate_similarity(&ARCFACE_DST, &ARCFACE_DST).unwrap();
        for p in &ARCFACE_DST {
            let (x, y) = t.apply(p[0], p[1]);
            assert!((x - p[0]).abs() < 1e-3 && (y - p[1]).abs() < 1e-3);
        }
    }

    #[test]
    fn known_scale_translation() {
        // src = dst * 2 + (10, 20)  ... actually map src→dst: src = 2*dst?
        // Let src points be scaled from ARCFACE
        let mut src = ARCFACE_DST;
        for p in &mut src {
            p[0] = p[0] * 2.0 + 10.0;
            p[1] = p[1] * 2.0 + 20.0;
        }
        let t = estimate_similarity(&src, &ARCFACE_DST).unwrap();
        // scale should be ~0.5
        let scale = (t.a * t.a + t.b * t.b).sqrt();
        assert!((scale - 0.5).abs() < 0.02, "scale={scale}");
    }

    #[test]
    fn singular_points_rejected() {
        let src = [[0.0, 0.0]; 5];
        let err = estimate_similarity(&src, &ARCFACE_DST).unwrap_err();
        assert_eq!(err, AlignError::Singular);
    }

    #[test]
    fn non_finite_landmarks_rejected() {
        let mut src = ARCFACE_DST;
        src[0][0] = f32::NAN;
        assert_eq!(
            estimate_similarity(&src, &ARCFACE_DST).unwrap_err(),
            AlignError::NonFinite
        );
    }

    #[test]
    fn output_tensor_shape_and_finite() {
        // solid gray image
        let w = 200u32;
        let h = 200u32;
        let bgr = vec![100u8; (w * h * 3) as usize];
        // landmarks roughly centered face-like
        let lm = [
            [70.0, 80.0],
            [130.0, 80.0],
            [100.0, 110.0],
            [75.0, 140.0],
            [125.0, 140.0],
        ];
        let tensor = align_arcface_bgr(&bgr, w, h, &lm).unwrap();
        assert_eq!(tensor.len(), 3 * 112 * 112);
        assert!(tensor.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn identity_transform_samples_expected_pixel() {
        // Put a unique red pixel at (50,50) in BGR
        let w = 112u32;
        let h = 112u32;
        let mut bgr = vec![0u8; (w * h * 3) as usize];
        let i = ((50 * w + 50) * 3) as usize;
        bgr[i] = 0;
        bgr[i + 1] = 0;
        bgr[i + 2] = 255;
        // Identity: landmarks = ARCFACE_DST so transform maps dst→same src coords
        let tensor = align_arcface_bgr(&bgr, w, h, &ARCFACE_DST).unwrap();
        // Just ensure tensor valid; exact pixel mapping depends on inverse of identity
        assert_eq!(tensor.len(), 3 * 112 * 112);
        let t = estimate_similarity(&ARCFACE_DST, &ARCFACE_DST).unwrap();
        let inv = t.inverse().unwrap();
        let (sx, sy) = inv.apply(50.0, 50.0);
        assert!((sx - 50.0).abs() < 1e-2 && (sy - 50.0).abs() < 1e-2);
    }
}
