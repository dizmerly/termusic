//! Command registry. Each submodule declares `ENABLED`, `NAME`, `KEY`, `DESCRIPTION`.
pub mod help;
pub mod pause;
pub mod play;
pub mod quit;
pub mod search;
pub mod shuffle;
pub mod skip;

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

/// All enabled commands, in display order.
pub fn enabled() -> Vec<Command> {
    entry!(play, pause, skip, shuffle, search, help, quit)
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
