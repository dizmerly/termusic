//! Small persistent settings (`settings.json` in the config dir).
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_VOLUME: u8 = 70;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub volume: u8,
    /// Where the play history lives; `None` = `history.json` next to settings.json.
    pub history_path: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: DEFAULT_VOLUME,
            history_path: None,
        }
    }
}

/// `$TERMUSIC_CONFIG_DIR` if set, else `<user config dir>/termusic`.
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("TERMUSIC_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::config_dir().map(|d| d.join("termusic")))
}

fn path() -> Option<PathBuf> {
    // Tests must never touch the real config.
    if cfg!(test) {
        return None;
    }
    config_dir().map(|d| d.join("settings.json"))
}

/// History file: `$TERMUSIC_HISTORY` wins, then the `history_path` setting, then `<dir>/history.json`.
fn resolve_history_path(
    env: Option<std::ffi::OsString>,
    setting: Option<&Path>,
    dir: Option<PathBuf>,
) -> Option<PathBuf> {
    env.filter(|e| !e.is_empty())
        .map(PathBuf::from)
        .or_else(|| setting.map(Path::to_path_buf))
        .or_else(|| dir.map(|d| d.join("history.json")))
}

impl Settings {
    /// The history file in use; `None` under test so tests never touch the real config.
    pub fn history_file(&self) -> Option<PathBuf> {
        if cfg!(test) {
            return None;
        }
        resolve_history_path(
            std::env::var_os("TERMUSIC_HISTORY"),
            self.history_path.as_deref(),
            config_dir(),
        )
    }

    /// Missing or corrupt file gives the defaults.
    pub fn load() -> Self {
        path().map(|p| Self::load_from(&p)).unwrap_or_default()
    }

    /// Best effort: a failed write must not interrupt playback.
    pub fn save(&self) {
        if let Some(p) = path() {
            let _ = self.save_to(&p);
        }
    }

    fn load_from(p: &Path) -> Self {
        let mut s: Self = std::fs::read(p)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        s.volume = s.volume.min(100);
        s
    }

    fn save_to(&self, p: &Path) -> std::io::Result<()> {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("termusic-set-{}-{tag}", std::process::id()))
    }

    #[test]
    fn roundtrip_missing_and_corrupt() {
        let dir = tmp("rt");
        let p = dir.join("settings.json");
        assert_eq!(Settings::load_from(&p), Settings::default()); // missing
        Settings {
            volume: 33,
            history_path: Some("/h.json".into()),
        }
        .save_to(&p)
        .unwrap();
        let got = Settings::load_from(&p);
        assert_eq!((got.volume, got.history_path), (33, Some("/h.json".into())));
        std::fs::write(&p, "{not json").unwrap();
        assert_eq!(Settings::load_from(&p), Settings::default()); // corrupt
        std::fs::write(&p, "{}").unwrap();
        assert_eq!(Settings::load_from(&p), Settings::default()); // missing field
        std::fs::write(&p, r#"{"volume":250}"#).unwrap();
        assert_eq!(Settings::load_from(&p).volume, 100); // clamped
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn history_path_precedence() {
        let (e, s, d) = (
            Some("/env.json".into()),
            Some(Path::new("/set.json")),
            Some("/cfg".into()),
        );
        assert_eq!(
            resolve_history_path(e, s, d.clone()),
            Some("/env.json".into())
        );
        assert_eq!(
            resolve_history_path(Some("".into()), s, d.clone()),
            Some("/set.json".into())
        );
        assert_eq!(
            resolve_history_path(None, None, d),
            Some("/cfg/history.json".into())
        );
        assert_eq!(resolve_history_path(None, None, None), None);
        assert_eq!(Settings::default().history_file(), None); // test mode
    }
}
