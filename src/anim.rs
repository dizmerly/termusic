//! Animation state. `App` advances it once per frame in `tick`; `ui` only reads it, and the
//! drawing functions take the plain numbers it gives out.
use std::time::{Duration, Instant};

/// One animation frame; the loop polls input this often while something moves.
pub const FRAME: Duration = Duration::from_millis(50);
/// Hubs advance one glyph every this many frames while playing.
const REEL_EVERY: u64 = 3;
/// Fast spin after a seek, next or prev.
const WHIRR: Duration = Duration::from_millis(400);
/// A counter digit that just changed is drawn dim this long.
const BLINK: Duration = Duration::from_millis(100);
/// A momentary key press holds the cap down this long.
const PRESS: Duration = Duration::from_millis(150);
/// The eject phase of a tape swap, skipped when there is no old tape.
const EJECT_MS: u64 = 200;
/// Label rows slide one row per this many ms.
const ROW_MS: u64 = 40;

/// Segments per level meter.
pub const SEGMENTS: usize = 14;
/// RMS level (dB) of an empty meter; 0 dB fills it.
const FLOOR_DB: f32 = -42.0;
/// The peak segment stays this long, then falls one segment per `FALL_MS`.
const HOLD_MS: u64 = 500;
const FALL_MS: u64 = 100;

/// Lit segments for an RMS level in dB, linear from `FLOOR_DB` (0) to 0 dB (`SEGMENTS`).
pub fn segments(db: f32) -> usize {
    let x = ((db - FLOOR_DB) / -FLOOR_DB * SEGMENTS as f32).round();
    if x.is_nan() {
        0
    } else {
        x.clamp(0.0, SEGMENTS as f32) as usize
    }
}

/// Where a peak reached `ms` ago is drawn now: held, then falling.
pub fn peak_fall(peak: usize, ms: u64) -> usize {
    peak.saturating_sub((ms.saturating_sub(HOLD_MS) / FALL_MS) as usize)
}

/// The transport key caps, left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Prev,
    Play,
    Pause,
    Next,
    Shuffle,
    Loop,
}

/// Where a tape swap is. Milestone 3 hides the label art during `Eject` and `Insert`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The old label has slid up this many rows (0..5).
    Eject(usize),
    /// This many rows of the new label have slid in from the top (0..5).
    Insert(usize),
    /// Full label, hubs whirring.
    Seat,
}

/// The swap phase `ms` into the sequence; `None` once it is over (600 ms).
pub fn swap_phase(ms: u64) -> Option<Phase> {
    match ms {
        0..200 => Some(Phase::Eject((ms / ROW_MS) as usize)),
        200..400 => Some(Phase::Insert(((ms - 200) / ROW_MS) as usize)),
        400..600 => Some(Phase::Seat),
        _ => None,
    }
}

/// Hub steps between frame numbers `prev` and `now`: every frame while whirring, else every
/// `REEL_EVERY` frames (counted on the frame clock, so the pace does not depend on the loop).
pub fn reel_steps(prev: u64, now: u64, whirr: bool) -> u64 {
    if whirr {
        now.saturating_sub(prev)
    } else {
        (now / REEL_EVERY).saturating_sub(prev / REEL_EVERY)
    }
}

/// The counter's three digits, most significant first.
fn digits(n: u64) -> [u64; 3] {
    [n / 100 % 10, n / 10 % 10, n % 10]
}

#[derive(Debug)]
pub struct Anim {
    pub start: Instant,
    /// The time this frame is drawn at; set by `advance`.
    pub now: Instant,
    pub whirr: Option<Instant>,
    pub pressed: Option<(Key, Instant)>,
    /// Running tape swap: when it started and the old label (title, channel).
    pub swap: Option<(Instant, String, String)>,
    /// The label in the deck (title, channel): the old label of the next swap.
    pub shown: Option<(String, String)>,
    /// Hub steps so far; advances only while the tape moves, so a paused hub holds its glyph.
    pub reel: usize,
    /// Frame number at the last `advance`.
    frame: u64,
    /// Counter value at the last `advance`, and when each digit last changed.
    count: u64,
    changed: [Option<Instant>; 3],
    /// Latest (left, right) RMS dB; `None` when nothing plays or there is no data.
    pub levels: Option<(f32, f32)>,
    /// Peak segment per channel and when it was reached.
    peaks: [(usize, Instant); 2],
}

impl Anim {
    pub fn new(now: Instant) -> Self {
        Self {
            start: now,
            now,
            whirr: None,
            pressed: None,
            swap: None,
            shown: None,
            reel: 0,
            frame: 0,
            count: 0,
            changed: [None; 3],
            levels: None,
            peaks: [(0, now); 2],
        }
    }

    fn since(&self, t: Instant) -> Duration {
        self.now.saturating_duration_since(t)
    }

    pub fn whirring(&self) -> bool {
        self.whirr.is_some_and(|t| self.since(t) < WHIRR) || self.phase() == Some(Phase::Seat)
    }

