//! The J-card pane: tab strip, search field and the active tab's list.
use super::{clean, fit, history_row, overlay, wrap};
use crate::app::{App, Screen};
use crate::theme::{self, Shell, fg};
use crate::youtube::Track;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};
use unicode_width::UnicodeWidthStr;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let s = app.shell();
    match app.screen {
        Screen::Offline => return overlay::offline(f, s, area),
        Screen::Help | Screen::Settings => return overlay::draw(f, app, area),
        _ => {}
    }
    let [tabs, search, rule, list] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(area);
    let w = area.width as usize;
    let strip = tab_strip(s, app.tab().tab_index(), w);
    f.render_widget(Paragraph::new(strip), tabs);
    f.render_widget(Paragraph::new(search_line(s, app, w)), search);
    f.render_widget(
        Paragraph::new(Line::styled("─".repeat(w), fg(s.frame))),
        rule,
    );
    match app.tab() {
        Screen::Login => login(f, app, list),
        Screen::Queue => {
            let rows: Vec<Track> = app
                .queue
                .current()
                .into_iter()
                .chain(app.queue.upcoming())
                .cloned()
                .collect();
            let empty = "Nothing queued. Enter on a result plays it.";
            draw_list(f, app, &rows, w / 3, empty, list);
        }
        Screen::Playlists => {
            let rows = std::mem::take(&mut app.playlists);
            draw_list(f, app, &rows, w / 3, "No playlists yet", list);
            app.playlists = rows;
        }
        Screen::History => {
            let rows: Vec<Track> = app.history.iter().map(history_row).collect();
            draw_list(f, app, &rows, w / 3, "Nothing played yet", list);
        }
        _ => {
            let rows = std::mem::take(&mut app.results);
            draw_list(f, app, &rows, w / 3, "Type / and search for a song", list);
            app.results = rows;
        }
    }
}

/// ` 1 RESULTS  2 QUEUE  3 LISTS  4 HISTORY `, spread over `w` columns; the active tab is
/// reversed. Labels shrink (short names, then digits) until they fit.
pub fn tab_strip(s: &Shell, active: Option<usize>, w: usize) -> Line<'static> {
    const NAMES: [(&str, &str); 4] = [
        ("RESULTS", "RES"),
        ("QUEUE", "QUE"),
        ("LISTS", "LST"),
        ("HISTORY", "HIS"),
    ];
    let options: [Vec<String>; 3] = [
        (0..4)
            .map(|i| format!("{} {}", i + 1, NAMES[i].0))
            .collect(),
        (0..4)
            .map(|i| format!("{} {}", i + 1, NAMES[i].1))
            .collect(),
        (0..4).map(|i| (i + 1).to_string()).collect(),
    ];
    let total = |l: &[String]| l.iter().map(|x| x.width()).sum::<usize>();
    let mut labels = options
        .iter()
        .find(|l| total(l) + 3 <= w)
        .unwrap_or(&options[2])
        .clone();
    if total(&labels) + 8 + 3 <= w {
        labels = labels.iter().map(|l| format!(" {l} ")).collect(); // room for key-like tabs
    }
    let free = w.saturating_sub(total(&labels));
    let edge = usize::from(free >= 5); // a margin on each side while the gaps keep a column
    let gap = free.saturating_sub(2 * edge) / 3;
    let on = Style::new()
        .bg(s.accent)
        .fg(s.body)
        .add_modifier(Modifier::BOLD);
    let mut spans = vec![Span::raw(" ".repeat(edge))];
    for (i, l) in labels.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" ".repeat(gap)));
        }
        let style = if active == Some(i) { on } else { fg(s.dim) };
        spans.push(Span::styled(l, style));
    }
    spans.push(Span::raw(" ".repeat(free.saturating_sub(edge + 3 * gap))));
    Line::from(spans)
}

/// `♪ query▏` while typing; the last query, or a hint, otherwise. `N found` sits at the
/// right end when there are results and room; the query always wins.
fn search_line(s: &Shell, app: &App, w: usize) -> Line<'static> {
    let mut line = query_line(s, app, w);
    let found = format!("{} found", app.results.len());
    let used = line.width() + 1; // keep a right margin
    if !app.results.is_empty() && used + 1 + found.width() <= w {
        line.spans
            .push(Span::raw(" ".repeat(w - used - found.width())));
        line.spans.push(Span::styled(found, fg(s.dim)));
    }
    line
}

fn query_line(s: &Shell, app: &App, w: usize) -> Line<'static> {
    let input = clean(&app.input);
    let room = w.saturating_sub(4);
    if app.searching {
        // Show the end of a long query: that is where the typing happens.
        let mut tail: Vec<char> = Vec::new();
        let mut used = 0;
        for c in input.chars().rev() {
            used += unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if used > room {
                break;
            }
            tail.push(c);
        }
        let tail: String = tail.into_iter().rev().collect();
        return Line::from(vec![
            Span::styled(format!(" {} ", theme::ICON_NOTE), fg(s.accent)),
            Span::styled(tail, fg(s.text)),
            Span::styled("▏", fg(s.accent)),
        ]);
    }
    if input.is_empty() {
        return Line::styled(format!(" {} / to search", theme::ICON_NOTE), fg(s.dim));
    }
    Line::from(vec![
        Span::styled(format!(" {} ", theme::ICON_NOTE), fg(s.dim)),
        Span::styled(fit(&input, room), fg(s.text)),
    ])
}

