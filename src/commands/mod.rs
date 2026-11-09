//! Command registry. Each submodule declares `ENABLED`, `NAME`, `KEY`, `DESCRIPTION`.
pub mod help;
pub mod loop_track;
pub mod pause;
pub mod play;
pub mod quit;
pub mod search;
pub mod shuffle;
pub mod skip;
pub mod volume_down;
pub mod volume_up;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub name: &'static str,
    pub key: char,
    pub description: &'static str,
}

macro_rules! entry {
    ($($m:ident),*) => {
        [$((($m::ENABLED), Command { name: $m::NAME, key: $m::KEY, description: $m::DESCRIPTION })),*]
    };
}

fn all() -> [(bool, Command); 10] {
    entry!(
        play,
        pause,
        skip,
        shuffle,
        loop_track,
        volume_up,
        volume_down,
        search,
        help,
        quit
    )
}

/// True if `key` belongs to a command whose `ENABLED` flag is off.
pub fn disabled(key: char) -> bool {
    all().iter().any(|(on, c)| !on && c.key == key)
}

/// All enabled commands, in display order.
pub fn enabled() -> Vec<Command> {
    all()
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, c)| c)
        .collect()
}

/// Human-readable key label.
pub fn key_label(c: char) -> String {
    match c {
        '\n' => "Enter".into(),
        c => c.to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_enabled_command_is_listed_once() {
        let keys: Vec<_> = super::enabled().iter().map(|c| c.key).collect();
        assert!(keys.contains(&'p') && !super::disabled('p'));
        assert_eq!(keys.len(), 10);
        assert!(keys.contains(&'l'));
    }
}
