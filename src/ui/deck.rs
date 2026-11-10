//! The deck pane: cassette, tape counter and transport keys. The drawing functions are
//! pure (shell and plain values in, lines out) so they test without a terminal.
use super::{clean, fit, pad, wrap};
use crate::anim::{Key, Phase};
use crate::app::{App, Screen};
use crate::commands;
use crate::queue::Loop;
use crate::theme::{self, Shell, fg};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

/// Width of the deck pane.
pub const PANE_W: u16 = 36;
/// Cassette, blank, counter, meter row, keys.
const STACK_H: u16 = 19;
/// Rows the deck takes in the Pocket tier: the stack without the blank row, plus the status
/// row, so a 31-row terminal leaves the J-card 10 rows.
pub const POCKET_H: u16 = STACK_H;
/// Cassette width; the counter row matches it.
const CASSETTE_W: usize = 34;
const LABEL_W: usize = 28;
const LABEL_H: usize = 5;
/// Columns kept for the thumbnail on the label (milestone 3): 10 x 5 cells.
const ART_W: usize = 10;

/// What the cassette shows.
pub struct CassetteView<'a> {
    pub title: &'a str,
    pub channel: &'a str,
    /// position / duration; `None` when the duration is unknown.
    pub progress: Option<f64>,
    /// Reel frame number (0 when paused or idle).
    pub spin: usize,
    /// A track is in the deck (playing or paused).
    pub loaded: bool,
    /// Leave the label's left 10 x 5 cells for the thumbnail.
    pub art: bool,
    /// A tape swap in progress, and the label (title, channel) being ejected.
    pub phase: Option<Phase>,
    pub old: (&'a str, &'a str),
}

/// The cassette: 12 rows of exactly 34 columns.
pub fn cassette(s: &Shell, v: &CassetteView) -> Vec<Line<'static>> {
    let f = fg(s.frame);
    let edge = |l: Vec<Span<'static>>| {
        let mut spans = vec![Span::styled("│ │", f)];
        spans.extend(l);
        spans.push(Span::styled("│ │", f));
        Line::from(spans)
    };
    let mut out = vec![
        Line::styled(format!("╭{}╮", "─".repeat(32)), f),
        Line::styled(format!("│ ┌{}┐ │", "─".repeat(LABEL_W)), f),
    ];
    let empty = Line::from(Span::styled(" ".repeat(LABEL_W), Style::new().bg(s.body)));
    let rows = match v.phase {
        Some(Phase::Eject(k)) => {
            let old = CassetteView {
                title: v.old.0,
                channel: v.old.1,
                loaded: true,
                phase: None,
                ..*v
            };
            slide(label(s, &old), k, empty)
        }
        Some(Phase::Insert(k)) => slide(label(s, v), LABEL_H.saturating_sub(k), empty),
        _ => label(s, v),
    };
    out.extend(rows.into_iter().map(|l| edge(l.spans)));
    out.push(Line::styled(format!("│ └{}┘ │", "─".repeat(LABEL_W)), f));
    out.push(Line::styled(format!("│     ╭{}╮     │", "─".repeat(20)), f));
    let mut win = vec![Span::styled("│     │", f)];
    win.extend(window(s, v));
    win.push(Span::styled("│     │", f));
    out.push(Line::from(win));
    out.push(Line::styled(format!("│     ╰{}╯     │", "─".repeat(20)), f));
    out.push(Line::styled(
        format!(
            "╰─◦{}{}{}◦─╯",
            "─".repeat(10),
            "▔".repeat(8),
            "─".repeat(10)
        ),
        f,
    ));
    out
}

