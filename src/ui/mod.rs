//! Rendering. Reads `App`, never mutates state (except the visible page size and help scroll).
//! Layout tiers: the Deck (deck pane beside the J-card), Pocket (deck above it) and Compact
//! (the J-card alone with a one-line status bar).
mod deck;
mod jcard;
mod overlay;

use crate::app::{App, Screen};
use crate::history::Entry;
use crate::queue::Loop;
use crate::theme::{self, Shell, fg};
use crate::youtube::Track;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tier {
    Deck,
    Pocket,
    Compact,
}

fn tier(w: u16, h: u16) -> Tier {
    if w >= 76 && h >= 22 {
        Tier::Deck
    } else if w >= 38 && h >= 30 {
        Tier::Pocket
    } else {
        Tier::Compact
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let s = app.shell();
    let area = f.area();
    f.render_widget(Block::new().style(Style::new().bg(s.body).fg(s.text)), area);
    let t = tier(area.width, area.height);
    if t == Tier::Compact {
        let [list, bar] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
        jcard::draw(f, app, list);
        draw_bar(f, app, bar);
        return;
    }
    let (hints, shell_name) = bottom_titles(app, s, area.width as usize);
    let frame = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(fg(s.frame))
        .title_top(Line::from(vec![
            Span::styled("─ ", fg(s.frame)),
            Span::styled("TERMUSIC", theme::title(s)),
            Span::styled(" ", fg(s.frame)),
        ]))
        .title_top(top_right(app, s, area.width).right_aligned())
        .title_bottom(hints)
        .title_bottom(shell_name.right_aligned());
    let inner = frame.inner(area);
    f.render_widget(frame, area);
    if t == Tier::Deck {
        let [d, card] =
            Layout::horizontal([Constraint::Length(deck::PANE_W), Constraint::Min(0)]).areas(inner);
        deck::draw(f, app, d, false);
        let divider = Block::new()
            .borders(Borders::LEFT)
            .border_style(fg(s.frame));
        let card_inner = divider.inner(card);
        f.render_widget(divider, card);
        jcard::draw(f, app, card_inner);
    } else {
        let [d, card] =
            Layout::vertical([Constraint::Length(deck::POCKET_H), Constraint::Min(0)]).areas(inner);
        deck::draw(f, app, d, true);
        jcard::draw(f, app, card);
    }
}

/// ` ⤮ ↻1 ── VOL ▮▮▮▮▮▮▮▯▯▯ 70% ─` for the top border; the bar only when there is room.
fn top_right(app: &App, s: &Shell, width: u16) -> Line<'static> {
    let loop_icon = match app.queue.looping {
        Loop::Off => None,
        Loop::All => Some(theme::ICON_LOOP),
        Loop::One => Some(theme::ICON_LOOP_ONE),
    };
    let icons: Vec<&str> = [app.queue.shuffle.then_some(theme::ICON_SHUFFLE), loop_icon]
        .into_iter()
        .flatten()
        .collect();
    let mut spans = Vec::new();
    if !icons.is_empty() {
        spans.push(Span::styled(
            format!(" {}", icons.join(" ")),
            theme::title(s),
        ));
        spans.push(Span::styled(" ──", fg(s.frame)));
    }
    let vol = app.player.volume;
    spans.push(Span::styled(" VOL ", fg(s.dim)));
    if width >= 60 {
        let level = bar(f64::from(vol) / 100.0, 10, ['▮', '▯']);
        spans.push(Span::styled(format!("{level} "), fg(s.text)));
    }
    spans.push(Span::styled(format!("{vol}%"), fg(s.text)));
    spans.push(Span::styled(" ─", fg(s.frame)));
    Line::from(spans)
}

/// Key hints for the focus (left) and the shell's name (right) for the bottom border.
fn bottom_titles(app: &App, s: &Shell, width: usize) -> (Line<'static>, Line<'static>) {
    let hints = match app.screen {
        Screen::Offline => "r retry · q quit",
        _ if app.searching => "Enter search · Esc back · F1 help",
        Screen::Help | Screen::Settings => "Esc back",
        Screen::Login => "/ search · Tab next tab · Enter sign in · ? help",
        _ => "/ search · Tab next tab · Enter play · ? help",
    };
    let name = Line::from(vec![
        Span::styled(" ", fg(s.frame)),
        Span::styled(s.name, fg(s.dim)),
        Span::styled(" ─", fg(s.frame)),
    ]);
    // 2 corners, "─ " and " " around the hints, and a column before the name.
    let room = width.saturating_sub(name.width() + 6);
    let hints = Line::from(vec![
        Span::styled("─ ", fg(s.frame)),
        Span::styled(fit(hints, room), fg(s.text)),
        Span::styled(" ", fg(s.frame)),
    ]);
    (hints, name)
}

