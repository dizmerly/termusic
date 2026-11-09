//! Rendering. Reads `App`, never mutates state (except the visible page size).
use crate::app::{App, Screen};
use crate::queue::Loop;
use crate::theme::{self, fg};
use crate::{commands, youtube::Track};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, List, ListItem, Paragraph, Wrap},
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let show_next = app.screen != Screen::Help
        && f.area().height >= 12
        && app.queue.upcoming().next().is_some();
    let [logo, body, next, bar] = Layout::vertical([
        // Logo only when there's room; Help needs every row.
        Constraint::Length(if app.screen == Screen::Help || f.area().height < 20 {
            0
        } else {
            6
        }),
        Constraint::Min(3),
        Constraint::Length(u16::from(show_next)),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let lines: Vec<Line> = theme::LOGO
        .iter()
        .zip(theme::LOGO_GRADIENT)
        .map(|(l, c)| Line::styled(*l, fg(c).bold()))
        .collect();
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), logo);

    let msg = |f: &mut Frame, lines: Vec<Line>, title, color| {
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Center)
                .block(theme::panel(title, color)),
            body,
        )
    };
    match app.screen {
        Screen::Offline => msg(
            f,
            vec![
                Line::styled("You are offline", fg(theme::ORANGE).bold()),
                Line::raw("Connect to the internet to play YouTube Music."),
                Line::from(vec![
                    Span::styled("r", theme::title()),
                    Span::raw(" retry   "),
                    Span::styled("q", theme::title()),
                    Span::raw(" quit"),
                ]),
            ],
            "Offline",
            theme::ORANGE,
        ),
        Screen::Login => {
            let lines = match &app.login_url {
                Some(url) => vec![
                    Line::raw("Your browser should open. If not, visit:"),
                    Line::styled(url.clone(), fg(theme::CYAN).underlined()),
                    Line::styled("Waiting for you to sign in…", fg(theme::YELLOW)),
                ],
                None => vec![
                    Line::styled(
                        "Sign in with Google to see your playlists",
                        fg(theme::GREEN).bold(),
                    ),
                    Line::from(vec![
                        Span::styled("Enter", theme::title()),
                        Span::raw(" sign in   "),
                        Span::styled("Esc", theme::title()),
                        Span::raw(" back to search   "),
                        Span::styled("q", theme::title()),
                        Span::raw(" quit"),
                    ]),
                ],
            };
            msg(f, lines, "Login", theme::GREEN)
        }
        Screen::Search => f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{} ", theme::ICON_NOTE), fg(theme::PINK)),
                Span::styled(app.input.clone(), fg(theme::YELLOW)),
                Span::styled("▏", fg(theme::PINK)),
            ]))
            .block(theme::panel(
                if app.results.is_empty() {
                    "Search · Enter run · Esc: quit"
                } else {
                    "Search · Enter run · Esc/Tab: back to results"
                },
                theme::CYAN,
            )),
            body,
        ),
        Screen::Help => draw_help(f, app, body),
        Screen::Settings => draw_settings(f, app, body),
        Screen::History => {
            let rows = std::mem::take(&mut app.history);
            let title = "History · Enter replays from here · Esc back";
            draw_list(f, app, &rows, title, theme::YELLOW, body);
            app.history = rows;
        }
        Screen::Results => {
            let title = format!(
                "Results ({}) · / or Esc: search · ? help",
                app.results.len()
            );
            let rows = std::mem::take(&mut app.results);
            draw_list(f, app, &rows, &title, theme::PURPLE, body);
            app.results = rows;
        }
        Screen::Playlists => {
            let rows = std::mem::take(&mut app.playlists);
            draw_list(f, app, &rows, "Playlists · Enter plays", theme::GREEN, body);
            app.playlists = rows;
        }
    }

    if show_next {
        draw_next(f, app, next);
    }
    draw_bar(f, app, bar);
    draw_volume(f, app.player.volume, f.area());
}

/// `filled`/`░` bar of `width` cells for a 0..=1 fraction.
fn bar(frac: f64, width: usize) -> String {
    let n = (frac.clamp(0.0, 1.0) * width as f64).round() as usize;
    "█".repeat(n) + &"░".repeat(width - n)
}

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// `♪ ████████░░ 80%` in the top-right corner, skipped when it would crowd the screen.
fn draw_volume(f: &mut Frame, vol: u8, area: Rect) {
    const W: u16 = 17;
    if area.width < 40 || area.height == 0 {
        return;
    }
    let text = format!(
        " {} {} {vol:>3}%",
        theme::ICON_NOTE,
        bar(f64::from(vol) / 100.0, 10)
    );
    // Inset 2 columns so a panel's corner stays intact when row 0 is its border.
    let r = Rect::new(area.right() - W - 3, area.y, W + 1, 1);
    f.render_widget(Paragraph::new(Span::styled(text, fg(theme::PINK))), r);
}

