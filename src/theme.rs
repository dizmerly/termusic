//! Logo, icons and colors.
use ratatui::{
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType},
};

pub const LOGO: [&str; 5] = [
    r"  _                           _",
    r" | |_ ___ _ __ _ __ ___  _   _ ___(_) ___",
    r" | __/ _ \ '__| '_ ` _ \| | | / __| |/ __|",
    r" | ||  __/ |  | | | | | | |_| \__ \ | (__",
    r"  \__\___|_|  |_| |_| |_|\__,_|___/_|\___|",
];

pub const ICON_NOTE: &str = "♪";
pub const ICON_PLAY: &str = "▶";
pub const ICON_PAUSE: &str = "⏸";
pub const ICON_SHUFFLE: &str = "🔀";
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
pub const LOGO_GRADIENT: [Color; 5] = [PINK, ORANGE, YELLOW, GREEN, CYAN];

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
