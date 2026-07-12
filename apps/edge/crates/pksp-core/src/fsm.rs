//! Attendance FSM + cooldown.

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
    Bidirectional,
}

impl Direction {
    pub fn parse(s: &str) -> Self {
        match s {
            "out" => Self::Out,
            "bidirectional" => Self::Bidirectional,
            _ => Self::In,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    CheckIn,
    CheckOut,
    Unrecognized,
    RejectedSpoof,
    RejectedLowConf,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CheckIn => "check_in",
            Self::CheckOut => "check_out",
            Self::Unrecognized => "unrecognized",
            Self::RejectedSpoof => "rejected_spoof",
            Self::RejectedLowConf => "rejected_low_conf",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "check_in" => Some(Self::CheckIn),
            "check_out" => Some(Self::CheckOut),
            "unrecognized" => Some(Self::Unrecognized),
            "rejected_spoof" => Some(Self::RejectedSpoof),
            "rejected_low_conf" => Some(Self::RejectedLowConf),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    Cooldown,
    NoTransition,
    MinDwell,
}

impl SkipReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cooldown => "cooldown",
            Self::NoTransition => "no_transition",
            Self::MinDwell => "min_dwell",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PriorEvent {
    pub kind: EventKind,
    pub ts: DateTime<Utc>,
    pub camera_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsmDecision {
    Commit { kind: EventKind },
    Skip { reason: SkipReason },
}

pub fn in_cooldown(
    last_same_camera_ts: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    cooldown_seconds: f64,
) -> bool {
    match last_same_camera_ts {
        None => false,
        Some(last) => {
            let delta = (now - last).num_milliseconds() as f64 / 1000.0;
            delta < cooldown_seconds
        }
    }
}

pub fn resolve_kind(
    direction: Direction,
    last_today: Option<&PriorEvent>,
    now: DateTime<Utc>,
    min_dwell_seconds: f64,
) -> Option<EventKind> {
    match direction {
        Direction::In => Some(EventKind::CheckIn),
        Direction::Out => Some(EventKind::CheckOut),
        Direction::Bidirectional => match last_today {
            None => Some(EventKind::CheckIn),
            Some(last) if last.kind == EventKind::CheckOut => Some(EventKind::CheckIn),
            Some(last) if last.kind == EventKind::CheckIn => {
                let dwell = (now - last.ts).num_milliseconds() as f64 / 1000.0;
                if dwell >= min_dwell_seconds {
                    Some(EventKind::CheckOut)
                } else {
                    None
                }
            }
            _ => Some(EventKind::CheckIn),
        },
    }
}

pub fn on_identity_commit(
    direction: Direction,
    now: DateTime<Utc>,
    last_today: Option<&PriorEvent>,
    last_same_camera_ts: Option<DateTime<Utc>>,
    cooldown_seconds: f64,
    min_dwell_seconds: f64,
) -> FsmDecision {
    if in_cooldown(last_same_camera_ts, now, cooldown_seconds) {
        return FsmDecision::Skip {
            reason: SkipReason::Cooldown,
        };
    }
    match resolve_kind(direction, last_today, now, min_dwell_seconds) {
        Some(kind) => FsmDecision::Commit { kind },
        None => FsmDecision::Skip {
            reason: SkipReason::NoTransition,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 12, h, m, 0).unwrap()
    }

    #[test]
    fn in_camera_always_check_in() {
        let d = on_identity_commit(Direction::In, ts(8, 0), None, None, 90.0, 30.0);
        assert_eq!(
            d,
            FsmDecision::Commit {
                kind: EventKind::CheckIn
            }
        );
    }

    #[test]
    fn out_camera_always_check_out() {
        let prior = PriorEvent {
            kind: EventKind::CheckIn,
            ts: ts(8, 0),
            camera_id: "cam_in".into(),
        };
        let d = on_identity_commit(Direction::Out, ts(17, 0), Some(&prior), None, 90.0, 30.0);
        assert_eq!(
            d,
            FsmDecision::Commit {
                kind: EventKind::CheckOut
            }
        );
    }

    #[test]
    fn bidirectional_walk_in_then_out_after_dwell() {
        let d1 = on_identity_commit(Direction::Bidirectional, ts(8, 0), None, None, 90.0, 30.0);
        assert_eq!(
            d1,
            FsmDecision::Commit {
                kind: EventKind::CheckIn
            }
        );

        let prior = PriorEvent {
            kind: EventKind::CheckIn,
            ts: ts(8, 0),
            camera_id: "cam_in".into(),
        };
        let d2 = on_identity_commit(
            Direction::Bidirectional,
            ts(8, 1),
            Some(&prior),
            Some(ts(8, 0)),
            30.0,
            30.0,
        );
        assert_eq!(
            d2,
            FsmDecision::Commit {
                kind: EventKind::CheckOut
            }
        );
    }

    #[test]
    fn cooldown_blocks_double_punch() {
        let now = ts(8, 1);
        let last = now - chrono::Duration::seconds(10);
        let prior = PriorEvent {
            kind: EventKind::CheckIn,
            ts: last,
            camera_id: "cam_in".into(),
        };
        let d = on_identity_commit(Direction::In, now, Some(&prior), Some(last), 90.0, 30.0);
        assert_eq!(
            d,
            FsmDecision::Skip {
                reason: SkipReason::Cooldown
            }
        );
    }

    #[test]
    fn in_cooldown_helper() {
        let now = Utc::now();
        assert!(in_cooldown(
            Some(now - chrono::Duration::seconds(10)),
            now,
            90.0
        ));
        assert!(!in_cooldown(
            Some(now - chrono::Duration::seconds(100)),
            now,
            90.0
        ));
        assert!(!in_cooldown(None, now, 90.0));
    }

    #[test]
    fn bidirectional_no_transition_before_dwell() {
        let prior = PriorEvent {
            kind: EventKind::CheckIn,
            ts: ts(8, 0),
            camera_id: "c".into(),
        };
        let kind = resolve_kind(
            Direction::Bidirectional,
            Some(&prior),
            ts(8, 0) + chrono::Duration::seconds(10),
            30.0,
        );
        assert!(kind.is_none());
    }
}