/// The paper label: 5 rows of 28 columns. With `art` the text starts at column 11 (0-based),
/// after the 10-column art area and one column of margin.
pub fn label(s: &Shell, v: &CassetteView) -> Vec<Line<'static>> {
    let (left, w) = if v.art {
        (ART_W + 1, LABEL_W - ART_W - 2)
    } else {
        (1, LABEL_W - 2)
    };
    let ink = Style::new().fg(s.label_ink).bg(s.label);
    let (two, three, chan) = if v.loaded {
        let l = wrap(v.title, w);
        let rest = match l.len() {
            0 | 1 => String::new(),
            2 => l[1].clone(),
            _ => fit(&l[1..].join(" "), w),
        };
        (l.first().cloned().unwrap_or_default(), rest, v.channel)
    } else {
        ("NO TAPE".into(), "press / to search".into(), "")
    };
    let rows = [
        (format!("A {}", "─".repeat(w.saturating_sub(2))), ink),
        (two, ink.add_modifier(Modifier::BOLD)),
        (three, ink.add_modifier(Modifier::BOLD)),
        (chan.to_owned(), ink.add_modifier(Modifier::DIM)),
        (format!("{} C90", "─".repeat(w.saturating_sub(4))), ink),
    ];
    rows.into_iter()
        .map(|(text, style)| {
            Line::from(vec![
                Span::styled(" ".repeat(left), ink),
                Span::styled(pad(&text, w), style),
                Span::styled(" ", ink),
            ])
        })
        .collect()
}

/// Label rows moved up by `offset`: row `i` shows `rows[i + offset]`, or `empty` past the end.
/// Eject slides the old label up (offset 0..5); insert brings the new one down (offset 5..0).
pub fn slide(rows: Vec<Line<'static>>, offset: usize, empty: Line<'static>) -> Vec<Line<'static>> {
    (0..rows.len())
        .map(|i| {
            rows.get(i + offset)
                .cloned()
                .unwrap_or_else(|| empty.clone())
        })
        .collect()
}

/// Tape pack beside a hub, 3 cells read from the outside in: `level / 2` full cells next to
/// the hub and a half cell outside them when the level is odd.
pub fn pack(level: usize) -> String {
    let level = level.min(6);
    let (full, half) = (level / 2, level % 2);
    " ".repeat(3 - full - half) + &"▒".repeat(half) + &"▓".repeat(full)
}

/// Pack levels (0..=6) of the left and right reels.
pub fn levels(v: &CassetteView) -> (usize, usize) {
    match (v.loaded, v.progress) {
        (false, _) => (0, 0), // no tape in the deck
        (true, None) => (3, 3),
        (true, Some(p)) => {
            let p = p.clamp(0.0, 1.0);
            (
                (6.0 * (1.0 - p)).round() as usize,
                (6.0 * p).round() as usize,
            )
        }
    }
}

/// The 20-column reel window: two 9-column reels and a 2-column gap.
fn window(s: &Shell, v: &CassetteView) -> Vec<Span<'static>> {
    let bg = Style::new().bg(s.body);
    // Out of the deck mid-swap: bare hubs.
    let (l, r) = match v.phase {
        Some(Phase::Eject(_) | Phase::Insert(_)) => (0, 0),
        _ => levels(v),
    };
    let frame = |n: usize| theme::REEL_FRAMES[n % theme::REEL_FRAMES.len()];
    let reel = |level: usize, hub: char| {
        let side = pack(level);
        [
            Span::styled(side.clone(), bg.fg(s.dim)),
            Span::styled(format!("({hub})"), bg.fg(s.accent)),
            Span::styled(side.chars().rev().collect::<String>(), bg.fg(s.dim)),
        ]
    };
    let mut out = Vec::from(reel(l, frame(v.spin)));
    out.push(Span::styled("  ", bg));
    out.extend(reel(r, frame(v.spin + 1)));
    out
}

