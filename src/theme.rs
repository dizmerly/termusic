//! Logo, icons and colors.
use ratatui::{
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType},
};

/// All lines must be the same width: `Paragraph` centers each line separately.
pub const LOGO: [&str; 3] = [
    "╔╦╗╔═╗╦═╗╔╦╗╦ ╦╔═╗╦╔═╗",
    " ║ ║╣ ╠╦╝║║║║ ║╚═╗║║  ",
    " ╩ ╚═╝╩╚═╩ ╩╚═╝╚═╝╩╚═╝",
];

pub const ICON_NOTE: &str = "♪";
pub const ICON_PLAY: &str = "▶";
pub const ICON_PAUSE: &str = "⏸";
pub const ICON_SHUFFLE: &str = "⤮";
pub const ICON_LOOP: &str = "↻";
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
pub const LOGO_GRADIENT: [Color; 3] = [PINK, YELLOW, CYAN];

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
    fn logo_lines_are_equal_width() {
        let w = LOGO[0].chars().count();
        assert!(LOGO.iter().all(|l| l.chars().count() == w));
    }
}