/// The Compact tier's one-line status bar.
fn draw_bar(f: &mut Frame, app: &App, area: Rect) {
    let s = app.shell();
    let (icon, color, text) = match (&app.player.now_playing, app.player.playing) {
        (Some(t), true) => (theme::ICON_PLAY, s.accent, t.as_str()),
        (Some(t), false) => (theme::ICON_PAUSE, s.text, t.as_str()),
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
        spans.push(Span::styled(format!(" {icon}"), fg(s.accent)));
    }
    if app.busy {
        spans.push(Span::styled(
            format!("  {} working…", theme::ICON_BUSY),
            fg(s.dim),
        ));
    }
    if !app.status.is_empty() {
        spans.push(Span::styled(
            format!("  {}", clean(&app.status)),
            fg(s.accent),
        ));
    }
    // Right side: progress (only with a known length and room), volume, then the help hint.
    // While typing the letters are text, so it shows the F-keys.
    let hint = match (area.width >= 70 && app.status.is_empty(), app.searching) {
        (true, true) => "F3 settings · F4 history · F1 help ",
        (true, false) => "S settings · H history · ? help ",
        (false, true) => "F1 help ",
        (false, false) => "? help ",
    };
    let mut right = Vec::new();
    if let (Some(d), true) = (app.player.duration, area.width >= 60) {
        let time = format!("{} / {}  ", mmss(app.player.position), mmss(d));
        let level = bar(app.player.position / d.max(1.0), 10, ['█', '░']);
        right.push(Span::styled(format!("{level} {time}"), fg(s.text)));
    }
    let vol = format!("{} {}%  ", theme::ICON_NOTE, app.player.volume);
    right.push(Span::styled(vol, fg(s.text)));
    right.push(Span::styled(hint, fg(s.dim)));
    // Short of room: the hint goes first, then the volume, before the title is cut.
    let left_full: usize = spans.iter().map(Span::width).sum::<usize>() + 1;
    for _ in 0..2 {
        if left_full + right.iter().map(Span::width).sum::<usize>() > area.width as usize {
            right.pop();
        }
    }
    let rw = right.iter().map(Span::width).sum::<usize>() as u16;
    let [left, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(rw)]).areas(area);
    // Truncate the title with … so it never runs into the right side (1 col gap).
    let other: usize = spans.iter().skip(2).map(Span::width).sum::<usize>() + spans[0].width();
    let room = (left.width as usize).saturating_sub(other + 1);
    spans[1] = Span::styled(fit(text, room), fg(color));
    f.render_widget(Block::new().style(Style::new().bg(s.select_bg)), area);
    f.render_widget(Paragraph::new(Line::from(spans)), left);
    f.render_widget(Paragraph::new(Line::from(right)), right_area);
}

/// `width` cells of `on`/`off` glyphs for a 0..=1 fraction.
fn bar(frac: f64, width: usize, [on, off]: [char; 2]) -> String {
    let n = (frac.clamp(0.0, 1.0) * width as f64).round() as usize;
    std::iter::repeat_n(on, n)
        .chain(std::iter::repeat_n(off, width - n))
        .collect()
}

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// History entry as a list row: the channel column shows the resume time (the deck's status
/// row shows the link).
fn history_row(e: &Entry) -> Track {
    Track {
        channel: mmss(e.position_secs as f64),
        ..e.track.clone()
    }
}

/// Truncate to `n` display columns with an ellipsis (at least the ellipsis).
fn fit(s: &str, n: usize) -> String {
    if s.width() <= n {
        return s.into();
    }
    let mut out = String::new();
    // Measure the whole prefix, as `width()` of the result will: sequences (emoji + U+FE0F) widen.
    for c in s.chars() {
        out.push(c);
        if out.width() + 1 > n {
            out.pop();
            break;
        }
    }
    out + "…"
}