    /// A timed effect other than the whirr (which only counts while playing) is still running.
    pub fn busy(&self) -> bool {
        self.pressed_key().is_some() || self.phase().is_some()
    }

    /// The cap held down by a momentary press.
    pub fn pressed_key(&self) -> Option<Key> {
        self.pressed
            .filter(|&(_, t)| self.since(t) < PRESS)
            .map(|(k, _)| k)
    }

    pub fn phase(&self) -> Option<Phase> {
        let (t, _, _) = self.swap.as_ref()?;
        swap_phase(self.since(*t).as_millis() as u64)
    }

    /// A new tape (`label`) goes in at `now`; with `animate` it plays the swap sequence. The
    /// label still in view is the one ejected: mid-eject that is the old one, else `shown`.
    pub fn swap_to(&mut self, label: (String, String), animate: bool, now: Instant) {
        let ejecting = matches!(self.phase(), Some(Phase::Eject(_)));
        let old = match self.swap.take() {
            Some((_, t, c)) if ejecting => Some((t, c)),
            _ => self.shown.take(),
        };
        self.shown = Some(label);
        if animate {
            self.swap = Some(match old {
                Some((t, c)) => (now, t, c),
                None => (
                    now - Duration::from_millis(EJECT_MS),
                    String::new(),
                    String::new(),
                ),
            });
        }
    }

    fn peak(&self, ch: usize) -> usize {
        let (p, t) = self.peaks[ch];
        peak_fall(p, self.since(t).as_millis() as u64)
    }

    /// (lit segments, peak segment) per channel; `None` draws both meters unlit.
    pub fn meters(&self) -> Option<[(usize, usize); 2]> {
        let (l, r) = self.levels?;
        Some([(segments(l), self.peak(0)), (segments(r), self.peak(1))])
    }

    /// Move to `now`: spin the hubs if the tape moves, note which counter digits changed and
    /// take the new `levels`, raising a peak they reach.
    pub fn advance(&mut self, now: Instant, playing: bool, count: u64, levels: Option<(f32, f32)>) {
        self.now = now;
        let frame =
            (now.saturating_duration_since(self.start).as_millis() / FRAME.as_millis()) as u64;
        let whirr = playing && self.whirring(); // reels hold when paused or idle
        if playing {
            self.reel += reel_steps(self.frame, frame, whirr) as usize;
        }
        self.frame = frame;
        let (old, new) = (digits(self.count), digits(count));
        for i in 0..3 {
            if old[i] != new[i] {
                self.changed[i] = Some(now);
            }
        }
        self.count = count;
        self.levels = levels;
        match levels {
            Some((l, r)) => {
                for (ch, db) in [l, r].into_iter().enumerate() {
                    let seg = segments(db);
                    if seg >= self.peak(ch) {
                        self.peaks[ch] = (seg, now);
                    }
                }
            }
            None => self.peaks = [(0, now); 2],
        }
    }

