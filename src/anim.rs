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
    /// Hub steps so far; advances only while the tape moves, so a paused hub holds its glyph.
    pub reel: usize,
    /// Frame number at the last `advance`.
    frame: u64,
    /// Counter value at the last `advance`, and when each digit last changed.
    count: u64,
    changed: [Option<Instant>; 3],
}

impl Anim {
    pub fn new(now: Instant) -> Self {
        Self {
            start: now,
            now,
            whirr: None,
            reel: 0,
            frame: 0,
            count: 0,
            changed: [None; 3],
        }
    }

    fn since(&self, t: Instant) -> Duration {
        self.now.saturating_duration_since(t)
    }

    pub fn whirring(&self) -> bool {
        self.whirr.is_some_and(|t| self.since(t) < WHIRR)
    }

    /// A timed effect (not plain playback) is still running.
    pub fn busy(&self) -> bool {
        self.whirring()
    }

    /// Move to `now`: spin the hubs if the tape moves and note which counter digits changed.
    pub fn advance(&mut self, now: Instant, playing: bool, count: u64) {
        self.now = now;
        let frame =
            (now.saturating_duration_since(self.start).as_millis() / FRAME.as_millis()) as u64;
        let whirr = self.whirring();
        if playing || whirr {
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
        a.advance(t0 + ms(300), true, 0); // 6 frames
        assert_eq!(a.reel, 2);
        a.advance(t0 + ms(900), false, 0); // paused: holds
        assert_eq!(a.reel, 2);
        a.advance(t0 + ms(1050), true, 0); // resumes from the held glyph, no jump
        assert_eq!(a.reel, 3);
        a.whirr = Some(t0 + ms(1050));
        a.advance(t0 + ms(1250), true, 0); // whirr: a step per frame
        assert_eq!(a.reel, 7);
        a.advance(t0 + ms(1400), false, 0); // still whirring at 350 ms, even paused
        assert_eq!(a.reel, 10);
        a.advance(t0 + ms(2000), false, 0);
        assert_eq!(a.reel, 10);
        assert!(!a.busy());
    }

    #[test]
    fn changed_counter_digits_blink_for_100_ms() {
        let t0 = Instant::now();
        let mut a = Anim::new(t0);
        assert_eq!(a.dim_digits(), [false; 3]);
        a.advance(t0 + ms(1000), true, 9);
        assert_eq!(a.dim_digits(), [false, false, true]);
        a.advance(t0 + ms(1050), true, 9);
        assert_eq!(a.dim_digits(), [false, false, true]);
        a.advance(t0 + ms(1100), true, 9);
        assert_eq!(a.dim_digits(), [false; 3]);
        a.advance(t0 + ms(2000), true, 10); // 009 -> 010: two digits roll
        assert_eq!(a.dim_digits(), [false, true, true]);
        a.advance(t0 + ms(3000), true, 0); // 010 -> 000 (counter wraps / track ends)
        assert_eq!(a.dim_digits(), [false, true, false]);
    }
}
