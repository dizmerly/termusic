//! Last played tracks, most recent first, kept in a small JSON file.
use crate::youtube::Track;
use std::path::Path;

pub const MAX: usize = 10;

/// Missing or corrupt file gives an empty history.
pub fn load(p: &Path) -> Vec<Track> {
    let mut v: Vec<Track> = std::fs::read(p)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    v.truncate(MAX);
    v
}

/// Best effort: a failed write must not interrupt playback.
pub fn save(p: &Path, tracks: &[Track]) {
    let _ = (|| -> std::io::Result<()> {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(tracks)?)?;
        std::fs::rename(tmp, p)
    })();
}

/// Put `t` first, dropping any older copy and everything beyond `MAX`.
pub fn push(v: &mut Vec<Track>, t: Track) {
    v.retain(|x| x.id != t.id);
    v.insert(0, t);
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
            (v.len(), v[0].id.as_str(), v[9].id.as_str()),
            (10, "11", "2")
        );
        push(&mut v, t("5"));
        assert_eq!(
            (v.len(), v[0].id.as_str(), v[1].id.as_str()),
            (10, "5", "11")
        );
        assert_eq!(v.iter().filter(|x| x.id == "5").count(), 1);
    }

    #[test]
    fn save_load_roundtrip_missing_corrupt() {
        let dir = std::env::temp_dir().join(format!("termusic-hist-{}", std::process::id()));
        let p = dir.join("sub").join("history.json");
        assert!(load(&p).is_empty()); // missing
        let v = vec![t("a"), t("b")];
        save(&p, &v);
        assert_eq!(load(&p), v);
        std::fs::write(&p, "{nope").unwrap();
        assert!(load(&p).is_empty()); // corrupt
        let long: Vec<Track> = (0..15).map(|i| t(&i.to_string())).collect();
        save(&p, &long);
        assert_eq!(load(&p).len(), MAX);
        let _ = std::fs::remove_dir_all(dir);
    }
}