/// `next: A · B · C` for the next three tracks, cut to the width.
fn draw_next(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<&str> = app
        .queue
        .upcoming()
        .take(3)
        .map(|t| t.title.as_str())
        .collect();
    let text = fit(
        &format!(" next: {}", titles.join(" · ")),
        area.width as usize,
    );
    f.render_widget(Paragraph::new(Span::styled(text, fg(theme::CYAN))), area);
}

fn draw_bar(f: &mut Frame, app: &App, area: Rect) {
    let (icon, color, text) = match (&app.player.now_playing, app.player.playing) {
        (Some(t), true) => (theme::ICON_PLAY, theme::GREEN, t.as_str()),
        (Some(t), false) => (theme::ICON_PAUSE, theme::YELLOW, t.as_str()),
        _ => (theme::ICON_NOTE, theme::PURPLE, "nothing playing"),
    };
    let mut spans = vec![
        Span::styled(format!(" {icon} "), fg(color).bold()),
        Span::styled(text.to_owned(), fg(color)),
    ];
    let loop_icon = match app.queue.looping {
        Loop::Off => None,
        Loop::All => Some(theme::ICON_LOOP),
        Loop::One => Some(theme::ICON_LOOP_ONE),
    };
    for icon in [app.queue.shuffle.then_some(theme::ICON_SHUFFLE), loop_icon]
        .into_iter()
        .flatten()
    {
        spans.push(Span::raw(format!(" {icon}")));
    }
    if app.busy {
        spans.push(Span::styled(
            format!("  {} working…", theme::ICON_BUSY),
            fg(theme::CYAN),
        ));
    }
    if !app.status.is_empty() {
        spans.push(Span::styled(format!("  {}", app.status), fg(theme::ORANGE)));
    }
    // Right side: progress (only with a known length and room) then the help hint.
    let hint = if area.width >= 70 && app.status.is_empty() {
        "S settings · H history · ? help "
    } else {
        "? help "
    };
    let mut right = vec![Span::styled(hint, fg(theme::PINK))];
    if let (Some(d), true) = (app.player.duration, area.width >= 60) {
        let time = format!("{} / {}  ", mmss(app.player.position), mmss(d));
        right.insert(
            0,
            Span::styled(
                format!("{} {time}", bar(app.player.position / d.max(1.0), 10)),
                fg(theme::CYAN),
            ),
        );
    }
    let rw = right.iter().map(Span::width).sum::<usize>() as u16;
    let [left, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(rw)]).areas(area);
    // Truncate the title with … so it never runs into the right side (1 col gap).
    let other: usize = spans.iter().skip(2).map(Span::width).sum::<usize>() + spans[0].width();
    let room = (left.width as usize).saturating_sub(other + 1);
    spans[1] = Span::styled(fit(text, room), fg(color));
    f.render_widget(Block::new().style(Style::new().bg(theme::BAR_BG)), area);
    f.render_widget(Paragraph::new(Line::from(spans)), left);
    f.render_widget(Paragraph::new(Line::from(right)), right_area);
}

fn draw_list(
    f: &mut Frame,
    app: &mut App,
    rows: &[Track],
    title: &str,
    color: ratatui::style::Color,
    area: Rect,
) {
    let block = theme::panel(title, color);
    let inner = block.inner(area);
    app.page = inner.height as usize;
    let items: Vec<ListItem> = if rows.is_empty() {
        vec![ListItem::new(Span::styled(
            "  nothing here yet",
            theme::dim(),
        ))]
    } else {
        rows.iter()
            .enumerate()
            .skip(app.offset)
            .take(app.page)
            .map(|(i, t)| {
                let sel = i == app.selected;
                let channel = fit(&t.channel, 24);
                // 3 marker + 3 index + 2 gap columns; the channel always stays visible.
                let room = (inner.width as usize).saturating_sub(8 + channel.chars().count());
                let marker = match (sel, app.player.now_playing.as_deref() == Some(&t.title)) {
                    (true, _) => " ▶ ",
                    (_, true) => " ♪ ",
                    _ => "   ",
                };
                let item = ListItem::new(Line::from(vec![
                    Span::styled(marker, fg(theme::PINK)),
                    Span::styled(format!("{:>2} ", i + 1), theme::dim()),
                    Span::styled(
                        fit(&t.title, room),
                        fg(if sel { theme::YELLOW } else { theme::CYAN }),
                    ),
                    Span::styled(format!("  {channel}"), fg(theme::PURPLE)),
                ]));
                if sel {
                    item.style(theme::selected())
                } else {
                    item
                }
            })
            .collect()
    };
    f.render_widget(block, area);
    f.render_widget(List::new(items), inner);
}

