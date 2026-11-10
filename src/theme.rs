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

pub const PINK: Color = Color::Rgb(255, 105, 180);
pub const ORANGE: Color = Color::Rgb(255, 170, 100);
pub const YELLOW: Color = Color::Rgb(255, 224, 120);
pub const GREEN: Color = Color::Rgb(130, 240, 170);
pub const CYAN: Color = Color::Rgb(100, 220, 255);
pub const PURPLE: Color = Color::Rgb(180, 140, 255);
pub const ROW_BG: Color = Color::Rgb(58, 42, 92);
pub const BAR_BG: Color = Color::Rgb(30, 30, 46);

/// One color per logo line, top to bottom.
pub const LOGO_GRADIENT: [Color; 8] = [PINK, PINK, ORANGE, YELLOW, GREEN, CYAN, CYAN, PURPLE];

pub fn title() -> Style {
    Style::default().fg(PINK).add_modifier(Modifier::BOLD)
}
pub fn selected() -> Style {
    Style::default().bg(ROW_BG).add_modifier(Modifier::BOLD)
}
pub fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
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
    fn style_helpers_use_the_palette() {
        assert_eq!(fg(CYAN).fg, Some(CYAN));
        assert_eq!(title().fg, Some(PINK));
        assert!(title().add_modifier.contains(Modifier::BOLD));
        assert_eq!(selected().bg, Some(ROW_BG));
        assert_eq!(dim().fg, Some(Color::DarkGray));
        let _ = panel("t", GREEN); // builds without panicking
    }
}