/// `012  ━━━━●────  1:12`: tape counter, progress and elapsed time in `width` columns.
/// `dim` digits are between numbers (they just rolled).
pub fn counter_row(
    s: &Shell,
    position: f64,
    duration: Option<f64>,
    loaded: bool,
    width: usize,
    dim: [bool; 3],
) -> Line<'static> {
    let secs = if loaded { position.max(0.0) as u64 } else { 0 };
    let elapsed = if loaded {
        format!("{}:{:02}", secs / 60, secs % 60)
    } else {
        String::new()
    };
    let n = width.saturating_sub(12); // 3 counter + 2 + line + 2 + 5 time
    let mut spans: Vec<Span<'static>> = format!("{:03}", secs % 1000)
        .chars()
        .zip(dim)
        .map(|(c, d)| Span::styled(c.to_string(), fg(if d { s.dim } else { s.text })))
        .collect();
    spans.push(Span::raw("  "));
    match duration.filter(|d| loaded && *d > 0.0) {
        Some(d) if n > 0 => {
            let head = ((position / d).clamp(0.0, 1.0) * (n - 1) as f64).round() as usize;
            spans.push(Span::styled("━".repeat(head), fg(s.accent)));
            spans.push(Span::styled("●", fg(s.text)));
            spans.push(Span::styled("─".repeat(n - 1 - head), fg(s.dim)));
        }
        _ => spans.push(Span::styled("─".repeat(n), fg(s.dim))),
    }
    spans.push(Span::raw("  "));
    spans.push(Span::styled(format!("{elapsed:>5}"), fg(s.text)));
    Line::from(spans)
}

/// Six 5 x 3 transport key caps in 4 rows of 30 columns: prev, play, pause, next, shuffle,
/// loop. An up cap sits in rows 1-3 over a shadow; a down cap (latched by its state, or
/// `pressed` for a moment) sits in rows 2-4, lit.
pub fn key_caps(
    s: &Shell,
    loaded: bool,
    playing: bool,
    shuffle: bool,
    looping: Loop,
    pressed: Option<Key>,
) -> Vec<Line<'static>> {
    let loop_glyph = if looping == Loop::One {
        theme::ICON_LOOP_ONE
    } else {
        theme::ICON_LOOP
    };
    let caps = [
        (Key::Prev, "◀◀", commands::prev::KEY, false),
        (Key::Play, "▶", commands::pause::KEY, loaded && playing),
        (Key::Pause, "▮▮", commands::pause::KEY, loaded && !playing),
        (Key::Next, "▶▶", commands::skip::KEY, false),
        (
            Key::Shuffle,
            theme::ICON_SHUFFLE,
            commands::shuffle::KEY,
            shuffle,
        ),
        (
            Key::Loop,
            loop_glyph,
            commands::loop_track::KEY,
            looping != Loop::Off,
        ),
    ];
    let mut rows: [Vec<Span<'static>>; 4] = Default::default();
    for (k, glyph, key, latched) in caps {
        let down = latched || pressed == Some(k);
        let (edge, face) = if down {
            let on = fg(s.accent).add_modifier(Modifier::BOLD);
            (on, on)
        } else {
            (fg(s.frame), fg(s.text))
        };
        let gap = 3usize.saturating_sub(glyph.width());
        let text = format!(
            "{}{glyph}{}",
            " ".repeat(gap / 2),
            " ".repeat(gap - gap / 2)
        );
        let cap = [
            vec![Span::styled("┌───┐", edge)],
            vec![
                Span::styled("│", edge),
                Span::styled(text, face),
                Span::styled("│", edge),
            ],
            vec![Span::styled(format!("└─{key}─┘"), edge)],
        ];
        let top = usize::from(down);
        for (row, spans) in rows[top..].iter_mut().zip(cap) {
            row.extend(spans);
        }
        if down {
            rows[0].push(Span::raw("     "));
        } else {
            rows[3].push(Span::styled(" ▀▀▀ ", fg(s.dim)));
        }
    }
    rows.into_iter().map(Line::from).collect()
}