/// Truncate to `n` chars with an ellipsis.
fn fit(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.into();
    }
    s.chars().take(n.saturating_sub(1)).chain(['…']).collect()
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
const NAV_KEYS: [(&str, &str); 10] = [
    ("j/k ↓/↑", "move down / up in a list (scroll in Help)"),
    ("PgDn/Up", "move one page in a list"),
    ("g / G", "jump to the first / last row"),
    ("Enter", "run the search, or play the highlighted row"),
    (
        "Esc",
        "go back: Search to results, overlays to prior screen",
    ),
    ("Tab", "on Search: back to the results list"),
    ("F1", "help (same as ?)"),
    ("F2", "playlists, also while typing a search"),
    ("F3", "settings (same as S), also while typing a search"),
    ("F4", "history (same as H), also while typing a search"),
];

fn help_lines() -> Vec<Line<'static>> {
    let head = |s: &str| Line::styled(s.to_owned(), fg(theme::YELLOW).bold());
    let key = |k: String, d: String| {
        Line::from(vec![
            Span::styled(format!("{k:>8}  "), theme::title()),
            Span::styled(d, fg(theme::CYAN)),
        ])
    };
    let cmds = commands::enabled();
    let mut lines = vec![Line::styled(
        "Esc, ? or q closes this screen; j/k PgUp/PgDn scroll.",
        theme::dim(),
    )];
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
            lines.extend(rows.into_iter().map(line));
        }
    }
    let known = |c: &&commands::Command| GROUPS.iter().any(|(_, n)| n.contains(&c.name));
    let other: Vec<_> = cmds.iter().filter(|c| !known(c)).collect();
    if !other.is_empty() {
        lines.push(head("Other"));
        lines.extend(other.into_iter().map(line));
    }
    lines.push(head("Keys"));
    lines.extend(NAV_KEYS.map(|(k, d)| key(k.into(), d.into())));
    lines
}

fn draw_help(f: &mut Frame, app: &mut App, area: Rect) {
    let lines = help_lines();
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
            .block(theme::panel(title, theme::PINK)),
        area,
    );
}

