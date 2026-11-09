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
    widgets::{List, ListItem, Paragraph, Wrap},
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
                "Search · Enter to run · Esc to quit",
                theme::CYAN,
            )),
            body,
        ),
        Screen::Help => draw_help(f, body),
        Screen::Results => {
            let title = format!("Results ({})", app.results.len());
            let rows = std::mem::take(&mut app.results);
            draw_list(f, app, &rows, &title, theme::PURPLE, body);
            app.results = rows;
        }
        Screen::Playlists => {
            let rows = std::mem::take(&mut app.playlists);
            draw_list(f, app, &rows, "Playlists · Enter opens", theme::GREEN, body);
            app.playlists = rows;
        }
    }

    if show_next {
        draw_next(f, app, next);
    }
    draw_bar(f, app, bar);
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
        Span::styled(text, fg(color)),
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
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::new().bg(theme::BAR_BG)),
        area,
    );
    f.render_widget(
        Paragraph::new(Span::styled("? help ", fg(theme::PINK))).alignment(Alignment::Right),
        area,
    );
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

fn draw_help(f: &mut Frame, area: Rect) {
    let head = |s| Line::styled(s, fg(theme::YELLOW).bold());
    let key = |k: String, d: String| {
        Line::from(vec![
            Span::styled(format!("{k:>8}  "), theme::title()),
            Span::styled(d, fg(theme::CYAN)),
        ])
    };
    let mut lines = vec![head("Commands")];
    for c in commands::enabled() {
        lines.push(key(
            commands::key_label(c.key),
            format!("{}: {}", c.name, c.description),
        ));
    }
    lines.extend([
        Line::raw(""),
        head("Navigation (lists)"),
        key("j/k ↓/↑".into(), "move down / up".into()),
        key("PgDn/Up".into(), "move one page".into()),
        key("g / G".into(), "jump to first / last".into()),
        key("P".into(), "your playlists (Enter opens one)".into()),
        Line::styled("Esc, ? or q closes this screen.", theme::dim()),
    ]);
    f.render_widget(
        Paragraph::new(lines).block(theme::panel("Help", theme::PINK)),
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
        assert!(s.contains("Open the search prompt"));
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
}