/// The signed-out Lists tab.
fn login(f: &mut Frame, app: &App, area: Rect) {
    let s = app.shell();
    let key = |k: &'static str| Span::styled(k, theme::title(s));
    let lines = match &app.login_url {
        Some(url) => vec![
            Line::raw("Your browser should open. If not, visit:"),
            Line::styled(url.clone(), fg(s.meter_lo).underlined()),
            Line::styled("Waiting for you to sign in…", fg(s.accent)),
        ],
        None => vec![
            Line::styled(
                "Sign in with Google to see your playlists",
                fg(s.text).bold(),
            ),
            Line::raw(""),
            Line::from(vec![
                key("Enter"),
                Span::raw(" sign in   "),
                key("/"),
                Span::raw(" search   "),
                key("q"),
                Span::raw(" quit"),
            ]),
        ],
    };
    centered(f, lines, area);
}

/// Lines centered in `area`, a third of the way down, wrapped to its width.
fn centered(f: &mut Frame, lines: Vec<Line<'static>>, area: Rect) {
    let top = area.height / 3;
    let r = Rect::new(
        area.x,
        area.y + top,
        area.width,
        area.height.saturating_sub(top),
    );
    f.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(ratatui::widgets::Wrap { trim: true }),
        r,
    );
}

/// List rows: marker, two-digit index, title, and the right column (at most `cap` columns)
/// right-aligned. Sets `app.page` to the visible rows.
fn draw_list(f: &mut Frame, app: &mut App, rows: &[Track], cap: usize, empty: &str, area: Rect) {
    let s = app.shell();
    app.page = area.height as usize;
    if rows.is_empty() {
        let lines = wrap(empty, (area.width as usize).saturating_sub(2))
            .into_iter()
            .map(|l| Line::styled(l, fg(s.dim)))
            .collect();
        return centered(f, lines, area);
    }
    let w = area.width as usize;
    let playing = app
        .player
        .now_playing
        .as_ref()
        .and(app.queue.current())
        .map(|t| t.id.as_str());
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .skip(app.offset)
        .take(app.page)
        .map(|(i, t)| {
            let idx = format!("{:02}", i + 1);
            let right = fit(&t.channel, cap);
            // marker + index + space + title + 2 gap + right + 1 margin
            let room = w.saturating_sub(idx.width() + right.width() + 5);
            let title = fit(&t.title, room);
            let gap = w.saturating_sub(idx.width() + title.width() + right.width() + 3);
            let marker = if playing == Some(t.id.as_str()) {
                "▸"
            } else {
                " "
            };
            let item = ListItem::new(Line::from(vec![
                Span::styled(marker, fg(s.accent)),
                Span::styled(idx, fg(s.dim)),
                Span::raw(" "),
                Span::styled(title, fg(s.text)),
                Span::raw(" ".repeat(gap)),
                Span::styled(right, fg(s.dim)),
                Span::raw(" "),
            ]));
            if i == app.selected {
                item.style(theme::selected(s))
            } else {
                item
            }
        })
        .collect();
    f.render_widget(List::new(items), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::TPS_L2;

    #[test]
    fn tab_strip_fits_every_pane_width() {
        let s = &TPS_L2;
        for w in [0, 1, 4, 7, 10, 20, 23, 30, 36, 37, 41, 60, 81, 200] {
            {
                for active in [None, Some(0), Some(3)] {
                    let l = tab_strip(s, active, w);
                    assert!(l.width() <= w.max(7), "w={w} {:?}", l.to_string());
                }
            }
        }
        let t = |w| tab_strip(s, Some(0), w).to_string();
        assert_eq!(t(41), " 1 RESULTS  2 QUEUE  3 LISTS  4 HISTORY  "); // edge margins
        assert!(t(37).starts_with(" 1 RESULTS") && t(37).ends_with("4 HISTORY "));
        assert!(t(36).contains("4 HISTORY"));
        assert!(t(25).contains("1 RES") && t(25).contains("4 HIS"));
        assert_eq!(t(10).split_whitespace().collect::<String>(), "1234");
        assert!(t(81).contains(" 1 RESULTS ")); // wide: padded like keys
    }

    #[test]
    fn active_tab_is_reversed_and_others_dim() {
        let s = &TPS_L2;
        let l = tab_strip(s, Some(1), 41);
        let tabs: Vec<_> = l
            .spans
            .iter()
            .filter(|sp| !sp.content.trim().is_empty())
            .collect();
        assert_eq!(tabs.len(), 4);
        assert_eq!(tabs[1].content.trim(), "2 QUEUE");
        assert_eq!(
            (tabs[1].style.bg, tabs[1].style.fg),
            (Some(s.accent), Some(s.body))
        );
        assert!(tabs[1].style.add_modifier.contains(Modifier::BOLD));
        assert!([0, 2, 3].iter().all(|&i| tabs[i].style.fg == Some(s.dim)));
    }
}