fn draw_settings(f: &mut Frame, app: &App, area: Rect) {
    let path = app
        .settings
        .history_file()
        .map_or_else(|| "(none)".into(), |p| p.display().to_string());
    let row = |i: usize, label: &str, value: String| {
        let sel = app.setting_row == i;
        let line = Line::from(vec![
            Span::styled(if sel { " ▶ " } else { "   " }, fg(theme::PINK)),
            Span::styled(format!("{label:<14}"), fg(theme::YELLOW)),
            Span::styled(value, fg(theme::CYAN)),
        ]);
        if sel {
            line.style(theme::selected())
        } else {
            line
        }
    };
    let lines = vec![
        row(
            0,
            "Volume",
            format!("{}%  (←/→ or -/+ changes it by 5)", app.player.volume),
        ),
        row(1, "History file", path),
        Line::styled(
            "                change: env TERMUSIC_HISTORY or `history_path` in settings.json",
            theme::dim(),
        ),
        row(
            2,
            "Clear history",
            format!("Enter ({} tracks)", app.history.len()),
        ),
    ];
    f.render_widget(
        Paragraph::new(lines).block(theme::panel("Settings · j/k move · Esc back", theme::GREEN)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, app)).unwrap();
        t.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn help_fits_80x24() {
        let mut a = App::new();
        a.screen = Screen::Help;
        let s = render(&mut a, 80, 24);
        assert!(s.contains("closes this screen"), "help clipped");
        assert!(s.contains("Playback"));
    }

    #[test]
    fn help_lists_every_command_and_key_when_scrolled() {
        let mut a = App::new();
        a.screen = Screen::Help;
        let mut all = String::new();
        for scroll in [0, 8, 16, 24, 32] {
            a.help_scroll = scroll;
            all += &render(&mut a, 80, 24);
        }
        for c in commands::enabled() {
            assert!(all.contains(&format!("{}: ", c.name)), "{}", c.name);
            assert!(all.contains(c.description), "{}", c.name);
        }
        for g in ["Playback", "Queue", "Navigation", "Screens"] {
            assert!(all.contains(g), "{g}");
        }
        for k in [
            "Esc", "Tab", "F1", "F2", "F3", "F4", "PgDn/Up", "g / G", "Enter",
        ] {
            assert!(all.contains(k), "{k}");
        }
        assert!(a.help_scroll < 32, "scroll is clamped");
    }

    #[test]
    fn search_and_results_hints() {
        let mut a = App::new();
        a.screen = Screen::Search;
        assert!(render(&mut a, 80, 24).contains("Esc: quit"));
        a.results = vec![Track {
            title: "t".into(),
            channel: String::new(),
            id: "i".into(),
        }];
        assert!(render(&mut a, 80, 24).contains("Esc/Tab: back to results"));
        a.screen = Screen::Results;
        let s = render(&mut a, 80, 24);
        assert!(s.contains("/ or Esc: search") && s.contains("S settings · H history"));
    }

    #[test]
    fn settings_and_history_render() {
        let mut a = App::new();
        a.screen = Screen::Settings;
        let s = render(&mut a, 80, 24);
        for w in [
            "Volume",
            "History file",
            "TERMUSIC_HISTORY",
            "Clear history",
        ] {
            assert!(s.contains(w), "{w}");
        }
        a.screen = Screen::History;
        assert!(render(&mut a, 80, 24).contains("nothing here yet"));
        a.history = vec![Track {
            title: "Old song".into(),
            channel: "Chan".into(),
            id: "o".into(),
        }];
        let s = render(&mut a, 80, 24);
        assert!(s.contains("Old song") && s.contains("Chan"));
    }

    #[test]
    fn bars_and_time() {
        assert_eq!(bar(0.5, 10), "█████░░░░░");
        assert_eq!(bar(2.0, 4), "████");
        assert_eq!(bar(-1.0, 4), "░░░░");
        assert_eq!(mmss(75.9), "01:15");
        assert_eq!(mmss(-3.0), "00:00");
    }

    #[test]
    fn volume_keeps_panel_corner_and_title_clear_of_progress() {
        let mut a = App::new();
        a.screen = Screen::Help;
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| draw(f, &mut a)).unwrap();
        assert_eq!(t.backend().buffer()[(79, 0)].symbol(), "╮");
        assert!(render(&mut a, 80, 24).contains("70%"));
        a.player.now_playing = Some("T".repeat(120));
        a.player.duration = Some(100.0);
        let s = render(&mut a, 80, 24);
        assert!(s.contains("…") && s.contains("00:00 / 01:40"));
    }

    #[test]
    fn volume_and_progress_render() {
        let mut a = App::new();
        a.player.volume = 80;
        a.player.position = 65.0;
        a.player.duration = Some(185.0);
        let s = render(&mut a, 80, 24);
        assert!(s.contains("♪ ████████░░  80%"), "volume");
        assert!(s.contains("01:05 / 03:05") && s.contains("? help"));
        let s = render(&mut a, 50, 24); // narrow: no progress
        assert!(!s.contains("01:05") && s.contains("80%"));
        let s = render(&mut a, 30, 24); // tiny: no volume, no panic
        assert!(!s.contains("80%"));
        render(&mut a, 40, 1);
    }

    #[test]
    fn bar_shows_shuffle_and_loop_mode() {
        let mut a = App::new();
        assert!(!render(&mut a, 80, 24).contains(theme::ICON_LOOP));
        a.queue.toggle_shuffle();
        a.queue.looping = Loop::One;
        let s = render(&mut a, 80, 24);
        assert!(s.contains(theme::ICON_SHUFFLE));
        assert!(s.contains(theme::ICON_LOOP_ONE));
    }

    #[test]
    fn up_next_line_shows_three_and_hides_on_help_or_short() {
        let mut a = App::new();
        a.screen = Screen::Results;
        assert!(!render(&mut a, 80, 24).contains("next:"));
        for n in ["one", "two", "three", "four"] {
            a.queue.add(Track {
                title: n.into(),
                channel: String::new(),
                id: n.into(),
            });
        }
        assert!(render(&mut a, 80, 24).contains("next: one · two · three"));
        assert!(!render(&mut a, 80, 24).contains("four"));
        assert!(!render(&mut a, 80, 11).contains("next:"));
        render(&mut a, 10, 12); // narrow must not panic
        a.screen = Screen::Help;
        assert!(!render(&mut a, 80, 24).contains("next:"));
    }

    #[test]
    fn long_titles_keep_channel_visible() {
        let mut a = App::new();
        a.screen = Screen::Results;
        a.results = vec![Track {
            title: "x".repeat(200),
            channel: "Artist".into(),
            id: "i".into(),
        }];
        assert!(render(&mut a, 80, 24).contains("Artist"));
        render(&mut a, 20, 5); // tiny terminal must not panic
    }

    #[test]
    fn login_hint_fully_visible_on_search() {
        let mut a = App::new();
        a.screen = Screen::Search;
        a.status = crate::app::LOGIN_HINT.into();
        let s = render(&mut a, 80, 24);
        assert!(s.contains(crate::app::LOGIN_HINT) && s.contains("? help"));
    }
}
