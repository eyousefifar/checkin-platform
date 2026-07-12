//! Pure PKSP algorithms — zero I/O (no network, DB, or ONNX).

pub mod daily;
pub mod embed;
pub mod fsm;
pub mod match_;
pub mod quality;
pub mod scene;
pub mod track;
pub mod vote;

pub use daily::{
    aggregate_daily, daily_csv_headers, derive_status, DailyRow, EmployeeRef, RawEvent, Status,
};
pub use embed::{l2_normalize, mean_l2_embedding, pack_embedding, unpack_embedding, EmbedError};
pub use fsm::{in_cooldown, on_identity_commit, resolve_kind, Direction, EventKind, FsmDecision, PriorEvent, SkipReason};
pub use match_::{cosine_scores, match_top1, MatchResult};
pub use quality::{
    blur_ok, blur_variance, exposure_ok, mean_luma, pose_ok, pose_yaw_approx, quality_gate,
    quality_gate_extended, QualityResult,
};
pub use scene::{
    bbox_centroid, commit_eligible, default_door_zones, hud_state, motion_direction_hint,
    point_in_polygon, prefer_commit_track, should_vote, track_zone, trajectory_is_walkby, HudState,
    Zone, ZoneKind, ZoneMap,
};
pub use track::{assign_tracks, iou, Detection, Track, TrackerState};
pub use vote::TrackVote;
pub use vote::{evaluate_vote, VoteCommit};