/// The deck pane. Deck tier: the stack centered vertically with the status row pinned to the
/// bottom. Pocket tier: the stack at the top with the status row under it.
pub fn draw(f: &mut Frame, app: &App, area: Rect, pocket: bool) {
    let s = app.shell();
    let p = &app.player;
    let title = p.now_playing.as_deref().unwrap_or("");
    let channel = app
        .queue
        .current()
        .filter(|t| t.title == title)
        .map_or("", |t| t.channel.as_str());
    let anim = app.settings.animations.then_some(&app.anim);
    let swap = anim.and_then(|a| a.swap.as_ref());
    let v = CassetteView {
        title,
        channel,
        progress: p.duration.filter(|d| *d > 0.0).map(|d| p.position / d),
        // Animations off: the milestone 1 reels, one step per second of playback.
        spin: match (anim, p.playing) {
            (Some(a), _) => a.reel,
            (None, true) => p.position as usize,
            (None, false) => 0,
        },
        loaded: p.now_playing.is_some(),
        art: false,
        phase: anim.and_then(|a| a.phase()),
        old: swap.map_or(("", ""), |(_, t, c)| (t.as_str(), c.as_str())),
    };
    let mut lines = cassette(s, &v);
    if !pocket {
        lines.push(Line::raw(""));
    }
    let dim = anim.map_or([false; 3], |a| a.dim_digits());
    lines.push(counter_row(
        s, p.position, p.duration, v.loaded, CASSETTE_W, dim,
    ));
    lines.push(Line::raw("")); // level meters (milestone 2)
    lines.extend(key_caps(
        s,
        v.loaded,
        p.playing,
        app.queue.shuffle,
        app.queue.looping,
        anim.and_then(|a| a.pressed_key()),
    ));

    let top = if pocket {
        0
    } else {
        area.height.saturating_sub(STACK_H + 1) / 2
    };
    let stack_h = lines.len() as u16;
    let stack = Rect::new(
        area.x,
        area.y + top.min(area.height),
        area.width,
        stack_h.min(area.height.saturating_sub(top)),
    );
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), stack);

    let mut status = Vec::new();
    if app.busy {
        status.push(format!("{} working…", theme::ICON_BUSY));
    }
    let mut color = s.accent;
    if !app.status.is_empty() {
        status.push(clean(&app.status));
    } else if app.screen == Screen::History
        && !app.searching
        && let Some(e) = app.history.get(app.selected)
    {
        status.push(format!("youtu.be/{}", e.track.id)); // the link left out of the list row
        color = s.dim;
    }
    let row = if pocket {
        stack_h
    } else {
        area.height.saturating_sub(1)
    };
    if row < area.height {
        let r = Rect::new(area.x, area.y + row, area.width, 1);
        let text = fit(&status.join("  "), (area.width as usize).saturating_sub(4));
        f.render_widget(
            Paragraph::new(Line::styled(format!("  {text}"), fg(color))),
            r,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{SHELLS, TPS_L2};

    fn view(title: &str, progress: Option<f64>, loaded: bool, art: bool) -> CassetteView<'_> {
        CassetteView {
            title,
            channel: "Rick Astley",
            progress,
            spin: 0,
            loaded,
            art,
            phase: None,
            old: ("", ""),
        }
    }

    fn text(l: &Line) -> String {
        l.to_string()
    }

    #[test]
    fn cassette_rows_are_34_columns_for_any_title() {
        let long = "Never Gonna Give You Up (Official Music Video) remastered in 4K ".repeat(3);
        for title in [
            "",
            "x",
            "Never Gonna Give You Up",
            "世界世界世界世界世界世界世界世界世界世界世界",
            "😀 emoji title 😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀",
            "Supercalifragilisticexpialidocious_and_then_some_more",
            &long,
        ] {
            for (loaded, art) in [(true, false), (true, true), (false, false), (false, true)] {
                for p in [
                    None,
                    Some(0.0),
                    Some(0.3),
                    Some(1.0),
                    Some(7.0),
                    Some(f64::NAN),
                ] {
                    for s in SHELLS {
                        let lines = cassette(s, &view(title, p, loaded, art));
                        assert_eq!(lines.len(), 12);
                        for l in &lines {
                            assert_eq!(l.width(), CASSETTE_W, "{title:?} {art} {:?}", text(l));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn cassette_shape() {
        let l: Vec<String> = cassette(&TPS_L2, &view("t", Some(0.5), true, false))
            .iter()
            .map(text)
            .collect();
        assert_eq!(l[0], format!("╭{}╮", "─".repeat(32)));
        assert!(l[1].starts_with("│ ┌") && l[7].starts_with("│ └"));
        assert!(
            l[2..7]
                .iter()
                .all(|r| r.starts_with("│ │") && r.ends_with("│ │"))
        );
        assert!(l[9].starts_with("│     │") && l[9].ends_with("│     │"));
        assert_eq!(
            l[11],
            format!(
                "╰─◦{}{}{}◦─╯",
                "─".repeat(10),
                "▔".repeat(8),
                "─".repeat(10)
            )
        );
    }

    #[test]
    fn label_text_wraps_cuts_and_idles() {
        let rows =
            |v: &CassetteView| -> Vec<String> { label(&TPS_L2, v).iter().map(text).collect() };
        let l = rows(&view("Never Gonna Give You Up", None, true, false));
        assert!(l.iter().all(|r| r.width() == LABEL_W));
        assert!(l[0].starts_with(" A ─") && l[4].ends_with("─ C90 "));
        assert_eq!(l[1].trim(), "Never Gonna Give You Up");
        assert_eq!(l[3].trim(), "Rick Astley");
        let l = rows(&view(&"word ".repeat(30), None, true, false));
        assert!(l[1].trim().starts_with("word") && l[2].trim().ends_with('…'));
        let l = rows(&view("anything", None, false, false));
        assert_eq!(
            (l[1].trim(), l[2].trim(), l[3].trim()),
            ("NO TAPE", "press / to search", "")
        );
    }

    #[test]
    fn label_leaves_the_art_area_when_asked() {
        let v = view("Take On Me", Some(0.1), true, true);
        let l: Vec<String> = label(&TPS_L2, &v).iter().map(text).collect();
        for r in &l {
            assert_eq!(r.width(), LABEL_W);
            assert_eq!(&r[..ART_W + 1], " ".repeat(ART_W + 1)); // art + margin are blank
        }
        // 0-based column 11 is the first text column.
        assert_eq!(l[0].chars().nth(11), Some('A'));
        assert_eq!(l[1].chars().nth(11), Some('T'));
        let plain: Vec<String> = label(&TPS_L2, &view("Take On Me", None, true, false))
            .iter()
            .map(text)
            .collect();
        assert_eq!(plain[1].chars().nth(1), Some('T'));
    }

    #[test]
    fn label_is_paper_colored() {
        for s in SHELLS {
            for l in label(s, &view("t", None, true, false)) {
                assert!(l.spans.iter().all(|sp| sp.style.bg == Some(s.label)));
                assert!(l.spans.iter().all(|sp| sp.style.fg == Some(s.label_ink)));
            }
        }
    }

    #[test]
    fn packs_fill_from_the_hub_out() {
        assert_eq!(pack(0), "   ");
        assert_eq!(pack(1), "  ▒");
        assert_eq!(pack(2), "  ▓");
        assert_eq!(pack(3), " ▒▓");
        assert_eq!(pack(5), "▒▓▓");
        assert_eq!(pack(6), "▓▓▓");
        assert_eq!(pack(99), "▓▓▓");
    }

    #[test]
    fn reel_levels_follow_progress() {
        let lv = |p, loaded| levels(&view("t", p, loaded, false));
        assert_eq!(lv(Some(0.0), true), (6, 0));
        assert_eq!(lv(Some(0.25), true), (5, 2)); // each side rounds on its own
        assert_eq!(lv(Some(0.5), true), (3, 3));
        assert_eq!(lv(Some(1.0), true), (0, 6));
        assert_eq!(lv(Some(3.0), true), (0, 6));
        assert_eq!(lv(Some(-1.0), true), (6, 0));
        assert_eq!(lv(None, true), (3, 3)); // unknown length
        assert_eq!(lv(Some(0.9), false), (0, 0)); // no tape: both hubs bare
    }

    #[test]
    fn reel_window_draws_packs_and_spins() {
        let row = |p, spin| {
            let mut v = view("t", p, true, false);
            v.spin = spin;
            text(&cassette(&TPS_L2, &v)[9])
        };
        assert_eq!(row(Some(0.0), 0), "│     │▓▓▓(◐)▓▓▓     (◓)   │     │");
        assert_eq!(row(Some(0.25), 0), "│     │▒▓▓(◐)▓▓▒    ▓(◓)▓  │     │");
        assert_eq!(row(None, 0), "│     │ ▒▓(◐)▓▒    ▒▓(◓)▓▒ │     │");
        assert_eq!(row(Some(1.0), 0), "│     │   (◐)     ▓▓▓(◓)▓▓▓│     │");
        let mut idle = view("", Some(0.5), false, false);
        idle.spin = 3;
        assert_eq!(
            text(&cassette(&TPS_L2, &idle)[9]),
            "│     │   (◒)        (◐)   │     │"
        );
        assert_ne!(row(Some(0.3), 0), row(Some(0.3), 1));
        assert_eq!(row(Some(0.3), 0), row(Some(0.3), theme::REEL_FRAMES.len()));
    }

    #[test]
    fn counter_row_fills_its_width() {
        let s = &TPS_L2;
        let r = counter_row(s, 72.4, Some(240.0), true, 34, [false; 3]);
        assert_eq!(r.width(), 34);
        let t = text(&r);
        assert!(t.starts_with("072  ━") && t.ends_with(" 1:12"), "{t}");
        assert_eq!(t.matches('●').count(), 1);
        let t = text(&counter_row(s, 1234.0, None, true, 34, [false; 3])); // live: no head, wraps at 1000
        assert!(t.starts_with("234  ─") && !t.contains('●') && t.ends_with("20:34"));
        let t = text(&counter_row(s, 50.0, Some(100.0), false, 34, [false; 3])); // nothing playing
        assert!(t.starts_with("000  ─") && !t.contains('●') && t.trim_end().ends_with('─'));
        assert_eq!(
            text(&counter_row(s, 0.0, Some(10.0), true, 34, [false; 3])).find('●'),
            Some(5)
        );
        assert!(text(&counter_row(s, 10.0, Some(10.0), true, 34, [false; 3])).contains("━●  "));
        let r = counter_row(s, 72.0, Some(240.0), true, 34, [false, true, false]);
        let colors: Vec<_> = r.spans[..3].iter().map(|sp| sp.style.fg).collect();
        assert_eq!(colors, [Some(s.text), Some(s.dim), Some(s.text)]); // 0 7 2: the 7 rolls
        for w in [0, 5, 12, 13] {
            counter_row(s, 5.0, Some(10.0), true, w, [true; 3]); // tiny: no panic
        }
    }

    /// Cell `cap` (5 columns) of each key row, with the color of its middle column.
    fn caps_cell(l: &[Line], cap: usize) -> Vec<(String, Option<ratatui::style::Color>)> {
        l.iter()
            .map(|row| {
                let cells: Vec<(char, Style)> = row
                    .spans
                    .iter()
                    .flat_map(|sp| sp.content.chars().map(move |c| (c, sp.style)))
                    .collect();
                let c = &cells[cap * 5..cap * 5 + 5];
                (c.iter().map(|x| x.0).collect(), c[2].1.fg)
            })
            .collect()
    }

    #[test]
    fn key_caps_are_5_wide_4_rows_and_press_down() {
        let s = &TPS_L2;
        let down = |l: &[Line], cap: usize| {
            let c = caps_cell(l, cap);
            let d = c[0].0 == "     ";
            if d {
                assert_eq!(c[1].0, "┌───┐");
                assert!(c[2].1 == Some(s.accent) && c[3].0.starts_with("└─")); // lit while down
            } else {
                assert!(c[0].0 == "┌───┐" && c[2].0.starts_with("└─"));
                assert_eq!((c[3].0.as_str(), c[3].1), (" ▀▀▀ ", Some(s.dim)));
                assert_eq!(c[1].1, Some(s.text));
            }
            d
        };
        for looping in [Loop::Off, Loop::One, Loop::All] {
            for pressed in [None, Some(Key::Next), Some(Key::Loop)] {
                let l = key_caps(s, true, true, true, looping, pressed);
                assert_eq!(l.len(), 4);
                assert!(l.iter().all(|r| r.width() == 30));
                assert_eq!(
                    down(&l, 5),
                    looping != Loop::Off || pressed == Some(Key::Loop)
                );
                assert_eq!(down(&l, 3), pressed == Some(Key::Next));
            }
        }
        let rows: Vec<String> = key_caps(s, false, false, false, Loop::One, None)
            .iter()
            .map(text)
            .collect();
        assert_eq!(
            rows,
            [
                "┌───┐┌───┐┌───┐┌───┐┌───┐     ",
                "│◀◀ ││ ▶ ││▮▮ ││▶▶ ││ ⤮ │┌───┐",
                "└─b─┘└─p─┘└─p─┘└─n─┘└─s─┘│↻1 │",
                " ▀▀▀  ▀▀▀  ▀▀▀  ▀▀▀  ▀▀▀ └─l─┘",
            ]
        );
        let all = |l: &[Line]| (0..6).map(|c| down(l, c)).collect::<Vec<_>>();
        let l = key_caps(s, true, true, false, Loop::Off, None); // playing
        assert_eq!(all(&l), [false, true, false, false, false, false]);
        let l = key_caps(s, true, false, true, Loop::Off, None); // paused, shuffle on
        assert_eq!(all(&l), [false, false, true, false, true, false]);
        let l = key_caps(s, false, true, false, Loop::Off, Some(Key::Prev)); // nothing loaded
        assert_eq!(all(&l), [true, false, false, false, false, false]);
    }

    #[test]
    fn swap_slides_the_label_out_and_in() {
        let mut v = view("New Song", Some(0.5), true, false);
        v.channel = "New Band";
        v.old = ("Old Song", "Old Band");
        let label_rows = |v: &CassetteView| -> Vec<String> {
            cassette(&TPS_L2, v)[2..7]
                .iter()
                .map(|l| text(l).chars().skip(3).take(LABEL_W).collect())
                .collect()
        };
        let full_old = {
            let mut o = view("Old Song", None, true, false);
            o.channel = "Old Band";
            label_rows(&o)
        };
        let full_new = label_rows(&v);
        let blank = " ".repeat(LABEL_W);
        v.phase = Some(Phase::Eject(0));
        assert_eq!(label_rows(&v), full_old);
        v.phase = Some(Phase::Eject(2)); // up two rows, the bottom two empty
        let r = label_rows(&v);
        assert_eq!(r[..3], full_old[2..]);
        assert!(r[3..].iter().all(|x| *x == blank));
        v.phase = Some(Phase::Insert(0));
        assert!(label_rows(&v).iter().all(|x| *x == blank));
        v.phase = Some(Phase::Insert(1)); // the label's bottom row shows at the top
        let r = label_rows(&v);
        assert_eq!(r[0], full_new[4]);
        assert!(r[1..].iter().all(|x| *x == blank));
        v.phase = Some(Phase::Insert(4));
        assert_eq!(label_rows(&v)[..4], full_new[1..]);
        v.phase = Some(Phase::Seat);
        assert_eq!(label_rows(&v), full_new);
        // Empty slot is the deck body color; mid-swap the reels are bare.
        v.phase = Some(Phase::Insert(0));
        let c = cassette(&TPS_L2, &v);
        assert!(
            c[4].spans
                .iter()
                .skip(1)
                .take(1)
                .all(|sp| sp.style.bg == Some(TPS_L2.body))
        );
        assert_eq!(text(&c[9]), "│     │   (◐)        (◓)   │     │");
        v.phase = Some(Phase::Seat);
        assert_ne!(
            text(&cassette(&TPS_L2, &v)[9]),
            "│     │   (◐)        (◓)   │     │"
        );
        for k in 0..=9 {
            for ph in [Phase::Eject(k), Phase::Insert(k)] {
                v.phase = Some(ph);
                assert!(
                    cassette(&TPS_L2, &v)
                        .iter()
                        .all(|l| l.width() == CASSETTE_W)
                );
            }
        }
    }
}
