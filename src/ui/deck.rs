//! The deck pane: cassette, tape counter and transport keys. The drawing functions are
//! pure (shell and plain values in, lines out) so they test without a terminal.
use super::{fit, pad, wrap};
use crate::app::App;
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
const STACK_H: u16 = 18;
/// Rows the deck takes in the Pocket tier: the stack without the blank row, plus the status
/// row, so a 30-row terminal leaves the J-card 10 rows.
pub const POCKET_H: u16 = STACK_H;
/// Cassette width; the counter row matches it.
const CASSETTE_W: usize = 34;
const LABEL_W: usize = 28;
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
    out.extend(label(s, v).into_iter().map(|l| edge(l.spans)));
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
        (false, _) => (6, 0),
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
    let (l, r) = levels(v);
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
pub fn counter_row(
    s: &Shell,
    position: f64,
    duration: Option<f64>,
    loaded: bool,
    width: usize,
) -> Line<'static> {
    let secs = if loaded { position.max(0.0) as u64 } else { 0 };
    let elapsed = if loaded {
        format!("{}:{:02}", secs / 60, secs % 60)
    } else {
        String::new()
    };
    let n = width.saturating_sub(12); // 3 counter + 2 + line + 2 + 5 time
    let mut spans = vec![
        Span::styled(format!("{:03}", secs % 1000), fg(s.text)),
        Span::raw("  "),
    ];
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

/// Six 5 x 3 transport key caps, 30 columns: prev, play, pause, next, shuffle, loop.
pub fn key_caps(
    s: &Shell,
    loaded: bool,
    playing: bool,
    shuffle: bool,
    looping: Loop,
) -> Vec<Line<'static>> {
    let loop_glyph = if looping == Loop::One {
        theme::ICON_LOOP_ONE
    } else {
        theme::ICON_LOOP
    };
    let caps = [
        ("◀◀", commands::prev::KEY, false),
        ("▶", commands::pause::KEY, loaded && playing),
        ("▮▮", commands::pause::KEY, loaded && !playing),
        ("▶▶", commands::skip::KEY, false),
        (theme::ICON_SHUFFLE, commands::shuffle::KEY, shuffle),
        (loop_glyph, commands::loop_track::KEY, looping != Loop::Off),
    ];
    let mut rows: [Vec<Span<'static>>; 3] = Default::default();
    for (glyph, key, lit) in caps {
        let (edge, face) = if lit {
            let on = fg(s.accent).add_modifier(Modifier::BOLD);
            (on, on)
        } else {
            (fg(s.frame), fg(s.text))
        };
        let gap = 3usize.saturating_sub(glyph.width());
        rows[0].push(Span::styled("┌───┐", edge));
        rows[1].push(Span::styled("│", edge));
        rows[1].push(Span::styled(
            format!(
                "{}{glyph}{}",
                " ".repeat(gap / 2),
                " ".repeat(gap - gap / 2)
            ),
            face,
        ));
        rows[1].push(Span::styled("│", edge));
        rows[2].push(Span::styled(format!("└─{key}─┘"), edge));
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
    let v = CassetteView {
        title,
        channel,
        progress: p.duration.filter(|d| *d > 0.0).map(|d| p.position / d),
        spin: if p.playing { p.position as usize } else { 0 },
        loaded: p.now_playing.is_some(),
        art: false,
    };
    let mut lines = cassette(s, &v);
    if !pocket {
        lines.push(Line::raw(""));
    }
    lines.push(counter_row(s, p.position, p.duration, v.loaded, CASSETTE_W));
    lines.push(Line::raw("")); // level meters (milestone 2)
    lines.extend(key_caps(
        s,
        v.loaded,
        p.playing,
        app.queue.shuffle,
        app.queue.looping,
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
    if !app.status.is_empty() {
        status.push(app.status.clone());
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
            Paragraph::new(Line::styled(format!("  {text}"), fg(s.accent))),
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
        assert_eq!(lv(Some(0.9), false), (6, 0)); // no tape: all on the left
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
        assert_ne!(row(Some(0.3), 0), row(Some(0.3), 1));
        assert_eq!(row(Some(0.3), 0), row(Some(0.3), theme::REEL_FRAMES.len()));
    }

    #[test]
    fn counter_row_fills_its_width() {
        let s = &TPS_L2;
        let r = counter_row(s, 72.4, Some(240.0), true, 34);
        assert_eq!(r.width(), 34);
        let t = text(&r);
        assert!(t.starts_with("072  ━") && t.ends_with(" 1:12"), "{t}");
        assert_eq!(t.matches('●').count(), 1);
        let t = text(&counter_row(s, 1234.0, None, true, 34)); // live: no head, wraps at 1000
        assert!(t.starts_with("234  ─") && !t.contains('●') && t.ends_with("20:34"));
        let t = text(&counter_row(s, 50.0, Some(100.0), false, 34)); // nothing playing
        assert!(t.starts_with("000  ─") && !t.contains('●') && t.trim_end().ends_with('─'));
        assert_eq!(
            text(&counter_row(s, 0.0, Some(10.0), true, 34)).find('●'),
            Some(5)
        );
        assert!(text(&counter_row(s, 10.0, Some(10.0), true, 34)).contains("━●  "));
        for w in [0, 5, 12, 13] {
            counter_row(s, 5.0, Some(10.0), true, w); // tiny: no panic
        }
    }

    #[test]
    fn key_caps_are_5_by_3_and_light_up() {
        let s = &TPS_L2;
        let lit = |l: &[Line], cap: usize| l[1].spans[cap * 3 + 1].style.fg == Some(s.accent);
        for looping in [Loop::Off, Loop::One, Loop::All] {
            let l = key_caps(s, true, true, true, looping);
            assert_eq!(l.len(), 3);
            assert!(l.iter().all(|r| r.width() == 30));
            for cap in 0..6 {
                let cell: String = l[1].spans[cap * 3..cap * 3 + 3]
                    .iter()
                    .map(|sp| sp.content.as_ref())
                    .collect();
                assert_eq!(cell.width(), 5, "{cell}");
            }
            assert_eq!(lit(&l, 5), looping != Loop::Off);
        }
        assert_eq!(
            text(&key_caps(s, false, false, false, Loop::One)[1]),
            "│◀◀ ││ ▶ ││▮▮ ││▶▶ ││ ⤮ ││↻1 │"
        );
        assert_eq!(
            text(&key_caps(s, false, false, false, Loop::Off)[2]),
            "└─b─┘└─p─┘└─p─┘└─n─┘└─s─┘└─l─┘"
        );
        let l = key_caps(s, true, true, false, Loop::Off); // playing
        assert!(!lit(&l, 0) && lit(&l, 1) && !lit(&l, 2) && !lit(&l, 3) && !lit(&l, 4));
        let l = key_caps(s, true, false, true, Loop::Off); // paused, shuffle on
        assert!(!lit(&l, 1) && lit(&l, 2) && lit(&l, 4));
        let l = key_caps(s, false, true, false, Loop::Off); // nothing loaded
        assert!(!lit(&l, 1) && !lit(&l, 2));
    }
}
