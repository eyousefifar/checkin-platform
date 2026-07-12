//! Embedding pack/unpack and mean L2 vector.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EmbedError {
    #[error("expected dim {expected}, got {got}")]
    WrongDim { expected: usize, got: usize },
    #[error("no vectors to average")]
    Empty,
    #[error("blob length {got} not divisible by 4")]
    BadBlob { got: usize },
}

/// L2-normalize `v` (eps 1e-12). Returns a new vec.
pub fn l2_normalize(v: &[f32]) -> Vec<f32> {
    l2_normalize_eps(v, 1e-12)
}

pub fn l2_normalize_eps(v: &[f32], eps: f32) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n < eps {
        return v.to_vec();
    }
    v.iter().map(|x| x / n).collect()
}

/// Pack float32 little-endian C-order bytes. Length must equal `dim`.
pub fn pack_embedding(v: &[f32], dim: usize) -> Result<Vec<u8>, EmbedError> {
    if v.len() != dim {
        return Err(EmbedError::WrongDim {
            expected: dim,
            got: v.len(),
        });
    }
    let mut out = Vec::with_capacity(dim * 4);
    for &x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    Ok(out)
}

/// Unpack LE float32 blob and L2-normalize (defensive).
pub fn unpack_embedding(blob: &[u8], dim: usize) -> Result<Vec<f32>, EmbedError> {
    if blob.len() % 4 != 0 {
        return Err(EmbedError::BadBlob { got: blob.len() });
    }
    let n = blob.len() / 4;
    if n != dim {
        return Err(EmbedError::WrongDim {
            expected: dim,
            got: n,
        });
    }
    let mut v = Vec::with_capacity(dim);
    for i in 0..dim {
        let start = i * 4;
        let bytes: [u8; 4] = blob[start..start + 4].try_into().unwrap();
        v.push(f32::from_le_bytes(bytes));
    }
    Ok(l2_normalize(&v))
}

/// Mean of L2-normalized vectors, then L2-normalize again.
pub fn mean_l2_embedding(vectors: &[Vec<f32>], dim: usize) -> Result<Vec<f32>, EmbedError> {
    if vectors.is_empty() {
        return Err(EmbedError::Empty);
    }
    let mut acc = vec![0.0f32; dim];
    for v in vectors {
        let n = l2_normalize(v);
        if n.len() != dim {
            return Err(EmbedError::WrongDim {
                expected: dim,
                got: n.len(),
            });
        }
        for i in 0..dim {
            acc[i] += n[i];
        }
    }
    let count = vectors.len() as f32;
    for x in &mut acc {
        *x /= count;
    }
    Ok(l2_normalize(&acc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng_vec(seed: u64, dim: usize) -> Vec<f32> {
        // simple LCG for deterministic tests without extra deps
        let mut s = seed;
        let mut v = Vec::with_capacity(dim);
        for _ in 0..dim {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
            let f = ((s >> 33) as f32) / (u32::MAX as f32) - 0.5;
            v.push(f);
        }
        l2_normalize(&v)
    }

    #[test]
    fn pack_unpack_roundtrip() {
        let v = rng_vec(0, 512);
        let blob = pack_embedding(&v, 512).unwrap();
        assert_eq!(blob.len(), 512 * 4);
        let out = unpack_embedding(&blob, 512).unwrap();
        assert_eq!(out.len(), 512);
        for (a, b) in v.iter().zip(out.iter()) {
            assert!((a - b).abs() < 1e-5);
        }
        let norm: f32 = out.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn pack_wrong_dim_raises() {
        let r = pack_embedding(&vec![1.0; 128], 512);
        assert!(matches!(r, Err(EmbedError::WrongDim { .. })));
    }

    #[test]
    fn mean_l2_unit_norm() {
        let vecs: Vec<Vec<f32>> = (0..5).map(|i| rng_vec(i + 1, 512)).collect();
        let m = mean_l2_embedding(&vecs, 512).unwrap();
        assert_eq!(m.len(), 512);
        let norm: f32 = m.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn mean_empty_raises() {
        assert!(matches!(
            mean_l2_embedding(&[], 512),
            Err(EmbedError::Empty)
        ));
    }
}
