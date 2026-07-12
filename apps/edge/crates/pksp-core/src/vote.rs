//! Temporal voting — commit only after multi-frame agreement.

use crate::track::Track;

#[derive(Debug, Clone, Copy)]
pub struct TrackVote {
    pub employee_id: Option<i64>,
    pub score: f32,
    pub ts: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VoteCommit {
    pub employee_id: i64,
    pub avg_score: f32,
    pub hits: usize,
}

pub fn evaluate_vote(
    track: &Track,
    vote_window: usize,
    vote_min_hits: usize,
    min_avg_score: f32,
) -> Option<VoteCommit> {
    if track.history.is_empty() {
        return None;
    }
    let recent: Vec<&TrackVote> = track.history.iter().rev().take(vote_window).collect();
    // reverse to chronological — order doesn't matter for counts
    let mut counts: std::collections::HashMap<i64, Vec<f32>> = std::collections::HashMap::new();
    for v in recent {
        if let Some(eid) = v.employee_id {
            counts.entry(eid).or_default().push(v.score);
        }
    }
    if counts.is_empty() {
        return None;
    }
    let best_id = *counts
        .iter()
        .max_by(|(id_a, sa), (id_b, sb)| {
            let cmp = sa.len().cmp(&sb.len());
            if cmp != std::cmp::Ordering::Equal {
                return cmp;
            }
            let avg_a = sa.iter().sum::<f32>() / sa.len() as f32;
            let avg_b = sb.iter().sum::<f32>() / sb.len() as f32;
            avg_a
                .partial_cmp(&avg_b)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| id_a.cmp(id_b))
        })
        .unwrap()
        .0;
    let scores = &counts[&best_id];
    if scores.len() < vote_min_hits {
        return None;
    }
    let avg = scores.iter().sum::<f32>() / scores.len() as f32;
    if avg < min_avg_score {
        return None;
    }
    Some(VoteCommit {
        employee_id: best_id,
        avg_score: avg,
        hits: scores.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{assign_tracks, Detection, TrackerState};

    #[test]
    fn vote_commits_after_min_hits() {
        let mut state = TrackerState::new();
        let mut tracks = Vec::new();
        for i in 0..5 {
            tracks = assign_tracks(
                &mut state,
                &[Detection {
                    bbox: (0.1 + i as f32 * 0.01, 0.1, 0.3 + i as f32 * 0.01, 0.4),
                    employee_id: Some(7),
                    score: 0.6,
                    label: "X".into(),
                    quality_ok: true,
                    ts: i as f64,
                    state: "tracking".into(),
                }],
                0.3,
                10,
                5,
            );
        }
        let commit = evaluate_vote(&tracks[0], 5, 3, 0.45);
        assert!(commit.is_some());
        let c = commit.unwrap();
        assert_eq!(c.employee_id, 7);
        assert!(c.hits >= 3);
    }

    #[test]
    fn vote_no_commit_insufficient_hits() {
        let mut state = TrackerState::new();
        let tracks = assign_tracks(
            &mut state,
            &[Detection {
                bbox: (0.1, 0.1, 0.3, 0.4),
                employee_id: Some(7),
                score: 0.6,
                label: "X".into(),
                quality_ok: true,
                ts: 1.0,
                state: "tracking".into(),
            }],
            0.3,
            10,
            5,
        );
        let commit = evaluate_vote(&tracks[0], 5, 3, 0.45);
        assert!(commit.is_none());
    }
}
