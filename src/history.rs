//! Last played tracks, most recent first, kept in a small JSON file.
use crate::youtube::Track;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MAX: usize = 10;

/// A played track and where the listener left off (0 = start over).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    #[serde(flatten)]
    pub track: Track,
    #[serde(default)]
    pub position_secs: u64,
}

/// Where to resume a track that was at `pos` of `dur`: the very start and the last 10 s restart.
pub fn resume_point(pos: f64, dur: Option<f64>) -> u64 {
    if pos < 10.0 || dur.is_some_and(|d| pos > d - 10.0) {
        0
    } else {
        pos as u64
    }
}

/// Missing or corrupt file gives an empty history.
pub fn load(p: &Path) -> Vec<Entry> {
    let mut v: Vec<Entry> = std::fs::read(p)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    v.truncate(MAX);
    v
}

/// Atomic write (tmp + rename); the caller decides how to report a failure.
pub fn save(p: &Path, entries: &[Entry]) -> std::io::Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(entries)?)?;
    std::fs::rename(tmp, p)
}

/// Put `t` first (position 0), dropping any older copy and everything beyond `MAX`.
pub fn push(v: &mut Vec<Entry>, t: Track) {
    v.retain(|x| x.track.id != t.id);
    v.insert(
        0,
        Entry {
            track: t,
            position_secs: 0,
        },
    );
    v.truncate(MAX);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str) -> Track {
        Track {
            title: format!("title {id}"),
            channel: "ch".into(),
            id: id.into(),
        }
    }

    #[test]
    fn push_dedups_orders_and_caps() {
        let mut v = Vec::new();
        for i in 0..12 {
            push(&mut v, t(&i.to_string()));
        }
        assert_eq!(
            (v.len(), v[0].track.id.as_str(), v[9].track.id.as_str()),
            (10, "11", "2")
        );
        push(&mut v, t("5"));
        assert_eq!(
            (v.len(), v[0].track.id.as_str(), v[1].track.id.as_str()),
            (10, "5", "11")
        );
        assert_eq!(v.iter().filter(|x| x.track.id == "5").count(), 1);
    }

    #[test]
    fn save_load_roundtrip_missing_corrupt() {
        let dir = std::env::temp_dir().join(format!("termusic-hist-{}", std::process::id()));
        let p = dir.join("sub").join("history.json");
        assert!(load(&p).is_empty()); // missing
        let v: Vec<Entry> = vec![t("a"), t("b")]
            .into_iter()
            .map(|track| Entry {
                track,
                position_secs: 42,
            })
            .collect();
        save(&p, &v).unwrap();
        assert_eq!(load(&p), v);
        std::fs::write(&p, "{nope").unwrap();
        assert!(load(&p).is_empty()); // corrupt
        let mut long = Vec::new();
        for i in 0..15 {
            push(&mut long, t(&i.to_string()));
        }
        long.extend(v);
        save(&p, &long).unwrap();
        assert_eq!(load(&p).len(), MAX);
        assert!(save(&p.join("x.json"), &long).is_err()); // parent is a file
        let _ = std::fs::remove_file(&p);
        // Old files (plain tracks, no position) still load.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&p, r#"[{"title":"t","channel":"c","id":"i"}]"#).unwrap();
        assert_eq!(load(&p)[0].position_secs, 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn resume_point_skips_start_and_end() {
        assert_eq!(resume_point(5.0, Some(200.0)), 0);
        assert_eq!(resume_point(95.7, Some(200.0)), 95);
        assert_eq!(resume_point(195.0, Some(200.0)), 0);
        assert_eq!(resume_point(95.0, None), 95);
    }
}
