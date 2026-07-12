//! Lightweight IoU tracker.

use crate::vote::TrackVote;
use std::collections::VecDeque;

pub fn iou(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> f32 {
    let (ax1, ay1, ax2, ay2) = a;
    let (bx1, by1, bx2, by2) = b;
    let ix1 = ax1.max(bx1);
    let iy1 = ay1.max(by1);
    let ix2 = ax2.min(bx2);
    let iy2 = ay2.min(by2);
    let iw = (ix2 - ix1).max(0.0);
    let ih = (iy2 - iy1).max(0.0);
    let inter = iw * ih;
    if inter <= 0.0 {
        return 0.0;
    }
    let area_a = (ax2 - ax1).max(0.0) * (ay2 - ay1).max(0.0);
    let area_b = (bx2 - bx1).max(0.0) * (by2 - by1).max(0.0);
    let union = area_a + area_b - inter;
    if union > 0.0 {
        inter / union
    } else {
        0.0
    }
}

#[derive(Debug, Clone)]
pub struct Detection {
    pub bbox: (f32, f32, f32, f32),
    pub employee_id: Option<i64>,
    pub score: f32,
    pub label: String,
    pub quality_ok: bool,
    pub ts: f64,
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub track_id: i64,
    pub bbox: (f32, f32, f32, f32),
    pub age: i32,
    pub hits: i32,
    pub history: VecDeque<TrackVote>,
    pub last_commit_ts: Option<f64>,
    pub label: String,
    pub employee_id: Option<i64>,
    pub score: f32,
    pub quality_ok: bool,
    pub state: String,
    /// Centroid history for trajectory (normalized coords).
    pub centroids: VecDeque<(f32, f32)>,
}

#[derive(Debug, Default)]
pub struct TrackerState {
    pub tracks: Vec<Track>,
    pub next_id: i64,
}

impl TrackerState {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            next_id: 1,
        }
    }
}

pub fn assign_tracks(
    state: &mut TrackerState,
    detections: &[Detection],
    iou_threshold: f32,
    max_age: i32,
    vote_window: usize,
) -> Vec<Track> {
    if state.next_id < 1 {
        state.next_id = 1;
    }
    for t in &mut state.tracks {
        t.age += 1;
    }

    let mut unmatched_dets: Vec<usize> = (0..detections.len()).collect();
    let mut unmatched_tracks: Vec<usize> = (0..state.tracks.len()).collect();
    let mut pairs: Vec<(usize, usize, f32)> = Vec::new();

    for (ti, tr) in state.tracks.iter().enumerate() {
        for (di, det) in detections.iter().enumerate() {
            let score = iou(tr.bbox, det.bbox);
            if score >= iou_threshold {
                pairs.push((ti, di, score));
            }
        }
    }
    pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let mut used_t = std::collections::HashSet::new();
    let mut used_d = std::collections::HashSet::new();
    for (ti, di, _) in pairs {
        if used_t.contains(&ti) || used_d.contains(&di) {
            continue;
        }
        used_t.insert(ti);
        used_d.insert(di);
        unmatched_tracks.retain(|&x| x != ti);
        unmatched_dets.retain(|&x| x != di);

        let det = &detections[di];
        let tr = &mut state.tracks[ti];
        tr.bbox = det.bbox;
        tr.age = 0;
        tr.hits += 1;
        tr.employee_id = det.employee_id;
        tr.score = det.score;
        tr.label = if det.employee_id.is_none() && det.label.is_empty() {
            "UNKNOWN".into()
        } else {
            det.label.clone()
        };
        tr.quality_ok = det.quality_ok;
        tr.state = det.state.clone();
        let cx = (det.bbox.0 + det.bbox.2) / 2.0;
        let cy = (det.bbox.1 + det.bbox.3) / 2.0;
        tr.centroids.push_back((cx, cy));
        while tr.centroids.len() > 16 {
            tr.centroids.pop_front();
        }
        if tr.quality_ok {
            tr.history.push_back(TrackVote {
                employee_id: tr.employee_id,
                score: tr.score,
                ts: det.ts,
            });
            let cap = (vote_window * 2).max(8);
            while tr.history.len() > cap {
                tr.history.pop_front();
            }
        }
    }

    for di in unmatched_dets {
        let det = &detections[di];
        let mut tr = Track {
            track_id: state.next_id,
            bbox: det.bbox,
            age: 0,
            hits: 1,
            history: VecDeque::with_capacity(16),
            last_commit_ts: None,
            label: if det.label.is_empty() {
                "UNKNOWN".into()
            } else {
                det.label.clone()
            },
            employee_id: det.employee_id,
            score: det.score,
            quality_ok: det.quality_ok,
            state: det.state.clone(),
            centroids: VecDeque::new(),
        };
        let cx = (det.bbox.0 + det.bbox.2) / 2.0;
        let cy = (det.bbox.1 + det.bbox.3) / 2.0;
        tr.centroids.push_back((cx, cy));
        if tr.quality_ok {
            tr.history.push_back(TrackVote {
                employee_id: tr.employee_id,
                score: tr.score,
                ts: det.ts,
            });
        }
        state.next_id += 1;
        state.tracks.push(tr);
    }

    state.tracks.retain(|t| t.age <= max_age);
    state.tracks.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iou_identical() {
        let box_ = (0.1, 0.1, 0.5, 0.5);
        assert!((iou(box_, box_) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn iou_disjoint() {
        assert_eq!(iou((0.0, 0.0, 0.1, 0.1), (0.5, 0.5, 0.6, 0.6)), 0.0);
    }

    #[test]
    fn assign_tracks_reuses_id() {
        let mut state = TrackerState::new();
        let t1 = assign_tracks(
            &mut state,
            &[Detection {
                bbox: (0.1, 0.1, 0.3, 0.4),
                employee_id: Some(1),
                score: 0.7,
                label: "A".into(),
                quality_ok: true,
                ts: 1.0,
                state: "tracking".into(),
            }],
            0.3,
            10,
            5,
        );
        assert_eq!(t1.len(), 1);
        let tid = t1[0].track_id;
        let t2 = assign_tracks(
            &mut state,
            &[Detection {
                bbox: (0.12, 0.11, 0.32, 0.41),
                employee_id: Some(1),
                score: 0.72,
                label: "A".into(),
                quality_ok: true,
                ts: 1.1,
                state: "tracking".into(),
            }],
            0.3,
            10,
            5,
        );
        assert_eq!(t2.len(), 1);
        assert_eq!(t2[0].track_id, tid);
        assert!(t2[0].history.len() >= 2);
    }
}
