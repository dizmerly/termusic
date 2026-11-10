//! Rendering. Reads `App`, never mutates state (except the visible page size).
use crate::app::{App, Row, Screen};
use crate::history::Entry;
use crate::queue::Loop;
use crate::theme::{self, Shell, fg};
use crate::{commands, youtube::Track};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, List, ListItem, Paragraph, Wrap},
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let s = app.shell();
    let show_next = app.screen != Screen::Help
        && app.screen != Screen::Queue
        && f.area().height >= 12
        && app.queue.upcoming().next().is_some();
    let [logo, body, next, bar] = Layout::vertical([
        // Logo only when there's room; Help needs every row.
        Constraint::Length(if app.screen == Screen::Help || f.area().height < 20 {
            0
        } else {
            8
        }),
        Constraint::Min(3),
        Constraint::Length(u16::from(show_next)),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let gradient = [
        s.frame, s.frame, s.accent, s.label, s.meter_lo, s.meter_lo, s.meter_lo, s.dim,
    ];
    let lines: Vec<Line> = theme::LOGO
        .iter()
        .zip(gradient)
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
                Line::styled("You are offline", fg(s.accent).bold()),
                Line::raw("Connect to the internet to play YouTube Music."),
                Line::from(vec![
                    Span::styled("r", theme::title(s)),
                    Span::raw(" retry   "),
                    Span::styled("q", theme::title(s)),
                    Span::raw(" quit"),
                ]),
            ],
            "Offline",
            s.accent,
        ),
        _ if app.searching => {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("{} ", theme::ICON_NOTE), fg(s.frame)),
                    Span::styled(app.input.clone(), fg(s.label)),
                    Span::styled("▏", fg(s.frame)),
                ]))
                .block(theme::panel(
                    "Search · Enter run · Esc/Tab: back to the list",
                    s.meter_lo,
                )),
                body,
            );
            // Below the input row, inside the panel border.
            let art = Rect::new(
                body.x + 1,
                body.y + 2,
                body.width.saturating_sub(2),
                body.height.saturating_sub(3),
            );
            draw_cassette(f, app, art);
        }
        Screen::Login => {
            let lines = match &app.login_url {
                Some(url) => vec![
                    Line::raw("Your browser should open. If not, visit:"),
                    Line::styled(url.clone(), fg(s.meter_lo).underlined()),
                    Line::styled("Waiting for you to sign in…", fg(s.label)),
                ],
                None => vec![
                    Line::styled(
                        "Sign in with Google to see your playlists",
                        fg(s.meter_lo).bold(),
                    ),
                    Line::from(vec![
                        Span::styled("Enter", theme::title(s)),
                        Span::raw(" sign in   "),
                        Span::styled("Esc", theme::title(s)),
                        Span::raw(" back to search   "),
                        Span::styled("q", theme::title(s)),
                        Span::raw(" quit"),
                    ]),
                ],
            };
            msg(f, lines, "Login", s.meter_lo)
        }
        Screen::Help => draw_help(f, app, body),
        Screen::Settings => draw_settings(f, app, body),
        Screen::History => {
            let rows: Vec<Track> = app.history.iter().map(history_row).collect();
            let title = "History · Enter replays from where you stopped · Esc back";
            draw_list(f, app, &rows, title, s.label, body);
        }
        Screen::Results => {
            let title = format!(
                "Results ({}) · / or Esc: search · ? help",
                app.results.len()
            );
            let rows = std::mem::take(&mut app.results);
            let body = cassette_above(f, app, body);
            draw_list(f, app, &rows, &title, s.dim, body);
            app.results = rows;
        }
        Screen::Queue => {
            let rows: Vec<Track> = app
                .queue
                .current()
                .into_iter()
                .chain(app.queue.upcoming())
                .cloned()
                .collect();
            draw_list(f, app, &rows, "Queue · Enter jumps to it", s.meter_lo, body);
        }
        Screen::Playlists => {
            let rows = std::mem::take(&mut app.playlists);
            draw_list(f, app, &rows, "Playlists · Enter plays", s.meter_lo, body);
            app.playlists = rows;
        }
    }

    if show_next {
        draw_next(f, app, next);
    }
    draw_bar(f, app, bar);
    draw_volume(f, s, app.player.volume, f.area());
}

/// Cassette art with spinning reels; `spin` picks the frame (0 when idle) and
/// `now` is the now-playing text shown on the label.
fn cassette_lines(s: &Shell, spin: usize, now: &str) -> Vec<Line<'static>> {
    let reel = theme::REEL_FRAMES[spin % theme::REEL_FRAMES.len()].to_string();
    theme::CASSETTE
        .iter()
        .map(|l| {
            let field = l.matches('#').count();
            let l = if field > 0 {
                let t = if now.is_empty() {
                    String::new()
                } else {
                    fit(now, field)
                };
                l.replace(&"#".repeat(field), &format!("{t:^field$}"))
            } else {
                (*l).to_owned()
            };
            let mut spans = Vec::new();
            for (i, part) in l.split('@').enumerate() {
                if i > 0 {
                    spans.push(Span::styled(reel.clone(), fg(s.label).bold()));
                }
                spans.push(Span::styled(part.to_owned(), fg(s.frame)));
            }
            Line::from(spans)
        })
        .collect()
}