    /// Which counter digits are between numbers (drawn dim).
    pub fn dim_digits(&self) -> [bool; 3] {
        self.changed
            .map(|t| t.is_some_and(|t| self.since(t) < BLINK))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn reel_steps_every_third_frame_or_every_frame_when_whirring() {
        assert_eq!(reel_steps(0, 2, false), 0);
        assert_eq!(reel_steps(2, 3, false), 1);
        assert_eq!(reel_steps(0, 9, false), 3);
        assert_eq!(reel_steps(4, 5, false), 0); // 150 ms per step, not per call
        assert_eq!(reel_steps(4, 5, true), 1);
        assert_eq!(reel_steps(0, 8, true), 8);
        assert_eq!(reel_steps(9, 3, false), 0); // never backwards
    }

    #[test]
    fn hubs_turn_while_playing_and_hold_when_paused() {
        let t0 = Instant::now();
        let mut a = Anim::new(t0);
        a.advance(t0 + ms(300), true, 0, None); // 6 frames
        assert_eq!(a.reel, 2);
        a.advance(t0 + ms(900), false, 0, None); // paused: holds
        assert_eq!(a.reel, 2);
        a.advance(t0 + ms(1050), true, 0, None); // resumes from the held glyph, no jump
        assert_eq!(a.reel, 3);
        a.whirr = Some(t0 + ms(1050));
        a.advance(t0 + ms(1250), true, 0, None); // whirr: a step per frame
        assert_eq!(a.reel, 7);
        a.advance(t0 + ms(1400), false, 0, None); // paused: holds even mid-whirr
        assert_eq!(a.reel, 7);
        a.advance(t0 + ms(2000), false, 0, None);
        assert_eq!(a.reel, 7);
        assert!(!a.busy());
    }

    #[test]
    fn meter_scale_is_linear_and_clamped() {
        assert_eq!(segments(-42.0), 0);
        assert_eq!(segments(-60.0), 0);
        assert_eq!(segments(f32::NEG_INFINITY), 0); // digital silence
        assert_eq!(segments(f32::NAN), 0);
        assert_eq!(segments(-21.0), 7);
        assert_eq!(segments(-3.0), 13);
        assert_eq!(segments(0.0), SEGMENTS);
        assert_eq!(segments(6.0), SEGMENTS);
    }

    #[test]
    fn peaks_hold_500_ms_then_fall_a_segment_per_100_ms() {
        assert_eq!(peak_fall(10, 0), 10);
        assert_eq!(peak_fall(10, 599), 10);
        assert_eq!(peak_fall(10, 600), 9);
        assert_eq!(peak_fall(10, 1000), 5);
        assert_eq!(peak_fall(2, 5000), 0);
        let t0 = Instant::now();
        let mut a = Anim::new(t0);
        assert_eq!(a.meters(), None);
        a.advance(t0, true, 0, Some((-6.0, -21.0))); // 12 and 7 segments
        assert_eq!(a.meters(), Some([(12, 12), (7, 7)]));
        a.advance(t0 + ms(300), true, 0, Some((-30.0, -21.0))); // left drops: peak held
        assert_eq!(a.meters(), Some([(4, 12), (7, 7)]));
        a.advance(t0 + ms(800), true, 0, Some((-30.0, -12.0))); // right rises: new peak
        assert_eq!(a.meters(), Some([(4, 9), (10, 10)]));
        a.advance(t0 + ms(2000), true, 0, Some((-30.0, -12.0))); // fallen to the level
        assert_eq!(a.meters(), Some([(4, 4), (10, 10)]));
        a.advance(t0 + ms(2100), false, 0, None); // paused: empty, peaks gone
        assert_eq!(a.meters(), None);
        a.advance(t0 + ms(2150), true, 0, Some((-42.0, -42.0)));
        assert_eq!(a.meters(), Some([(0, 0), (0, 0)]));
    }

    #[test]
    fn swap_phases_by_time() {
        let p = |n| swap_phase(n);
        assert_eq!(p(0), Some(Phase::Eject(0)));
        assert_eq!(p(39), Some(Phase::Eject(0)));
        assert_eq!(p(40), Some(Phase::Eject(1)));
        assert_eq!(p(199), Some(Phase::Eject(4)));
        assert_eq!(p(200), Some(Phase::Insert(0)));
        assert_eq!(p(399), Some(Phase::Insert(4)));
        assert_eq!(p(400), Some(Phase::Seat));
        assert_eq!(p(599), Some(Phase::Seat));
        assert_eq!(p(600), None);
    }

    #[test]
    fn swap_ejects_the_label_in_view() {
        let t0 = Instant::now();
        let lab = |s: &str| (s.to_owned(), format!("{s} ch"));
        let mut a = Anim::new(t0);
        a.swap_to(lab("one"), true, t0); // first tape: no eject
        a.now = t0;
        assert_eq!(a.phase(), Some(Phase::Insert(0)));
        a.now = t0 + ms(300); // seated; the next swap ejects "one"
        assert_eq!(a.phase(), Some(Phase::Seat));
        assert!(a.whirring() && a.busy());
        a.swap_to(lab("two"), true, a.now);
        assert_eq!(a.swap.as_ref().unwrap().1, "one");
        assert_eq!(a.phase(), Some(Phase::Eject(0)));
        a.now += ms(100); // mid-eject "one" is still in view: it stays the old label
        a.swap_to(lab("three"), true, a.now);
        assert_eq!(a.swap.as_ref().unwrap().1, "one");
        assert_eq!(a.shown, Some(lab("three")));
        a.now += ms(600);
        assert_eq!(a.phase(), None);
        assert!(!a.busy());
        a.swap_to(lab("four"), false, a.now); // animations off: no sequence, label tracked
        assert!(a.swap.is_none() && a.shown == Some(lab("four")));
    }

    #[test]
    fn a_press_holds_the_cap_150_ms() {
        let t0 = Instant::now();
        let mut a = Anim::new(t0);
        a.pressed = Some((Key::Next, t0));
        assert_eq!(a.pressed_key(), Some(Key::Next));
        assert!(a.busy());
        a.now = t0 + ms(149);
        assert_eq!(a.pressed_key(), Some(Key::Next));
        a.now = t0 + ms(150);
        assert_eq!(a.pressed_key(), None);
        assert!(!a.busy());
    }

    #[test]
    fn changed_counter_digits_blink_for_100_ms() {
        let t0 = Instant::now();
        let mut a = Anim::new(t0);
        assert_eq!(a.dim_digits(), [false; 3]);
        a.advance(t0 + ms(1000), true, 9, None);
        assert_eq!(a.dim_digits(), [false, false, true]);
        a.advance(t0 + ms(1050), true, 9, None);
        assert_eq!(a.dim_digits(), [false, false, true]);
        a.advance(t0 + ms(1100), true, 9, None);
        assert_eq!(a.dim_digits(), [false; 3]);
        a.advance(t0 + ms(2000), true, 10, None); // 009 -> 010: two digits roll
        assert_eq!(a.dim_digits(), [false, true, true]);
        a.advance(t0 + ms(3000), true, 0, None); // 010 -> 000 (counter wraps / track ends)
        assert_eq!(a.dim_digits(), [false, true, false]);
    }
}