/// Replace control characters (tabs, newlines) so text from outside draws on one line.
fn clean(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// `fit`, then spaces up to exactly `n` columns (`format!` pads by chars, not columns).
fn pad(s: &str, n: usize) -> String {
    if n == 0 {
        return String::new();
    }
    let t = fit(s, n);
    let w = t.width();
    t + &" ".repeat(n.saturating_sub(w))
}

/// Greedy word wrap to `w` columns; a word longer than a line is split.
fn wrap(s: &str, w: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let mut word = word;
        while !word.is_empty() {
            let sep = usize::from(!cur.is_empty());
            if cur.width() + sep + word.width() <= w {
                if sep == 1 {
                    cur.push(' ');
                }
                cur.push_str(word);
                break;
            }
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            // Longer than a whole line: cut it (at least one char so this always ends).
            let mut used = 0;
            let cut = word
                .char_indices()
                .find(|&(i, c)| {
                    used += c.width().unwrap_or(0);
                    i > 0 && used > w
                })
                .map_or(word.len(), |(i, _)| i);
            out.push(word[..cut].to_owned());
            word = &word[cut..];
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands;
    use crate::theme::SHELLS;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

    /// A blank app with focus on the list and a quiet status (startup depends on the machine).
    fn blank() -> App {
        let mut a = App::blank();
        (a.screen, a.searching) = (Screen::Results, false);
        a.status.clear();
        a
    }

    fn buffer(app: &mut App, w: u16, h: u16) -> Buffer {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, app)).unwrap();
        t.backend().buffer().clone()
    }

    fn rows(app: &mut App, w: u16, h: u16) -> Vec<String> {
        let b = buffer(app, w, h);
        (0..h)
            .map(|y| (0..w).map(|x| b[(x, y)].symbol()).collect())
            .collect()
    }

    fn render(app: &mut App, w: u16, h: u16) -> String {
        rows(app, w, h).concat()
    }

    fn track(title: &str, channel: &str, id: &str) -> Track {
        Track {
            title: title.into(),
            channel: channel.into(),
            id: id.into(),
        }
    }

    /// A playing track at 30% with shuffle and loop-one on (`mpv` is never started).
    fn playing() -> App {
        let mut a = blank();
        a.results = [
            ("Never Gonna Give You Up", "Rick Astley"),
            ("Take On Me", "a-ha"),
            ("Africa", "Toto"),
            ("Sweet Dreams (Are Made of This)", "Eurythmics"),
            ("Everybody Wants To Rule The World", "Tears For Fears"),
            ("Don't You (Forget About Me)", "Simple Minds"),
            ("Tainted Love", "Soft Cell"),
            ("Blue Monday", "New Order"),
        ]
        .iter()
        .enumerate()
        .map(|(i, (t, c))| track(t, c, &i.to_string()))
        .collect();
        a.queue.set(a.results.clone(), 0);
        a.player.now_playing = Some(a.results[0].title.clone());
        a.player.playing = true;
        a.player.duration = Some(240.0);
        a.player.position = 72.0;
        a.queue.toggle_shuffle();
        a.queue.looping = Loop::One;
        a.input = "never gonna give".into();
        a.status = "queued: Take On Me".into();
        a
    }

    #[test]
    fn tiers_by_size() {
        assert_eq!(tier(80, 24), Tier::Deck);
        assert_eq!(tier(76, 22), Tier::Deck);
        assert_eq!(tier(75, 40), Tier::Pocket);
        assert_eq!(tier(76, 21), Tier::Compact);
        assert_eq!(tier(38, 30), Tier::Pocket);
        assert_eq!(tier(37, 30), Tier::Compact);
        assert_eq!(tier(60, 29), Tier::Compact);
        assert_eq!(tier(0, 0), Tier::Compact);
    }

    #[test]
    fn deck_at_80x24() {
        let mut a = playing();
        let r = rows(&mut a, 80, 24);
        assert!(
            r[0].starts_with("╭─ TERMUSIC ") && r[0].ends_with("─╮"),
            "{}",
            r[0]
        );
        assert!(r[0].contains("⤮ ↻1 ── VOL ▮▮▮▮▮▮▮▯▯▯ 70% ─╮"), "{}", r[0]);
        assert!(r[23].starts_with("╰─ / search") && r[23].ends_with(" TPS-L2 ─╯"));
        let cassette_top = r
            .iter()
            .position(|l| l.contains("╭────────────────────────────────╮"))
            .unwrap();
        assert!(r[cassette_top].starts_with("│ ╭")); // deck pane: one column of margin
        assert!(
            r.iter().all(|l| l.chars().nth(37) == Some('│')
                || l.starts_with('╭')
                || l.starts_with('╰'))
        );
        assert!(r.iter().any(|l| l.contains("Never Gonna Give")));
        assert!(r.iter().any(|l| l.contains("072  ━") && l.contains("1:12")));
        assert!(
            r.iter()
                .any(|l| l.contains("│◀◀ ││ ▶ ││▮▮ ││▶▶ ││ ⤮ ││↻1 │"))
        );
        assert!(
            r.iter()
                .any(|l| l.contains("└─b─┘└─p─┘└─p─┘└─n─┘└─s─┘└─l─┘"))
        );
        assert!(r[22].contains("queued: Take On Me")); // status pinned to the pane bottom
        assert_eq!(
            r[1],
            "│                                    │ 1 RESULTS  2 QUEUE  3 LISTS  4 HISTORY  │"
        );
        assert!(
            r[2].ends_with("♪ never gonna give              8 found │"),
            "{}",
            r[2]
        );
        assert!(r[2].contains("♪ never gonna give"));
        assert!(r.iter().any(|l| l.contains("▸01 Never Gonna Give")));
        assert!(r.iter().all(|l| l.width() == 80));
    }

    #[test]
    fn animations_off_draws_the_milestone_1_reels() {
        let hubs = |a: &mut App| {
            let r = rows(a, 80, 24);
            let l = r
                .iter()
                .find(|l| l.contains(")▓") || l.contains(") "))
                .unwrap();
            l.chars()
                .filter(|c| theme::REEL_FRAMES.contains(c))
                .collect::<String>()
        };
        let mut a = playing(); // 72 s in
        a.anim.reel = 5;
        a.settings.animations = false;
        assert_eq!(hubs(&mut a), "◐◓"); // frame = whole seconds, as before
        a.player.position = 73.0;
        assert_eq!(hubs(&mut a), "◓◑");
        a.player.playing = false;
        assert_eq!(hubs(&mut a), "◐◓"); // paused: back to frame 0
        a.settings.animations = true;
        assert_eq!(hubs(&mut a), "◓◑"); // the animation clock's frame, held while paused
    }

    #[test]
    fn idle_deck_shows_no_tape_and_empty_results_hint() {
        let mut a = blank();
        let s = render(&mut a, 80, 24);
        assert!(s.contains("NO TAPE") && s.contains("press / to search"));
        assert!(s.contains("000  ──────"));
        assert!(s.contains("Type / and search for a song"));
        assert!(s.contains("♪ / to search"));
        assert!(!s.contains('●') && s.contains("VOL ▮▮▮▮▮▮▮▯▯▯ 70% ─"));
    }

    #[test]
    fn nothing_panics_at_any_size() {
        let mut apps = [blank(), playing()];
        apps[1].screen = Screen::Queue;
        for a in &mut apps {
            for (w, h) in [
                (0, 0),
                (1, 1),
                (1, 0),
                (0, 5),
                (3, 3),
                (10, 2),
                (20, 5),
                (30, 6),
                (37, 30),
                (38, 30),
                (40, 1),
                (60, 16),
                (75, 21),
                (76, 22),
                (80, 3),
                (80, 24),
                (200, 60),
            ] {
                for scr in [
                    Screen::Results,
                    Screen::Help,
                    Screen::Settings,
                    Screen::Offline,
                    Screen::Login,
                ] {
                    a.screen = scr;
                    render(a, w, h);
                }
                a.searching = true;
                render(a, w, h);
                a.searching = false;
            }
        }
    }

    #[test]
    fn every_color_comes_from_the_shell() {
        for s in SHELLS {
            let mut a = playing();
            a.settings.shell = s.id.into();
            a.select(1);
            let roles = [
                s.body,
                s.frame,
                s.text,
                s.dim,
                s.accent,
                s.label,
                s.label_ink,
                s.meter_lo,
                s.meter_hi,
                s.select_bg,
            ];
            for (w, h) in [(80, 24), (60, 34), (60, 16)] {
                for scr in [
                    Screen::Results,
                    Screen::Queue,
                    Screen::Help,
                    Screen::Settings,
                    Screen::Offline,
                ] {
                    a.screen = scr;
                    let b = buffer(&mut a, w, h);
                    for c in b.content() {
                        assert!(
                            roles.contains(&c.bg),
                            "{} {w}x{h} {scr:?} bg {:?}",
                            s.id,
                            c.bg
                        );
                        assert!(
                            roles.contains(&c.fg),
                            "{} {w}x{h} {scr:?} fg {:?}",
                            s.id,
                            c.fg
                        );
                        assert_ne!(c.bg, Color::Reset);
                    }
                }
            }
            a.screen = Screen::Results;
            let b = buffer(&mut a, 80, 24);
            let r = rows(&mut a, 80, 24);
            let find = |t: &str| {
                // The first match left of the J-card (the label), else anywhere.
                let y = r
                    .iter()
                    .position(|l| l.chars().take(37).collect::<String>().contains(t))
                    .or_else(|| r.iter().position(|l| l.contains(t)))
                    .unwrap();
                let col = r[y][..r[y].find(t).unwrap()].chars().count();
                (col as u16, y as u16)
            };
            assert_eq!(b[(39, 1)].bg, s.accent); // active tab, reversed
            assert_eq!(b[find("Never Gonna Give You Up")].bg, s.label); // label text on paper
            assert_eq!(b[find("02 Take On Me")].bg, s.select_bg); // selected row
        }
    }

    #[test]
    fn pocket_and_compact_layouts() {
        let mut a = playing();
        let r = rows(&mut a, 60, 34);
        assert!(r[0].starts_with("╭─ TERMUSIC") && r[33].ends_with(" TPS-L2 ─╯"));
        let top = r
            .iter()
            .position(|l| l.contains("╭────────────────────────────────╮"))
            .unwrap();
        let strip = r.iter().position(|l| l.contains("1 RESULTS")).unwrap();
        assert!(top < strip && 34 - strip >= 11, "J-card keeps 10+ rows");
        assert!(r.iter().any(|l| l.contains("└─b─┘")));
        let r = rows(&mut a, 38, 30); // narrowest pocket: the volume bar goes, the rest fits
        assert!(r[0].contains("⤮ ↻1 ── VOL 70% ─╮") && r[0].starts_with("╭─ TERMUSIC"));
        let strip = r.iter().position(|l| l.contains("1 RESULTS")).unwrap();
        assert_eq!(29 - strip, 10, "J-card rows above the bottom border");
        assert!(r[strip - 1].contains("queued: Take On Me"));
        assert!(r.iter().any(|l| l.contains("└─b─┘")));
        a.status.clear();
        let r = rows(&mut a, 60, 16);
        assert!(!r[0].contains("TERMUSIC") && r[0].contains("1 RESULTS"));
        assert!(r[15].contains("▶ Never Gonna Give You Up") && r[15].contains("01:12 / 04:00"));
    }

    #[test]
    fn help_fits_the_card_at_80x24() {
        let mut a = blank();
        a.screen = Screen::Help;
        let r = rows(&mut a, 80, 24);
        let s = r.concat();
        assert!(s.contains("closes this"), "help clipped");
        for want in ["Playback", "play: Play the", "pause: Pause or"] {
            assert!(s.contains(want), "{want}");
        }
        assert!(!s.contains("Keys")); // the rest scrolls
        assert!(r[1].starts_with("│ ") && r[1].contains("╭ Help")); // over the J-card only
        assert!(s.contains("NO TAPE")); // the deck stays visible
    }

    #[test]
    fn help_lists_every_command_and_key_when_scrolled() {
        let mut a = blank();
        a.screen = Screen::Help;
        let mut all = String::new();
        let mut scroll = 0;
        loop {
            a.help_scroll = scroll;
            all += &render(&mut a, 80, 24);
            if a.help_scroll < scroll {
                break; // clamped: the end was shown
            }
            scroll += 10;
        }
        for c in commands::enabled() {
            assert!(all.contains(&format!("{}: ", c.name)), "{}", c.name);
        }
        for g in ["Playback", "Queue", "Navigation", "Screens", "Keys"] {
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
        let lines = overlay::help_lines(a.shell(), 39).len();
        assert_eq!(
            usize::from(a.help_scroll),
            lines - 20,
            "scroll is clamped to the last page"
        );
    }

    #[test]
    fn bottom_hints_follow_focus() {
        let mut a = blank();
        let last = |a: &mut App| rows(a, 80, 24).pop().unwrap();
        assert!(last(&mut a).contains("/ search · Tab next tab · Enter play · ? help"));
        a.searching = true;
        assert!(last(&mut a).contains("Enter search · Esc back · F1 help"));
        a.searching = false;
        a.screen = Screen::Settings;
        assert!(last(&mut a).contains("─ Esc back ─"));
        a.screen = Screen::Offline;
        assert!(last(&mut a).contains("r retry"));
        a.screen = Screen::Login;
        assert!(last(&mut a).contains("Enter sign in"));
        for w in [76, 60, 38] {
            a.screen = Screen::Results;
            let r = rows(&mut a, w as u16, 40);
            assert!(r[39].ends_with("TPS-L2 ─╯"), "{w}: {}", r[39]);
        }
    }

    #[test]
    fn search_field_shows_focus_text_and_hint() {
        let mut a = blank();
        let row = |a: &mut App| rows(a, 80, 24)[2].clone();
        assert!(row(&mut a).contains("♪ / to search"));
        a.input = "take on me".into();
        assert!(row(&mut a).contains("♪ take on me") && !row(&mut a).contains('▏'));
        a.searching = true;
        assert!(row(&mut a).contains("♪ take on me▏"));
        a.input = "x".repeat(100) + "END";
        assert!(row(&mut a).contains("xEND▏")); // a long query shows its end
        assert!(!row(&mut a).contains("found")); // the query wins over the count
        a.results = vec![track("t", "c", "i")];
        a.input = "take on me".into();
        assert!(
            row(&mut a)
                .trim_end_matches(['│', ' '])
                .ends_with("1 found")
        );
        a.searching = false;
        assert!(row(&mut a).contains("♪ take on me") && row(&mut a).contains("1 found"));
    }

    #[test]
    fn settings_overlay_and_history_tab() {
        let mut a = blank();
        a.screen = Screen::Settings;
        let s = render(&mut a, 80, 24);
        for w in [
            "Volume",
            "Shell",
            "TPS-L2",
            "History file",
            "TERMUSIC_HISTORY",
            "Clear history",
        ] {
            assert!(s.contains(w), "{w}");
        }
        a.screen = Screen::History;
        assert!(render(&mut a, 80, 24).contains("Nothing played yet"));
        crate::history::push(&mut a.history, track("Old song", "Chan", "o"));
        let s = render(&mut a, 80, 24);
        assert!(s.contains("Old song") && s.contains("youtu.be/"));
        a.screen = Screen::Settings;
        a.history_path = Some("/very/long/".repeat(20).into());
        let s = render(&mut a, 80, 24);
        assert!(s.contains("1 track)") && s.contains("…"));
    }

    #[test]
    fn empty_tab_states() {
        let mut a = blank();
        a.screen = Screen::Queue;
        let s = render(&mut a, 80, 24);
        assert!(s.contains("Nothing queued.") && s.contains("result plays"));
        a.screen = Screen::Login;
        let s = render(&mut a, 80, 24);
        assert!(s.contains("Sign in with Google") && s.contains("sign in"));
        a.login_url = Some("https://accounts.example/x".into());
        assert!(render(&mut a, 80, 24).contains("https://accounts.example/x"));
        a.screen = Screen::Offline;
        let s = render(&mut a, 80, 24);
        assert!(s.contains("You are offline") && s.contains("NO TAPE"));
    }

    #[test]
    fn queue_tab_lists_current_then_upcoming() {
        let mut a = playing();
        a.screen = Screen::Queue;
        for n in ["one", "two", "three", "four"] {
            a.queue.add(track(n, "", n));
        }
        let s = render(&mut a, 80, 24);
        assert!(s.contains("▸01 Never Gonna Give"));
        for n in ["02 one", "03 two", "04 three", "05 four"] {
            assert!(s.contains(n), "{n}");
        }
    }

    #[test]
    fn bars_and_time() {
        assert_eq!(bar(0.5, 10, ['█', '░']), "█████░░░░░");
        assert_eq!(bar(2.0, 4, ['▮', '▯']), "▮▮▮▮");
        assert_eq!(bar(-1.0, 4, ['▮', '▯']), "▯▯▯▯");
        assert_eq!(mmss(75.9), "01:15");
        assert_eq!(mmss(-3.0), "00:00");
    }

    #[test]
    fn top_border_shows_modes_and_volume() {
        let mut a = blank();
        assert!(!rows(&mut a, 80, 24)[0].contains(theme::ICON_LOOP));
        a.player.volume = 100;
        a.queue.toggle_shuffle();
        a.queue.looping = Loop::One;
        let top = rows(&mut a, 80, 24)[0].clone();
        assert!(top.contains("⤮ ↻1") && top.contains("VOL ▮▮▮▮▮▮▮▮▮▮ 100% ─╮"));
        a.queue.looping = Loop::All;
        assert!(rows(&mut a, 80, 24)[0].contains("⤮ ↻ ──"));
        let top = rows(&mut a, 38, 30)[0].clone();
        assert!(
            top.starts_with("╭─ TERMUSIC") && top.ends_with("100% ─╮"),
            "{top}"
        );
    }

    #[test]
    fn compact_bar_keeps_progress_volume_and_titles_cut() {
        let mut a = blank();
        a.player.volume = 80;
        a.player.position = 65.0;
        a.player.duration = Some(185.0);
        let s = rows(&mut a, 60, 16).pop().unwrap();
        assert!(s.contains("01:05 / 03:05") && s.contains("80%") && s.contains("help"));
        let s = rows(&mut a, 50, 16).pop().unwrap(); // narrow: no progress
        assert!(!s.contains("01:05") && s.contains("80%"));
        let s = rows(&mut a, 30, 16).pop().unwrap(); // short: the hint goes first
        assert!(s.contains("nothing playing") && s.contains("80%") && !s.contains("help"));
        let s = rows(&mut a, 22, 16).pop().unwrap(); // then the volume
        assert!(s.contains("nothing playing") && !s.contains("80%"));
        a.player.now_playing = Some("T".repeat(120));
        let s = rows(&mut a, 60, 16).pop().unwrap(); // a long title is cut only after both
        assert!(s.contains("…") && s.contains("01:05 / 03:05"));
        assert!(!s.contains("80%") && !s.contains("help"));
        a.player.now_playing = Some("Short".into());
        a.status = "queued: x".into();
        let s = rows(&mut a, 60, 16).pop().unwrap();
        assert!(s.contains("Short") && s.contains("queued: x"));
    }

    #[test]
    fn long_titles_keep_channel_visible() {
        let mut a = blank();
        a.results = vec![track(&"x".repeat(200), "Artist", "i")];
        assert!(render(&mut a, 80, 24).contains("Artist"));
        assert!(render(&mut a, 60, 16).contains("Artist"));
        render(&mut a, 20, 5); // tiny terminal must not panic
    }

    #[test]
    fn status_row_shows_and_cuts_long_status() {
        let mut a = blank();
        a.status = "queued: a fairly long title for the bar".into();
        let r = rows(&mut a, 80, 24);
        assert!(r[22].contains("queued: a fairly long title") && r[22].contains('…'));
        a.busy = true;
        assert!(rows(&mut a, 80, 24)[22].contains("⟳ working…"));
    }

    #[test]
    fn history_rows_show_link_and_resume_time() {
        let e = Entry {
            track: track("Old", "Chan", "abc123"),
            position_secs: 83,
        };
        assert_eq!(history_row(&e).channel, "01:23");
        let mut app = blank();
        app.screen = Screen::History;
        app.history = vec![e];
        let r = rows(&mut app, 80, 24);
        assert!(
            r.iter()
                .any(|l| l.contains("01 Old") && l.ends_with("01:23 │"))
        );
        assert!(r[22].contains("youtu.be/abc123")); // the link is in the deck's status row
        let b = buffer(&mut app, 80, 24);
        assert_eq!(b[(3, 22)].fg, app.shell().dim);
        app.status = "history cleared".into(); // a real status wins
        let r = rows(&mut app, 80, 24);
        assert!(r[22].contains("history cleared") && !r.concat().contains("youtu.be"));
        app.status.clear();
        app.searching = true; // focus in the search field: no link
        assert!(!render(&mut app, 80, 24).contains("youtu.be"));
        app.searching = false;
        app.screen = Screen::Help; // overlay: no link
        assert!(!render(&mut app, 80, 24).contains("youtu.be"));
        app.screen = Screen::History;
        assert!(!render(&mut app, 60, 16).contains("youtu.be")); // Compact has no status row
        render(&mut app, 30, 6);
    }

    #[test]
    fn fit_pad_and_wrap_measure_columns() {
        assert_eq!(fit("abc", 3), "abc");
        assert_eq!(fit("abcd", 3), "ab…");
        assert_eq!(fit("世界世界", 3), "世…"); // two columns per character
        assert_eq!(fit("世界世界", 4), "世…");
        assert_eq!(fit("abc", 0), "…");
        assert_eq!(fit("", 0), "");
        assert_eq!(pad("世", 4), "世  ");
        assert_eq!(pad("abcdef", 4).width(), 4);
        assert_eq!(wrap("aa bb cc", 5), ["aa bb", "cc"]);
        assert_eq!(wrap("abcdefgh", 3), ["abc", "def", "gh"]);
        assert_eq!(wrap("世界世界", 3), ["世", "界", "世", "界"]);
        assert_eq!(wrap("x", 0), ["x"]); // never loops forever
        assert!(wrap("   ", 4).is_empty());
    }

    #[test]
    fn list_draws_rows_and_never_panics_when_tiny() {
        let mut app = blank();
        app.results = (0..5)
            .map(|i| track(&format!("song{i}"), "chan", &i.to_string()))
            .collect();
        let out = render(&mut app, 80, 24);
        assert!(out.contains("song0") && out.contains("song4"));
        for (w, h) in [(1, 1), (3, 3), (10, 2), (80, 4)] {
            render(&mut app, w, h);
        }
    }

    /// Prints the renders the designer reviews:
    /// `cargo test dump_renders -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn dump_renders() {
        let show = |name: &str, a: &mut App, w: u16, h: u16| {
            println!("=== {name} ({w}x{h}) ===");
            for l in rows(a, w, h) {
                println!("{l}");
            }
            println!();
        };
        for (w, h) in [(80, 24), (120, 36)] {
            show("Deck, idle", &mut blank(), w, h);
            show("Deck, playing 30%, shuffle, loop one", &mut playing(), w, h);
            let mut a = playing();
            a.screen = Screen::Queue;
            for n in [
                "Here Comes The Rain Again",
                "Smalltown Boy",
                "Enjoy The Silence",
            ] {
                a.queue.add(track(n, "Various", n));
            }
            show("Deck, Queue tab", &mut a, w, h);
            let mut a = playing();
            a.screen = Screen::History;
            a.status.clear();
            for (i, t) in a.results.clone().into_iter().enumerate().rev() {
                crate::history::push(&mut a.history, t);
                a.history[0].position_secs = 37 * i as u64;
            }
            a.select(1);
            show("Deck, History tab", &mut a, w, h);
            let mut a = playing();
            a.screen = Screen::Help;
            show("Deck, Help overlay", &mut a, w, h);
            let mut a = playing();
            a.screen = Screen::Settings;
            show("Deck, Settings overlay", &mut a, w, h);
            let mut a = playing();
            a.searching = true;
            show("Deck, search focused", &mut a, w, h);
        }
        show("Pocket, playing", &mut playing(), 60, 34);
        show("Compact, playing", &mut playing(), 60, 16);
    }

    #[test]
    fn fit_and_pad_never_exceed_their_width() {
        let nasty = [
            "",
            "plain text",
            "日本語のタイトル",
            "⚠️⚠️⚠️⚠️⚠️⚠️⚠️",
            "👨\u{200d}👩\u{200d}👧 family",
            "e\u{301}e\u{301}e\u{301}e\u{301}",
            "a\tb\nc\u{7f}d\u{1b}[0m",
            "❤️x❤️y❤️",
        ];
        for s in nasty {
            for n in 0..=12 {
                assert!(fit(s, n).width() <= n.max(1), "fit({s:?}, {n})");
                assert_eq!(pad(s, n).width(), n, "pad({s:?}, {n})");
            }
        }
        assert_eq!(clean("a\tb\nc"), "a b c");
    }

    #[test]
    fn control_characters_in_status_and_input_do_not_break_the_frame() {
        let check = |a: &mut App, want: &str| {
            let r = rows(a, 80, 24);
            assert!(r.iter().any(|l| l.contains(want)), "{want:?} not drawn");
            let last = |l: &String| l.chars().last();
            assert_eq!(last(&r[0]), Some('╮'));
            assert_eq!(last(&r[23]), Some('╯'));
            assert!(
                r[1..23].iter().all(|l| last(l) == Some('│')),
                "border moved"
            );
        };
        let mut a = blank();
        a.status = "a\tb\nc".into();
        check(&mut a, "a b c");
        let mut a = blank();
        a.searching = true;
        a.input = "x\ty".into();
        check(&mut a, "x y");
    }
}
