//! Daily attendance aggregate.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Absent,
    Present,
    Incomplete,
    Anomaly,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Present => "present",
            Self::Incomplete => "incomplete",
            Self::Anomaly => "anomaly",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RawEvent {
    pub employee_id: i64,
    pub kind: String,
    pub ts: NaiveDateTime,
}

#[derive(Debug, Clone)]
pub struct EmployeeRef {
    pub id: i64,
    pub employee_code: String,
    pub full_name: String,
    pub department: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyRow {
    pub employee_id: i64,
    pub employee_code: String,
    pub full_name: String,
    pub department: Option<String>,
    pub first_in: Option<NaiveDateTime>,
    pub last_out: Option<NaiveDateTime>,
    pub duration_minutes: Option<i64>,
    pub status: String,
    pub check_in_count: i32,
    pub check_out_count: i32,
}

pub fn derive_status(
    first_in: Option<NaiveDateTime>,
    last_out: Option<NaiveDateTime>,
    check_in_count: i32,
    check_out_count: i32,
) -> Status {
    let _ = (first_in, last_out);
    if check_in_count == 0 && check_out_count == 0 {
        return Status::Absent;
    }
    if check_in_count > 0 && check_out_count == 0 {
        return Status::Incomplete;
    }
    if check_in_count > 0 && check_out_count > 0 {
        return Status::Present;
    }
    if check_out_count > 0 && check_in_count == 0 {
        return Status::Anomaly;
    }
    Status::Absent
}

pub fn aggregate_daily(employees: &[EmployeeRef], events: &[RawEvent]) -> Vec<DailyRow> {
    use std::collections::HashMap;
    let mut by_emp: HashMap<i64, Vec<&RawEvent>> = HashMap::new();
    for ev in events {
        if ev.kind != "check_in" && ev.kind != "check_out" {
            continue;
        }
        by_emp.entry(ev.employee_id).or_default().push(ev);
    }

    let mut rows = Vec::new();
    for emp in employees {
        let mut evs = by_emp.get(&emp.id).cloned().unwrap_or_default();
        evs.sort_by_key(|e| e.ts);
        let ins: Vec<_> = evs.iter().filter(|e| e.kind == "check_in").collect();
        let outs: Vec<_> = evs.iter().filter(|e| e.kind == "check_out").collect();
        let first_in = ins.first().map(|e| e.ts);
        let last_out = outs.last().map(|e| e.ts);
        let duration = match (first_in, last_out) {
            (Some(fi), Some(lo)) if lo >= fi => Some((lo - fi).num_minutes()),
            _ => None,
        };
        let status = derive_status(
            first_in,
            last_out,
            ins.len() as i32,
            outs.len() as i32,
        );
        rows.push(DailyRow {
            employee_id: emp.id,
            employee_code: emp.employee_code.clone(),
            full_name: emp.full_name.clone(),
            department: emp.department.clone(),
            first_in,
            last_out,
            duration_minutes: duration,
            status: status.as_str().to_string(),
            check_in_count: ins.len() as i32,
            check_out_count: outs.len() as i32,
        });
    }
    rows.sort_by(|a, b| a.full_name.to_lowercase().cmp(&b.full_name.to_lowercase()));
    rows
}

pub fn daily_csv_headers() -> Vec<&'static str> {
    vec![
        "date",
        "employee_code",
        "name",
        "department",
        "first_in",
        "last_out",
        "duration_minutes",
        "status",
        "check_in_count",
        "check_out_count",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 7, 12)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    #[test]
    fn derive_status_matrix() {
        let t1 = dt(8, 0);
        let t2 = dt(17, 0);
        assert_eq!(derive_status(None, None, 0, 0), Status::Absent);
        assert_eq!(derive_status(Some(t1), None, 1, 0), Status::Incomplete);
        assert_eq!(derive_status(Some(t1), Some(t2), 1, 1), Status::Present);
        assert_eq!(derive_status(None, Some(t2), 0, 1), Status::Anomaly);
    }

    #[test]
    fn aggregate_daily_rows() {
        let emps = vec![
            EmployeeRef {
                id: 1,
                employee_code: "E1".into(),
                full_name: "Alice".into(),
                department: Some("Eng".into()),
            },
            EmployeeRef {
                id: 2,
                employee_code: "E2".into(),
                full_name: "Bob".into(),
                department: None,
            },
        ];
        let events = vec![
            RawEvent {
                employee_id: 1,
                kind: "check_in".into(),
                ts: dt(8, 0),
            },
            RawEvent {
                employee_id: 1,
                kind: "check_out".into(),
                ts: dt(17, 0),
            },
            RawEvent {
                employee_id: 2,
                kind: "check_in".into(),
                ts: dt(9, 0),
            },
        ];
        let rows = aggregate_daily(&emps, &events);
        let by_code: std::collections::HashMap<_, _> =
            rows.into_iter().map(|r| (r.employee_code.clone(), r)).collect();
        assert_eq!(by_code["E1"].status, "present");
        assert_eq!(by_code["E1"].duration_minutes, Some(540));
        assert_eq!(by_code["E1"].check_in_count, 1);
        assert_eq!(by_code["E1"].check_out_count, 1);
        assert_eq!(by_code["E2"].status, "incomplete");
        assert!(by_code["E2"].last_out.is_none());
    }

    #[test]
    fn csv_headers() {
        let h = daily_csv_headers();
        assert_eq!(h[0], "date");
        assert!(h.contains(&"first_in"));
        assert!(h.contains(&"status"));
        assert!(h.contains(&"check_out_count"));
    }
}
