//! Smart scene: zones + trajectory eligibility (pure).

use crate::fsm::Direction;
use crate::track::Track;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneKind {
    Ignore,
    Approach,
    Active,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Zone {
    pub id: String,
    pub kind: ZoneKind,
    /// Normalized polygon vertices (x,y) in 0–1.
    pub polygon: Vec<(f32, f32)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ZoneMap {
    pub zones: Vec<Zone>,
}

/// Ray-casting point-in-polygon.
pub fn point_in_polygon(x: f32, y: f32, polygon: &[(f32, f32)]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let n = polygon.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = polygon[i];
        let (xj, yj) = polygon[j];
        let intersect = ((yi > y) != (yj > y))
            && (x < (xj - xi) * (y - yi) / (yj - yi + 1e-12) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Centroid of normalized bbox.
pub fn bbox_centroid(bbox: (f32, f32, f32, f32)) -> (f32, f32) {
    ((bbox.0 + bbox.2) / 2.0, (bbox.1 + bbox.3) / 2.0)
}

/// Prefer Active over Approach over Ignore when multiple contain the point.
pub fn track_zone(bbox: (f32, f32, f32, f32), zones: &[Zone]) -> Option<ZoneKind> {
    let (cx, cy) = bbox_centroid(bbox);
    let mut best: Option<ZoneKind> = None;
    for z in zones {
        if point_in_polygon(cx, cy, &z.polygon) {
            best = match (best, z.kind) {
                (None, k) => Some(k),
                (Some(ZoneKind::Active), _) => Some(ZoneKind::Active),
                (Some(ZoneKind::Approach), ZoneKind::Active) => Some(ZoneKind::Active),
                (Some(ZoneKind::Approach), _) => Some(ZoneKind::Approach),
                (Some(ZoneKind::Ignore), k) => Some(k),
            };
        }
    }
    best
}

/// Ignore zone must not vote toward attendance. Approach/Active may vote.
/// When smart scene is off or no zones, always allow vote.
pub fn should_vote(zone: Option<ZoneKind>, smart_scene_enabled: bool, zones_empty: bool) -> bool {
    if !smart_scene_enabled || zones_empty {
        return true;
    }
    match zone {
        Some(ZoneKind::Ignore) => false,
        Some(ZoneKind::Approach) | Some(ZoneKind::Active) | None => true,
    }
}

/// Walk-by: high lateral motion, never dwelled long in active zone.
/// Heuristic: if net |Δx| > |Δy| * 1.5 over history and few frames in active → walkby.
pub fn trajectory_is_walkby(track: &Track, zones: &[Zone], min_dwell_frames: usize) -> bool {
    if track.centroids.len() < 3 {
        return false;
    }
    let first = track.centroids.front().copied().unwrap();
    let last = track.centroids.back().copied().unwrap();
    let dx = (last.0 - first.0).abs();
    let dy = (last.1 - first.1).abs();

    let active_frames = track
        .centroids
        .iter()
        .filter(|&&(x, y)| {
            zones.iter().any(|z| {
                z.kind == ZoneKind::Active && point_in_polygon(x, y, &z.polygon)
            })
        })
        .count();

    if active_frames >= min_dwell_frames {
        return false;
    }
    // Lateral cut across FOV without lingering in active
    dx > dy * 1.5 && dx > 0.15
}

/// Commit eligible when smart scene on: vote ok AND currently in Active (or no zones configured).
pub fn commit_eligible(
    track: &Track,
    zones: &[Zone],
    smart_scene_enabled: bool,
    min_dwell_frames: usize,
) -> bool {
    if !smart_scene_enabled || zones.is_empty() {
        return true;
    }
    if trajectory_is_walkby(track, zones, min_dwell_frames) {
        return false;
    }
    match track_zone(track.bbox, zones) {
        Some(ZoneKind::Active) => true,
        Some(ZoneKind::Approach) | Some(ZoneKind::Ignore) | None => false,
    }
}

/// Soft motion hint from net vertical displacement (normalized FOV).
/// Positive Δy (moving down in image) → often "approaching door" when cam above door;
/// treat increasing y as In, decreasing y as Out. Heuristic only.
pub fn motion_direction_hint(track: &Track, min_dy: f32) -> Option<Direction> {
    if track.centroids.len() < 3 {
        return None;
    }
    let first = track.centroids.front().copied().unwrap();
    let last = track.centroids.back().copied().unwrap();
    let dy = last.1 - first.1;
    if dy.abs() < min_dy {
        return None;
    }
    if dy > 0.0 {
        Some(Direction::In)
    } else {
        Some(Direction::Out)
    }
}

/// Prefer largest quality_ok face currently in Active with best score.
pub fn prefer_commit_track<'a>(
    tracks: &'a [Track],
    zones: &[Zone],
    smart_scene_enabled: bool,
) -> Option<&'a Track> {
    let mut best: Option<&Track> = None;
    let mut best_area = -1.0f32;
    let mut best_score = -1.0f32;
    for tr in tracks {
        if !tr.quality_ok || tr.employee_id.is_none() {
            continue;
        }
        if smart_scene_enabled && !zones.is_empty() {
            if track_zone(tr.bbox, zones) != Some(ZoneKind::Active) {
                continue;
            }
        }
        let area = (tr.bbox.2 - tr.bbox.0).max(0.0) * (tr.bbox.3 - tr.bbox.1).max(0.0);
        let better = area > best_area + 1e-6
            || ((area - best_area).abs() <= 1e-6 && tr.score > best_score);
        if better {
            best = Some(tr);
            best_area = area;
            best_score = tr.score;
        }
    }
    best
}

/// HUD track state for WS (additive; frontend may ignore unknown).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudState {
    Tracking,
    Approaching,
    Ready,
    Walkby,
    LowQuality,
    Committed,
    Cooldown,
}

impl HudState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tracking => "tracking",
            Self::Approaching => "approaching",
            Self::Ready => "ready",
            Self::Walkby => "walkby",
            Self::LowQuality => "low_quality",
            Self::Committed => "committed",
            Self::Cooldown => "cooldown",
        }
    }
}

