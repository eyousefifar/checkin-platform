//! Cosine gallery matching (not FAISS).

use crate::embed::l2_normalize;

#[derive(Debug, Clone, PartialEq)]
pub struct MatchResult {
    pub employee_id: Option<i64>,
    pub score: f32,
    pub margin: f32,
    pub label: String,
}

/// Cosine scores: gallery rows × query. Both should be L2-normalized.
pub fn cosine_scores(query: &[f32], gallery: &[Vec<f32>]) -> Vec<f32> {
    let q = l2_normalize(query);
    gallery
        .iter()
        .map(|row| {
            let g = l2_normalize(row);
            if g.len() != q.len() {
                return 0.0;
            }
            g.iter().zip(q.iter()).map(|(a, b)| a * b).sum::<f32>()
        })
        .collect()
}

pub fn match_top1(
    query: &[f32],
    gallery: &[Vec<f32>],
    employee_ids: &[i64],
    names: &[String],
    threshold: f32,
    margin: f32,
) -> MatchResult {
    if gallery.is_empty() || employee_ids.is_empty() {
        return MatchResult {
            employee_id: None,
            score: 0.0,
            margin: 0.0,
            label: "UNKNOWN".into(),
        };
    }
    // Non-finite query or gallery vectors must never select an employee.
    if query.iter().any(|x| !x.is_finite())
        || gallery.iter().any(|row| row.iter().any(|x| !x.is_finite()))
    {
        return MatchResult {
            employee_id: None,
            score: 0.0,
            margin: 0.0,
            label: "UNKNOWN".into(),
        };
    }

    let scores = cosine_scores(query, gallery);
    let mut order: Vec<usize> = (0..scores.len()).collect();
    order.sort_by(|&a, &b| {
        scores[b]
            .partial_cmp(&scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top1 = order[0];
    let top1_score = scores[top1];
    let top2_score = if order.len() > 1 {
        scores[order[1]]
    } else {
        -1.0
    };
    let m = if order.len() > 1 {
        top1_score - top2_score
    } else {
        top1_score
    };

    if top1_score < threshold {
        return MatchResult {
            employee_id: None,
            score: top1_score,
            margin: m,
            label: "UNKNOWN".into(),
        };
    }
    if m < margin && order.len() > 1 {
        return MatchResult {
            employee_id: None,
            score: top1_score,
            margin: m,
            label: "AMBIGUOUS".into(),
        };
    }
    MatchResult {
        employee_id: Some(employee_ids[top1]),
        score: top1_score,
        margin: m,
        label: names[top1].clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::l2_normalize;

    fn ortho_pair() -> (Vec<f32>, Vec<f32>) {
        let mut a = vec![0.0f32; 512];
        a[0] = 1.0;
        let mut b = vec![0.0f32; 512];
        b[1] = 1.0;
        (l2_normalize(&a), l2_normalize(&b))
    }

    #[test]
    fn cosine_identical_is_one() {
        let (a, _) = ortho_pair();
        let scores = cosine_scores(&a, std::slice::from_ref(&a));
        assert!((scores[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn match_accepts_above_threshold_with_margin() {
        let (a, b) = ortho_pair();
        let gallery = vec![a.clone(), b];
        let mut q = a.clone();
        for x in &mut q {
            *x += 0.01;
        }
        let q = l2_normalize(&q);
        let ids = vec![10i64, 20];
        let names = vec!["Alice".into(), "Bob".into()];
        let r = match_top1(&q, &gallery, &ids, &names, 0.4, 0.05);
        assert_eq!(r.employee_id, Some(10));
        assert_eq!(r.label, "Alice");
        assert!(r.score >= 0.4);
        assert!(r.margin >= 0.05);
    }

    #[test]
    fn match_unknown_below_threshold() {
        let (a, b) = ortho_pair();
        let gallery = vec![a, b];
        let mut q = vec![0.0f32; 512];
        q[2] = 1.0;
        let q = l2_normalize(&q);
        let r = match_top1(
            &q,
            &gallery,
            &[10, 20],
            &["Alice".into(), "Bob".into()],
            0.45,
            0.08,
        );
        assert!(r.employee_id.is_none());
        assert_eq!(r.label, "UNKNOWN");
    }

    #[test]
    fn match_ambiguous_low_margin() {
        let (a, _) = ortho_pair();
        let g1 = a.clone();
        let mut g2 = a.clone();
        for x in &mut g2 {
            *x += 0.001;
        }
        let g2 = l2_normalize(&g2);
        let gallery = vec![g1, g2];
        let r = match_top1(&a, &gallery, &[1, 2], &["A".into(), "B".into()], 0.3, 0.5);
        assert_eq!(r.label, "AMBIGUOUS");
        assert!(r.employee_id.is_none());
    }

    #[test]
    fn empty_gallery_unknown() {
        let q = l2_normalize(&vec![1.0; 512]);
        let r = match_top1(&q, &[], &[], &[], 0.45, 0.08);
        assert_eq!(r.label, "UNKNOWN");
    }
}
