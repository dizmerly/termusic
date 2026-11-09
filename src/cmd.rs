//! Playback commands (what the user asks for) and player events (what happened).
//! TUI keys map to `Cmd` here; media keys or a menu-bar helper can send the same `Cmd`s.
use crate::commands;
use crossterm::event::{KeyCode, KeyEvent};

/// Volume and seek step per key press.
const STEP: i32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    PlayPause,
    Next,
    Prev,
    /// Seconds, negative = back.
    Seek(i32),
    /// Volume change in percent points.
    Volume(i32),
    ToggleShuffle,
    CycleLoop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Playback of the track with this id started.
    TrackStarted(String),
    /// The track played to its end.
    TrackEnded,
    /// mpv exited right away (track unavailable or mpv/yt-dlp missing).
    TrackFailed,
    /// Connectivity changed: true = online.
    Connection(bool),
}

/// The playback command a key triggers, if any; keys of disabled commands give `None`.
pub fn from_key(key: KeyEvent) -> Option<Cmd> {
    let KeyCode::Char(c) = key.code else {
        return None;
    };
    if commands::disabled(c) {
        return None;
    }
    Some(match c {
        commands::pause::KEY => Cmd::PlayPause,
        commands::skip::KEY => Cmd::Next,
        commands::prev::KEY => Cmd::Prev,
        commands::seek_back::KEY => Cmd::Seek(-STEP),
        commands::seek_forward::KEY => Cmd::Seek(STEP),
        commands::volume_up::KEY => Cmd::Volume(STEP),
        commands::volume_down::KEY => Cmd::Volume(-STEP),
        commands::shuffle::KEY => Cmd::ToggleShuffle,
        commands::loop_track::KEY => Cmd::CycleLoop,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(c: KeyCode) -> Option<Cmd> {
        from_key(KeyEvent::new(c, KeyModifiers::NONE))
    }

    #[test]
    fn playback_keys_map_to_cmds() {
        for (c, want) in [
            ('p', Cmd::PlayPause),
            ('n', Cmd::Next),
            ('b', Cmd::Prev),
            (',', Cmd::Seek(-5)),
            ('.', Cmd::Seek(5)),
            ('+', Cmd::Volume(5)),
            ('-', Cmd::Volume(-5)),
            ('s', Cmd::ToggleShuffle),
            ('l', Cmd::CycleLoop),
        ] {
            assert_eq!(key(KeyCode::Char(c)), Some(want), "{c}");
        }
    }

    #[test]
    fn other_keys_are_not_cmds() {
        for c in ['j', 'k', 'a', 'q', '/', '?', 'P', 'x'] {
            assert_eq!(key(KeyCode::Char(c)), None, "{c}");
        }
        assert_eq!(key(KeyCode::Enter), None);
        assert_eq!(key(KeyCode::Down), None);
    }

    #[test]
    fn disabled_commands_map_to_none() {
        for c in ['p', 'n', 'b', ',', '.', '+', '-', 's', 'l'] {
            assert_eq!(
                key(KeyCode::Char(c)).is_none(),
                commands::disabled(c),
                "{c}"
            );
        }
    }
}
