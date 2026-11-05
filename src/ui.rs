//! Rendering. Reads `App`, never mutates state (except the visible page size).
use crate::app::{App, Screen};
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
    let [logo, body, bar] = Layout::vertical([
        Constraint::Length(6),
        Constraint::Min(3),
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
            let lines = match &app.login_code {
                Some((url, code)) => vec![
                    Line::raw("Open this page on any device:"),
                    Line::styled(url.clone(), fg(theme::CYAN).underlined()),
                    Line::raw("and enter the code:"),
                    Line::styled(code.clone(), fg(theme::YELLOW).bold()),
                ],
                None => vec![
                    Line::styled("Sign in with Google", fg(theme::GREEN).bold()),
                    Line::from(vec![
                        Span::styled("Enter", theme::title()),
                        Span::raw(" start   "),
                        Span::styled("?", theme::title()),
                        Span::raw(" help   "),
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

    draw_bar(f, app, bar);
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
    if app.player.shuffle {
        spans.push(Span::raw(format!(" {}", theme::ICON_SHUFFLE)));
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
                let item = ListItem::new(Line::from(vec![
                    Span::styled(if sel { " ▶ " } else { "   " }, fg(theme::PINK)),
                    Span::styled(format!("{:>2} ", i + 1), theme::dim()),
                    Span::styled(
                        t.title.clone(),
                        fg(if sel { theme::YELLOW } else { theme::CYAN }),
                    ),
                    Span::styled(format!("  {}", t.channel), fg(theme::PURPLE)),
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

fn draw_help(f: &mut Frame, area: Rect) {
    let head = |s| Line::styled(s, fg(theme::YELLOW).bold());
    let key = |k: String, d: String| {
        Line::from(vec![
            Span::styled(format!("{k:>8}  "), theme::title()),
            Span::styled(d, fg(theme::CYAN)),
        ])
    };
    let mut lines = vec![head("Commands"), Line::raw("")];
    for c in commands::enabled() {
        lines.push(key(
            commands::key_label(c.key),
            format!("{}: {}", c.name, c.description),
        ));
    }
    lines.extend([
        Line::raw(""),
        head("Navigation (lists)"),
        key("j / ↓".into(), "move down".into()),
        key("k / ↑".into(), "move up".into()),
        key("PgDn/Up".into(), "move one page".into()),
        key("g / G".into(), "jump to first / last".into()),
        key("P".into(), "your playlists (Enter opens one)".into()),
        key("/".into(), "new search".into()),
        Line::raw(""),
        Line::styled("Esc, ? or q closes this screen.", theme::dim()),
    ]);
    f.render_widget(
        Paragraph::new(lines).block(theme::panel("Help", theme::PINK)),
        area,
    );
}
