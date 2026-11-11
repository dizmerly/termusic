//! Help, Settings and Offline, drawn as a panel over the J-card pane.
use super::{fit, wrap};
use crate::app::{App, Row, Screen};
use crate::commands;
use crate::theme::{self, Shell, fg};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    match app.screen {
        Screen::Help => draw_help(f, app, area),
        _ => draw_settings(f, app, area),
    }
}

/// Help sections: commands are grouped by name; any enabled command not named here lands in "Other".
const GROUPS: [(&str, &[&str]); 4] = [
    (
        "Playback",
        &[
            "play",
            "pause",
            "skip",
            "prev",
            "seek_back",
            "seek_forward",
            "volume_up",
            "volume_down",
            "shuffle",
            "loop",
        ],
    ),
    ("Queue", &["queue"]),
    ("Navigation", &["search", "playlists"]),
    ("Screens", &["history", "settings", "help", "quit"]),
];

/// Navigation and screen keys that are not commands.
const NAV_KEYS: [(&str, &str); 13] = [
    ("j/k ↓/↑", "move down / up in a list (scroll in Help)"),
    ("PgDn/Up", "move one page in a list"),
    ("g / G", "jump to the first / last row"),
    (
        "Enter",
        "run the search, play the highlighted row, or jump to it in Queue",
    ),
    (
        "Esc",
        "leave the search field, close an overlay; in a list: search",
    ),
    ("Tab", "next tab; in the search field: back to the list"),
    ("Shift-Tab", "previous tab"),
    ("1-4", "Results, Queue, Lists, History tab"),
    (
        "Ctrl-W",
        "in the search field: delete a word (also Alt/Ctrl-Backspace)",
    ),
    ("F1", "help (same as ?)"),
    ("F2", "Lists tab (same as P), also while typing a search"),
    ("F3", "settings (same as S), also while typing a search"),
    ("F4", "History tab (same as H), also while typing a search"),
];

/// Key column width; descriptions start after it and two spaces.
const KEY_W: usize = 9;

/// Help text wrapped to `width` columns; descriptions continue under themselves.
pub fn help_lines(s: &Shell, width: usize) -> Vec<Line<'static>> {
    let head = |t: &str| Line::styled(t.to_owned(), fg(s.accent).bold());
    let room = width.saturating_sub(KEY_W + 2).max(10);
    let key = |k: String, d: String| {
        wrap(&d, room)
            .into_iter()
            .enumerate()
            .map(|(i, part)| {
                let k = if i == 0 { k.as_str() } else { "" };
                Line::from(vec![
                    Span::styled(format!("{k:>KEY_W$}  "), theme::title(s)),
                    Span::styled(part, fg(s.text)),
                ])
            })
            .collect::<Vec<_>>()
    };
    let cmds = commands::enabled();
    let mut lines: Vec<Line> = wrap(
        "Esc, ? or q closes this screen; j/k PgUp/PgDn scroll.",
        width.max(10),
    )
    .into_iter()
    .map(|l| Line::styled(l, theme::dim(s)))
    .collect();
    let line = |c: &commands::Command| {
        key(
            commands::key_label(c.key),
            format!("{}: {}", c.name, c.description),
        )
    };
    for (title, names) in GROUPS {
        let rows: Vec<_> = cmds.iter().filter(|c| names.contains(&c.name)).collect();
        if !rows.is_empty() {
            lines.push(head(title));
            lines.extend(rows.into_iter().flat_map(line));
        }
    }
    let known = |c: &&commands::Command| GROUPS.iter().any(|(_, n)| n.contains(&c.name));
    let other: Vec<_> = cmds.iter().filter(|c| !known(c)).collect();
    if !other.is_empty() {
        lines.push(head("Other"));
        lines.extend(other.into_iter().flat_map(line));
    }
    lines.push(head("Keys"));
    lines.extend(
        NAV_KEYS
            .iter()
            .flat_map(|(k, d)| key((*k).into(), (*d).into())),
    );
    lines
}

fn draw_help(f: &mut Frame, app: &mut App, area: Rect) {
    let s = app.shell();
    let lines = help_lines(s, area.width.saturating_sub(2) as usize);
    let rows = area.height.saturating_sub(2) as usize;
    let max = lines.len().saturating_sub(rows) as u16;
    app.help_scroll = app.help_scroll.min(max);
    let title = if max > 0 {
        "Help · scroll with j/k"
    } else {
        "Help"
    };
    f.render_widget(
        Paragraph::new(lines)
            .scroll((app.help_scroll, 0))
            .block(theme::panel(title, s.frame)),
        area,
    );
}

fn draw_settings(f: &mut Frame, app: &App, area: Rect) {
    let s = app.shell();
    let inner = area.width.saturating_sub(2) as usize;
    let room = inner.saturating_sub(17);
    let path = app
        .history_path
        .as_ref()
        .map_or_else(|| "(none)".into(), |p| fit(&p.display().to_string(), room));
    let n = app.history.len();
    let clear = if app.confirm_clear {
        "Enter again to confirm".to_owned()
    } else {
        format!("Enter ({n} track{})", if n == 1 { "" } else { "s" })
    };
    let row = |r: Row, label: &str, value: String| {
        let sel = Row::ALL[app.setting_row] == r;
        let line = Line::from(vec![
            Span::styled(if sel { " ▸ " } else { "   " }, fg(s.accent)),
            Span::styled(format!("{label:<14}"), fg(s.text)),
            Span::styled(fit(&value, room), fg(s.accent)),
        ]);
        if sel {
            line.style(theme::selected(s))
        } else {
            line
        }
    };
    let mut lines = vec![
        row(
            Row::Volume,
            "Volume",
            format!("{}%  ←/→ -/+", app.player.volume),
        ),
        row(Row::Shell, "Shell", format!("{}  ←/→", app.shell().name)),
        row(Row::HistoryFile, "History file", path),
    ];
    lines.extend(
        wrap(
            "change: env TERMUSIC_HISTORY or `history_path` in settings.json",
            inner.saturating_sub(3).max(10),
        )
        .into_iter()
        .map(|l| Line::styled(format!("   {l}"), theme::dim(s))),
    );
    lines.push(row(Row::ClearHistory, "Clear history", clear));
    f.render_widget(
        Paragraph::new(lines).block(theme::panel("Settings · j/k move · Esc back", s.frame)),
        area,
    );
}

pub fn offline(f: &mut Frame, s: &Shell, area: Rect) {
    let key = |k: &'static str| Span::styled(k, theme::title(s));
    let lines = vec![
        Line::styled("You are offline", fg(s.accent).bold()),
        Line::raw("Connect to the internet to play YouTube Music."),
        Line::from(vec![
            key("r"),
            Span::raw(" retry   "),
            key("q"),
            Span::raw(" quit"),
        ]),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center)
            .block(theme::panel("Offline", s.accent)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::TPS_L2;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn help_lines_wrap_to_the_width_and_keep_every_word() {
        let flat = |w| {
            help_lines(&TPS_L2, w)
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
        };
        let wide = flat(500).join("\n");
        for c in commands::enabled() {
            assert!(
                wide.contains(&format!("{}: {}", c.name, c.description)),
                "{}",
                c.name
            );
        }
        for w in [39, 60] {
            let lines = flat(w);
            assert!(lines.iter().all(|l| l.width() <= w), "{w}");
            let words = |t: &str| t.split_whitespace().map(String::from).collect::<Vec<_>>();
            assert_eq!(words(&lines.join(" ")), words(&wide));
        }
        assert!(flat(39).len() > flat(500).len());
    }
}