/// Derive HUD state before/without a just-completed commit decision.
pub fn hud_state(
    quality_ok: bool,
    employee_id: Option<i64>,
    zone: Option<ZoneKind>,
    is_walkby: bool,
    smart_scene_enabled: bool,
    zones_empty: bool,
) -> HudState {
    if !quality_ok {
        return HudState::LowQuality;
    }
    if is_walkby && smart_scene_enabled && !zones_empty {
        return HudState::Walkby;
    }
    if !smart_scene_enabled || zones_empty {
        return if employee_id.is_some() {
            HudState::Ready
        } else {
            HudState::Tracking
        };
    }
    match zone {
        Some(ZoneKind::Approach) => HudState::Approaching,
        Some(ZoneKind::Active) if employee_id.is_some() => HudState::Ready,
        Some(ZoneKind::Active) => HudState::Tracking,
        Some(ZoneKind::Ignore) => HudState::Tracking,
        None => HudState::Tracking,
    }
}

/// Default door layout for single cam (normalized).
pub fn default_door_zones() -> ZoneMap {
    ZoneMap {
        zones: vec![
            Zone {
                id: "active".into(),
                kind: ZoneKind::Active,
                polygon: vec![
                    (0.30, 0.25),
                    (0.70, 0.25),
                    (0.70, 0.85),
                    (0.30, 0.85),
                ],
            },
            Zone {
                id: "approach".into(),
                kind: ZoneKind::Approach,
                polygon: vec![
                    (0.15, 0.10),
                    (0.85, 0.10),
                    (0.85, 0.90),
                    (0.15, 0.90),
                ],
            },
            Zone {
                id: "ignore_left".into(),
                kind: ZoneKind::Ignore,
                // left strip — posters / frame edge
                polygon: vec![
                    (0.0, 0.0),
                    (0.12, 0.0),
                    (0.12, 1.0),
                    (0.0, 1.0),
                ],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::Track;
    use std::collections::VecDeque;

    fn active_map() -> ZoneMap {
        default_door_zones()
    }

    fn base_track(bbox: (f32, f32, f32, f32), centroids: Vec<(f32, f32)>) -> Track {
        Track {
            track_id: 1,
            bbox,
            age: 0,
            hits: 5,
            history: VecDeque::new(),
            last_commit_ts: None,
            label: "A".into(),
            employee_id: Some(1),
            score: 0.7,
            quality_ok: true,
            state: "tracking".into(),
            centroids: VecDeque::from(centroids),
        }
    }

    #[test]
    fn point_in_active_zone() {
        let z = &active_map().zones[0];
        assert!(point_in_polygon(0.5, 0.5, &z.polygon));
        assert!(!point_in_polygon(0.05, 0.05, &z.polygon));
    }

    #[test]
    fn ignore_zone_no_vote() {
        assert!(!should_vote(
            Some(ZoneKind::Ignore),
            true,
            false
        ));
        assert!(should_vote(Some(ZoneKind::Approach), true, false));
        assert!(should_vote(Some(ZoneKind::Active), true, false));
        assert!(should_vote(Some(ZoneKind::Ignore), false, false));
        assert!(should_vote(Some(ZoneKind::Ignore), true, true));
    }

    #[test]
    fn walkby_rejects_commit() {
        let zones = active_map().zones;
        let mut track = base_track(
            (0.05, 0.4, 0.15, 0.6),
            vec![
                (0.05, 0.5),
                (0.2, 0.5),
                (0.4, 0.5),
                (0.6, 0.5),
                (0.85, 0.5),
            ],
        );
        track.bbox = (0.80, 0.4, 0.90, 0.6);
        assert!(trajectory_is_walkby(&track, &zones, 3));
        assert!(!commit_eligible(&track, &zones, true, 3));
    }

    #[test]
    fn active_zone_allows_commit() {
        let zones = active_map().zones;
        let track = base_track(
            (0.4, 0.4, 0.55, 0.7),
            vec![
                (0.45, 0.5),
                (0.46, 0.51),
                (0.47, 0.52),
                (0.48, 0.53),
            ],
        );
        assert!(!trajectory_is_walkby(&track, &zones, 3));
        assert!(commit_eligible(&track, &zones, true, 3));
    }

    #[test]
    fn approach_not_eligible() {
        let zones = active_map().zones;
        // In approach ring but outside active center
        let track = base_track((0.16, 0.12, 0.25, 0.22), vec![(0.20, 0.15); 4]);
        assert_eq!(track_zone(track.bbox, &zones), Some(ZoneKind::Approach));
        assert!(!commit_eligible(&track, &zones, true, 3));
    }

    #[test]
    fn smart_off_always_eligible() {
        let zones = active_map().zones;
        let track = base_track((0.01, 0.01, 0.1, 0.1), vec![]);
        assert!(commit_eligible(&track, &zones, false, 3));
    }

    #[test]
    fn motion_hint_in_out() {
        let down = base_track(
            (0.4, 0.4, 0.5, 0.6),
            vec![(0.45, 0.2), (0.45, 0.35), (0.45, 0.55)],
        );
        assert_eq!(motion_direction_hint(&down, 0.1), Some(Direction::In));
        let up = base_track(
            (0.4, 0.4, 0.5, 0.6),
            vec![(0.45, 0.7), (0.45, 0.5), (0.45, 0.3)],
        );
        assert_eq!(motion_direction_hint(&up, 0.1), Some(Direction::Out));
    }

    #[test]
    fn hud_states() {
        assert_eq!(
            hud_state(false, None, None, false, true, false),
            HudState::LowQuality
        );
        assert_eq!(
            hud_state(true, Some(1), Some(ZoneKind::Approach), false, true, false),
            HudState::Approaching
        );
        assert_eq!(
            hud_state(true, Some(1), Some(ZoneKind::Active), false, true, false),
            HudState::Ready
        );
        assert_eq!(
            hud_state(true, Some(1), None, true, true, false),
            HudState::Walkby
        );
        assert_eq!(
            hud_state(true, Some(1), None, false, false, false).as_str(),
            "ready"
        );
    }

    #[test]
    fn prefer_largest_in_active() {
        let zones = active_map().zones;
        let small = base_track((0.45, 0.45, 0.50, 0.55), vec![]);
        let mut large = base_track((0.35, 0.30, 0.65, 0.80), vec![]);
        large.track_id = 2;
        large.score = 0.6;
        let mut outside = base_track((0.02, 0.02, 0.10, 0.20), vec![]);
        outside.track_id = 3;
        outside.score = 0.99;
        let tracks = vec![small, large.clone(), outside];
        let best = prefer_commit_track(&tracks, &zones, true).unwrap();
        assert_eq!(best.track_id, 2);
    }
}
