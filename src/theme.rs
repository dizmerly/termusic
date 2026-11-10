//! Logo, icons and colors.
use ratatui::{
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType},
};

/// All lines must be the same width: `Paragraph` centers each line separately.
pub const LOGO: [&str; 8] = [
    r" _________  _______   ________  _____ ______   ___  ___  ________  ___  ________     ",
    r"|\___   ___\\  ___ \ |\   __  \|\   _ \  _   \|\  \|\  \|\   ____\|\  \|\   ____\    ",
    r"\|___ \  \_\ \   __/|\ \  \|\  \ \  \\\__\ \  \ \  \\\  \ \  \___|\ \  \ \  \___|    ",
    r"     \ \  \ \ \  \_|/_\ \   _  _\ \  \\|__| \  \ \  \\\  \ \_____  \ \  \ \  \       ",
    r"      \ \  \ \ \  \_|\ \ \  \\  \\ \  \    \ \  \ \  \\\  \|____|\  \ \  \ \  \____  ",
    r"       \ \__\ \ \_______\ \__\\ _\\ \__\    \ \__\ \_______\____\_\  \ \__\ \_______\",
    r"        \|__|  \|_______|\|__|\|__|\|__|     \|__|\|_______|\_________\|__|\|_______|",
    r"                                                            \|_________|             ",
];

/// Cassette art. Keep every line the same width. `@` marks a reel hub (swapped for a
/// spinning glyph each frame); the `#` run is the now-playing field.
pub const CASSETTE: [&str; 13] = [
    r#"   ____________________________ "#,
    r#" /|............................|"#,
    r#"| |:##########################:|"#,
    r#"| |:                          :|"#,
    r#"| |:     ,-.   _____   ,-.    :|"#,
    r#"| |:    ( @)) [_____] ( @))   :|"#,
    r#"|v|:     `-`   ' ' '   `-`    :|"#,
    r#"|||:     ,______________.     :|"#,
    r#"|||...../::::o::::::o::::\.....|"#,
    r#"|^|..../:::O::::::::::O:::\....|"#,
    r#"|/`---/--------------------`---|"#,
    r#"`.___/ /====/ /=//=/ /====/____/"#,
    r#"     `--------------------'     "#,
];
/// Frames for a spinning reel hub.
pub const REEL_FRAMES: [char; 4] = ['|', '/', '-', '\\'];

pub const ICON_NOTE: &str = "♪";
pub const ICON_PLAY: &str = "▶";
pub const ICON_PAUSE: &str = "⏸";
pub const ICON_SHUFFLE: &str = "⤮";
pub const ICON_LOOP: &str = "↻";
pub const ICON_LOOP_ONE: &str = "↻1";
pub const ICON_BUSY: &str = "⟳";

/// A named color scheme: every color the UI draws comes from the active shell's roles.
#[derive(Debug, PartialEq, Eq)]
pub struct Shell {
    /// Shown in the UI.
    pub name: &'static str,
    /// Stored in settings.json.
    pub id: &'static str,
    pub body: Color,
    pub frame: Color,
    pub text: Color,
    pub dim: Color,
    pub accent: Color,
    pub label: Color,
    pub label_ink: Color,
    pub meter_lo: Color,
    pub meter_hi: Color,
    pub select_bg: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const SUNSET: Shell = Shell {
    name: "SUNSET TAPE",
    id: "sunset",
    body: rgb(0x1A1026),
    frame: rgb(0xFF69B4),
    text: rgb(0xFFE6F4),
    dim: rgb(0x9A7FBD),
    accent: rgb(0xFFAA64),
    label: rgb(0xFFE078),
    label_ink: rgb(0x3A2A5C),
    meter_lo: rgb(0x64DCFF),
    meter_hi: rgb(0xFF4D8D),
    select_bg: rgb(0x3A2A5C),
};

pub const TPS_L2: Shell = Shell {
    name: "TPS-L2",
    id: "tps-l2",
    body: rgb(0x16284F),
    frame: rgb(0xB9C2D3),
    text: rgb(0xE8ECF4),
    dim: rgb(0x7F8FB3),
    accent: rgb(0xFF7A1A),
    label: rgb(0xF2E8CF),
    label_ink: rgb(0x26221C),
    meter_lo: rgb(0x7FD1B9),
    meter_hi: rgb(0xFF4D3D),
    select_bg: rgb(0x2A4687),
};

pub const AMBER: Shell = Shell {
    name: "AMBER HI-FI",
    id: "amber",
    body: rgb(0x241812),
    frame: rgb(0xC9A27A),
    text: rgb(0xF3E3C3),
    dim: rgb(0x94795C),
    accent: rgb(0xFFB000),
    label: rgb(0xEADBB8),
    label_ink: rgb(0x3B2A1A),
    meter_lo: rgb(0x5CFFB0),
    meter_hi: rgb(0xFF6A3D),
    select_bg: rgb(0x4A3423),
};

/// Settings order; the first is the default.
pub const SHELLS: [&Shell; 3] = [&SUNSET, &TPS_L2, &AMBER];

/// Index into `SHELLS` of the shell with this id; an unknown id gives the default.
pub fn shell_index(id: &str) -> usize {
    SHELLS.iter().position(|s| s.id == id).unwrap_or(0)
}

pub fn shell(id: &str) -> &'static Shell {
    SHELLS[shell_index(id)]
}

pub fn title(s: &Shell) -> Style {
    fg(s.accent).add_modifier(Modifier::BOLD)
}
pub fn selected(s: &Shell) -> Style {
    Style::default()
        .bg(s.select_bg)
        .add_modifier(Modifier::BOLD)
}
pub fn dim(s: &Shell) -> Style {
    fg(s.dim)
}
pub fn fg(c: Color) -> Style {
    Style::default().fg(c)
}

/// Rounded panel with a colored border and bold matching title.
pub fn panel(title: &str, color: Color) -> Block<'_> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(fg(color))
        .title(format!(" {title} "))
        .title_style(fg(color).add_modifier(Modifier::BOLD))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cassette_lines_are_equal_width() {
        use unicode_width::UnicodeWidthStr;
        let w = CASSETTE[0].width();
        assert!(CASSETTE.iter().all(|l| l.width() == w));
    }

    #[test]
    fn logo_lines_are_equal_width() {
        let w = LOGO[0].chars().count();
        assert!(LOGO.iter().all(|l| l.chars().count() == w));
    }

    #[test]
    fn style_helpers_use_the_shell() {
        let s = &SUNSET;
        assert_eq!(fg(s.text).fg, Some(s.text));
        assert_eq!(title(s).fg, Some(s.accent));
        assert!(title(s).add_modifier.contains(Modifier::BOLD));
        assert_eq!(selected(s).bg, Some(s.select_bg));
        assert_eq!(dim(s).fg, Some(s.dim));
        let _ = panel("t", s.frame); // builds without panicking
    }

    #[test]
    fn shells_resolve_by_id_and_unknown_falls_back() {
        assert_eq!(rgb(0x16284F), Color::Rgb(0x16, 0x28, 0x4F));
        for (i, s) in SHELLS.iter().enumerate() {
            assert_eq!(shell_index(s.id), i);
            assert_eq!(shell(s.id).name, s.name);
        }
        assert_eq!(shell("bogus"), SHELLS[0]);
        assert_eq!(shell(""), SHELLS[0]);
    }
}