/// Draws the cassette centered in `area` if it fits; reels spin only while playing.
fn draw_cassette(f: &mut Frame, app: &App, area: Rect) {
    let spin = if app.player.playing {
        app.player.position as usize
    } else {
        0
    };
    // Same text as the status bar.
    let now = match (&app.player.now_playing, app.player.playing) {
        (Some(t), true) => format!("{} {t}", theme::ICON_PLAY),
        (Some(t), false) => format!("{} {t}", theme::ICON_PAUSE),
        _ => "nothing playing".into(),
    };
    let lines = cassette_lines(app.shell(), spin, &now);
    if area.height >= lines.len() as u16 && area.width >= lines[0].width() as u16 {
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
    }
}

/// Reserves a cassette strip at the top of `body` when there's room; returns the rest.
fn cassette_above(f: &mut Frame, app: &App, body: Rect) -> Rect {
    let h = theme::CASSETTE.len() as u16;
    if body.height < h + 6 || body.width < theme::CASSETTE[0].len() as u16 {
        return body;
    }
    let [top, rest] = Layout::vertical([Constraint::Length(h), Constraint::Min(3)]).areas(body);
    draw_cassette(f, app, top);
    rest
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
fn draw_volume(f: &mut Frame, s: &Shell, vol: u8, area: Rect) {
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
    f.render_widget(Paragraph::new(Span::styled(text, fg(s.frame))), r);
}

/// `next: A · B · C` for the next three tracks, cut to the width.
fn draw_next(f: &mut Frame, app: &App, area: Rect) {
    let s = app.shell();
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
    f.render_widget(Paragraph::new(Span::styled(text, fg(s.meter_lo))), area);
}

fn draw_bar(f: &mut Frame, app: &App, area: Rect) {
    let s = app.shell();
    let (icon, color, text) = match (&app.player.now_playing, app.player.playing) {
        (Some(t), true) => (theme::ICON_PLAY, s.meter_lo, t.as_str()),
        (Some(t), false) => (theme::ICON_PAUSE, s.label, t.as_str()),
        _ => (theme::ICON_NOTE, s.dim, "nothing playing"),
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
            fg(s.meter_lo),
        ));
    }
    if !app.status.is_empty() {
        spans.push(Span::styled(format!("  {}", app.status), fg(s.accent)));
    }
    // Right side: progress (only with a known length and room) then the help hint.
    // On Search the letters are text, so it shows the F-keys.
    let search = app.searching;
    let hint = match (area.width >= 70 && app.status.is_empty(), search) {
        (true, true) => "F3 settings · F4 history · F1 help ",
        (true, false) => "S settings · H history · ? help ",
        (false, true) => "F1 help ",
        (false, false) => "? help ",
    };
    let mut right = vec![Span::styled(hint, fg(s.frame))];
    if let (Some(d), true) = (app.player.duration, area.width >= 60) {
        let time = format!("{} / {}  ", mmss(app.player.position), mmss(d));
        right.insert(
            0,
            Span::styled(
                format!("{} {time}", bar(app.player.position / d.max(1.0), 10)),
                fg(s.meter_lo),
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
    f.render_widget(Block::new().style(Style::new().bg(s.body)), area);
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
    let s = app.shell();
    let block = theme::panel(title, color);
    let inner = block.inner(area);
    app.page = inner.height as usize;
    let items: Vec<ListItem> = if rows.is_empty() {
        vec![ListItem::new(Span::styled(
            "  nothing here yet",
            theme::dim(s),
        ))]
    } else {
        rows.iter()
            .enumerate()
            .skip(app.offset)
            .take(app.page)
            .map(|(i, t)| {
                let sel = i == app.selected;
                let channel = fit(&t.channel, 28);
                // 3 marker + 3 index + 2 gap columns; the channel always stays visible.
                let room = (inner.width as usize).saturating_sub(8 + channel.chars().count());
                let marker = match (sel, app.player.now_playing.as_deref() == Some(&t.title)) {
                    (true, _) => " ▶ ",
                    (_, true) => " ♪ ",
                    _ => "   ",
                };
                let item = ListItem::new(Line::from(vec![
                    Span::styled(marker, fg(s.frame)),
                    Span::styled(format!("{:>2} ", i + 1), theme::dim(s)),
                    Span::styled(
                        fit(&t.title, room),
                        fg(if sel { s.label } else { s.meter_lo }),
                    ),
                    Span::styled(format!("  {channel}"), fg(s.dim)),
                ]));
                if sel {
                    item.style(theme::selected(s))
                } else {
                    item
                }
            })
            .collect()
    };
    f.render_widget(block, area);
    f.render_widget(List::new(items), inner);
}

/// History entry as a list row: the channel column shows the short link and resume time.
fn history_row(e: &Entry) -> Track {
    Track {
        channel: format!("youtu.be/{} {}", e.track.id, mmss(e.position_secs as f64)),
        ..e.track.clone()
    }
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
const NAV_KEYS: [(&str, &str); 12] = [
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
    ("F1", "help (same as ?)"),
    ("F2", "Lists tab (same as P), also while typing a search"),
    ("F3", "settings (same as S), also while typing a search"),
    ("F4", "History tab (same as H), also while typing a search"),
];

fn help_lines(s: &Shell) -> Vec<Line<'static>> {
    let head = |t: &str| Line::styled(t.to_owned(), fg(s.label).bold());
    let key = |k: String, d: String| {
        Line::from(vec![
            Span::styled(format!("{k:>9}  "), theme::title(s)),
            Span::styled(d, fg(s.meter_lo)),
        ])
    };
    let cmds = commands::enabled();
    let mut lines = vec![Line::styled(
        "Esc, ? or q closes this screen; j/k PgUp/PgDn scroll.",
        theme::dim(s),
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
    let s = app.shell();
    let lines = help_lines(app.shell());
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
    let room = (area.width as usize).saturating_sub(24);
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
            Span::styled(if sel { " ▶ " } else { "   " }, fg(s.frame)),
            Span::styled(format!("{label:<14}"), fg(s.label)),
            Span::styled(value, fg(s.meter_lo)),
        ]);
        if sel {
            line.style(theme::selected(s))
        } else {
            line
        }
    };
    let lines = vec![
        row(
            Row::Volume,
            "Volume",
            format!("{}%  (←/→ or -/+ changes it by 5)", app.player.volume),
        ),
        row(
            Row::Shell,
            "Shell",
            format!("{}  (←/→ changes it)", app.shell().name),
        ),
        row(Row::HistoryFile, "History file", path),
        Line::styled(
            "                change: env TERMUSIC_HISTORY or `history_path` in settings.json",
            theme::dim(s),
        ),
        row(Row::ClearHistory, "Clear history", clear),
    ];
    f.render_widget(
        Paragraph::new(lines).block(theme::panel("Settings · j/k move · Esc back", s.meter_lo)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    /// A blank app with focus on the list (startup focuses the search field).
    fn blank() -> App {
        let mut a = App::blank();
        a.searching = false;
        a
    }

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
    fn cassette_shows_when_roomy_hides_when_small_and_spins() {
        use unicode_width::UnicodeWidthStr;
        let text = |l: &[Line]| l.iter().map(|l| l.to_string()).collect::<Vec<_>>();
        let (a, b) = (
            text(&cassette_lines(&theme::SUNSET, 0, "x")),
            text(&cassette_lines(&theme::SUNSET, 1, "x")),
        );
        assert_ne!(a, b);
        assert_eq!(
            a,
            text(&cassette_lines(
                &theme::SUNSET,
                theme::REEL_FRAMES.len(),
                "x"
            ))
        );
        assert!(a.iter().all(|l| !l.contains('@') && !l.contains('#')));
        assert!(a.iter().any(|l| l.contains(" x ")));
        let w = a[0].width();
        assert!(a.iter().all(|l| l.width() == w));

        let mut app = blank();
        (app.screen, app.searching) = (Screen::Results, true);
        assert!(render(&mut app, 80, 30).contains("[_____]"));
        for (w, h) in [(80, 8), (20, 30), (1, 1), (80, 3)] {
            assert!(!render(&mut app, w, h).contains("[_____]"), "{w}x{h}");
        }
        app.searching = false;
        assert!(render(&mut app, 80, 40).contains("[_____]"));
        assert!(!render(&mut app, 80, 20).contains("[_____]"));
    }

    #[test]
    fn help_fits_80x24() {
        let mut a = blank();
        a.screen = Screen::Help;
        let s = render(&mut a, 80, 24);
        assert!(s.contains("closes this screen"), "help clipped");
        for want in [
            "Playback",
            "play: Play the highlighted track",
            "pause: Pause or resume",
        ] {
            assert!(s.contains(want), "{want}");
        }
        assert!(!s.contains("Keys")); // the rest scrolls
    }

    #[test]
    fn help_lists_every_command_and_key_when_scrolled() {
        let mut a = blank();
        a.screen = Screen::Help;
        let mut all = String::new();
        for scroll in [0, 8, 16, 24, u16::MAX] {
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
            "Esc",
            "Tab",
            "Shift-Tab",
            "1-4",
            "F1",
            "F2",
            "F3",
            "F4",
            "PgDn/Up",
            "g / G",
            "Enter",
        ] {
            assert!(all.contains(k), "{k}");
        }
        assert!(a.help_scroll < 32, "scroll is clamped");
    }

    #[test]
    fn search_and_results_hints() {
        let mut a = blank();
        (a.screen, a.searching) = (Screen::Results, true);
        assert!(render(&mut a, 80, 24).contains("Esc/Tab: back to the list"));
        a.searching = false;
        let s = render(&mut a, 80, 24);
        assert!(s.contains("/ or Esc: search") && s.contains("S settings · H history"));
        a.searching = true; // letters are text there: the hint names the F-keys
        assert!(render(&mut a, 80, 24).contains("F3 settings · F4 history"));
    }

    #[test]
    fn settings_and_history_render() {
        let mut a = blank();
        a.screen = Screen::Settings;
        let s = render(&mut a, 80, 24);
        for w in [
            "Volume",
            "Shell",
            "History file",
            "TERMUSIC_HISTORY",
            "Clear history",
        ] {
            assert!(s.contains(w), "{w}");
        }
        a.screen = Screen::History;
        assert!(render(&mut a, 80, 24).contains("nothing here yet"));
        crate::history::push(
            &mut a.history,
            Track {
                title: "Old song".into(),
                channel: "Chan".into(),
                id: "o".into(),
            },
        );
        let s = render(&mut a, 80, 24);
        assert!(s.contains("Old song") && s.contains("youtu.be/"));
        a.screen = Screen::Settings;
        a.history_path = Some("/very/long/".repeat(20).into());
        let s = render(&mut a, 80, 24);
        assert!(s.contains("1 track)") && s.contains("…"));
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
        let mut a = blank();
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
        let mut a = blank();
        a.player.volume = 80;
        a.player.position = 65.0;
        a.player.duration = Some(185.0);
        let s = render(&mut a, 80, 24);
        assert!(s.contains("♪ ████████░░  80%"), "volume");
        assert!(s.contains("01:05 / 03:05") && s.contains("help"));
        let s = render(&mut a, 50, 24); // narrow: no progress
        assert!(!s.contains("01:05") && s.contains("80%"));
        let s = render(&mut a, 30, 24); // tiny: no volume, no panic
        assert!(!s.contains("80%"));
        render(&mut a, 40, 1);
    }

    #[test]
    fn bar_shows_shuffle_and_loop_mode() {
        let mut a = blank();
        assert!(!render(&mut a, 80, 24).contains(theme::ICON_LOOP));
        a.queue.toggle_shuffle();
        a.queue.looping = Loop::One;
        let s = render(&mut a, 80, 24);
        assert!(s.contains(theme::ICON_SHUFFLE));
        assert!(s.contains(theme::ICON_LOOP_ONE));
    }

    #[test]
    fn up_next_line_shows_three_and_hides_on_help_or_short() {
        let mut a = blank();
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
        let mut a = blank();
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
        let mut a = blank();
        (a.screen, a.searching) = (Screen::Results, true);
        a.status = "queued: a fairly long title for the bar".into();
        let s = render(&mut a, 80, 24);
        assert!(s.contains(&a.status) && s.contains("F1 help"));
    }

    #[test]
    fn history_rows_show_link_and_resume_time() {
        let e = Entry {
            track: Track {
                title: "Old".into(),
                channel: "Chan".into(),
                id: "abc123".into(),
            },
            position_secs: 83,
        };
        assert_eq!(history_row(&e).channel, "youtu.be/abc123 01:23");
        let mut app = blank();
        app.screen = Screen::History;
        app.history = vec![e];
        let out = render(&mut app, 80, 24);
        assert!(out.contains("Old") && out.contains("youtu.be/abc123 01:23"));
        render(&mut app, 30, 6);
    }

    #[test]
    fn fit_truncates_with_ellipsis() {
        assert_eq!(fit("abc", 3), "abc");
        assert_eq!(fit("abcd", 3), "ab…");
        assert_eq!(fit("世界世界", 3), "世界…");
        assert_eq!(fit("abc", 0), "…");
        assert_eq!(fit("", 0), "");
    }

    #[test]
    fn list_draws_rows_and_never_panics_when_tiny() {
        let mut app = blank();
        app.screen = Screen::Results;
        app.results = (0..5)
            .map(|i| Track {
                title: format!("song{i}"),
                channel: "chan".into(),
                id: i.to_string(),
            })
            .collect();
        let out = render(&mut app, 80, 24);
        assert!(out.contains("song0") && out.contains("song4"));
        for (w, h) in [(1, 1), (3, 3), (10, 2), (80, 4)] {
            render(&mut app, w, h);
        }
    }
}
