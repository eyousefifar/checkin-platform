//! Load per-camera zone maps from JSON.

use pksp_core::{default_door_zones, ZoneMap};
use std::path::Path;
use tracing::warn;

/// Load `zones.{camera_id}.json` from dir, else default door layout.
pub fn load_zones_for_camera(dir: &Path, camera_id: &str) -> ZoneMap {
    let path = dir.join(format!("zones.{camera_id}.json"));
    if path.is_file() {
        match std::fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str::<ZoneMap>(&s) {
                Ok(m) if !m.zones.is_empty() => return m,
                Ok(_) => warn!("zone file empty: {}", path.display()),
                Err(e) => warn!("zone file parse error {}: {e}", path.display()),
            },
            Err(e) => warn!("zone file read error {}: {e}", path.display()),
        }
    }
    default_door_zones()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn fallback_default() {
        let m = load_zones_for_camera(Path::new("/nonexistent"), "cam_in");
        assert!(!m.zones.is_empty());
    }

    #[test]
    fn loads_json() {
        let dir = std::env::temp_dir().join(format!("pksp-zones-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("zones.cam_x.json");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            r#"{{"zones":[{{"id":"a","kind":"active","polygon":[[0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0]]}}]}}"#
        )
        .unwrap();
        let m = load_zones_for_camera(&dir, "cam_x");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(m.zones.len(), 1);
        assert_eq!(m.zones[0].id, "a");
    }
}
