//! Play queue: current track, upcoming tracks, history, shuffle and loop modes.
use crate::youtube::Track;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Loop {
    #[default]
    Off,
    /// Start over from the first played track when the queue runs out.
    All,
    /// Repeat the current track.
    One,
}

impl Loop {
    /// Off -> All -> One -> Off.
    pub fn cycle(self) -> Self {
        match self {
            Loop::Off => Loop::All,
            Loop::All => Loop::One,
            Loop::One => Loop::Off,
        }
    }
}

#[derive(Debug, Default)]
pub struct Queue {
    current: Option<Track>,
    upcoming: VecDeque<Track>,
    /// Leading entries of `upcoming` the user queued; they survive `set_upcoming` and shuffle.
    user_added: usize,
    history: Vec<Track>,
    pub looping: Loop,
    pub shuffle: bool,
    /// xorshift64 state; never 0.
    rng: u64,
}

impl Queue {
    pub fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Self {
            rng: nanos | 1,
            ..Self::default()
        }
    }

    pub fn current(&self) -> Option<&Track> {
        self.current.as_ref()
    }

    /// Replace the queue: play `tracks[start]`, then the tracks after it.
    pub fn set(&mut self, mut tracks: Vec<Track>, start: usize) {
        self.history.clear();
        self.current = (start < tracks.len()).then(|| tracks.remove(start));
        self.set_upcoming(tracks.into_iter().skip(start).collect());
    }

    /// Replace what plays after the current track (shuffled if shuffle is on);
    /// tracks the user queued stay at the front.
    pub fn set_upcoming(&mut self, tracks: Vec<Track>) {
        self.upcoming.truncate(self.user_added);
        // A queued track repeated in the new list would play twice.
        let queued: Vec<String> = self.upcoming.iter().map(|t| t.id.clone()).collect();
        self.upcoming
            .extend(tracks.into_iter().filter(|t| !queued.contains(&t.id)));
        if self.shuffle {
            self.shuffle_upcoming();
        }
    }

    /// Queue a track after the ones already user-queued (before any mix); never interrupts or starts playback.
    pub fn add(&mut self, t: Track) {
        self.upcoming.insert(self.user_added, t);
        self.user_added += 1;
    }

    /// Tracks that play after the current one, in order.
    pub fn upcoming(&self) -> impl Iterator<Item = &Track> {
        self.upcoming.iter()
    }

    /// Advance and return the track to play: the same one on `Loop::One`, `None` at the end.
    pub fn next(&mut self) -> Option<&Track> {
        if self.looping == Loop::One && self.current.is_some() {
            return self.current.as_ref();
        }
        self.skip()
    }

    /// Advance ignoring `Loop::One` (the user asked for the next track).
    pub fn skip(&mut self) -> Option<&Track> {
        self.history.extend(self.current.take());
        if self.upcoming.is_empty() && self.looping == Loop::All {
            let all = std::mem::take(&mut self.history);
            self.set_upcoming(all);
        }
        self.current = self.upcoming.pop_front();
        self.user_added = self.user_added.saturating_sub(1);
        self.current.as_ref()
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        // shortcut: turning shuffle off keeps the shuffled order; remember the original if users want it back.
        if self.shuffle {
            self.shuffle_upcoming();
        }
    }

    /// Fisher-Yates over `upcoming` after the user-queued tracks; `current` is never touched.
    pub fn shuffle_upcoming(&mut self) {
        let from = self.user_added;
        for i in (from + 1..self.upcoming.len()).rev() {
            let j = from + self.rand_below(i + 1 - from);
            self.upcoming.swap(i, j);
        }
    }

    /// xorshift64: good enough to shuffle songs, no dependency.
    fn rand_below(&mut self, n: usize) -> usize {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % n as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str) -> Track {
        Track {
            title: id.into(),
            channel: String::new(),
            id: id.into(),
        }
    }

    fn q(ids: &[&str], start: usize) -> Queue {
        let mut q = Queue::new();
        q.set(ids.iter().map(|i| t(i)).collect(), start);
        q
    }

    fn id(t: Option<&Track>) -> Option<&str> {
        t.map(|t| t.id.as_str())
    }

    fn ids(q: &Queue) -> Vec<&str> {
        q.upcoming.iter().map(|t| t.id.as_str()).collect()
    }

    #[test]
    fn set_starts_at_index_and_resets() {
        let mut q = q(&["a", "b", "c", "d"], 1);
        assert_eq!(id(q.current()), Some("b"));
        assert_eq!(ids(&q), ["c", "d"]);
        q.skip();
        q.set(vec![t("x")], 0);
        assert_eq!((id(q.current()), q.history.len()), (Some("x"), 0));
        assert!(q.upcoming.is_empty());
        q.set(vec![t("x")], 5); // out of range: nothing current
        assert_eq!(q.current(), None);
    }

    #[test]
    fn add_does_not_interrupt() {
        let mut q = q(&["a"], 0);
        q.add(t("b"));
        assert_eq!(id(q.current()), Some("a"));
        assert_eq!(ids(&q), ["b"]);
    }

    #[test]
    fn add_when_empty_only_queues() {
        let mut q = Queue::new();
        q.add(t("a"));
        assert_eq!(q.current(), None);
        assert_eq!(id(q.next()), Some("a"));
    }

    #[test]
    fn queued_tracks_keep_order_and_survive_set_upcoming() {
        let mut q = q(&["a", "b"], 0);
        q.toggle_shuffle();
        q.add(t("x"));
        q.add(t("y"));
        assert_eq!(ids(&q)[..2], ["x", "y"]);
        q.set_upcoming(vec![t("m1"), t("m2")]); // the mix arrives
        assert_eq!(ids(&q)[..2], ["x", "y"]);
        assert_eq!(q.upcoming().count(), 4);
        assert_eq!(id(q.skip()), Some("x"));
        q.add(t("z")); // goes after y, before the mix
        assert_eq!(ids(&q)[..2], ["y", "z"]);
        q.skip();
        q.skip();
        q.set_upcoming(vec![t("n")]); // nothing user-queued left: fully replaced
        assert_eq!(ids(&q), ["n"]);
    }

    #[test]
    fn set_upcoming_drops_duplicates_of_queued() {
        let mut q = q(&["a"], 0);
        q.add(t("x"));
        q.set_upcoming(vec![t("m"), t("x")]);
        assert_eq!(ids(&q), ["x", "m"]);
    }

    #[test]
    fn skip_with_nothing_current_starts_first_upcoming() {
        let mut q = Queue::new();
        q.add(t("a"));
        q.add(t("b"));
        assert_eq!(id(q.skip()), Some("a"));
        assert_eq!(ids(&q), ["b"]);
    }

    #[test]
    fn next_in_order_and_history_grows() {
        let mut q = q(&["a", "b", "c"], 0);
        assert_eq!(id(q.next()), Some("b"));
        assert_eq!(id(q.next()), Some("c"));
        assert_eq!(q.history, [t("a"), t("b")]);
    }

    #[test]
    fn loop_off_ends() {
        let mut q = q(&["a", "b"], 0);
        q.next();
        assert_eq!(q.next(), None);
        assert_eq!(q.current(), None);
        assert_eq!(q.history.len(), 2);
    }

    #[test]
    fn loop_all_wraps_in_order() {
        let mut q = q(&["a", "b", "c"], 0);
        q.looping = Loop::All;
        q.next();
        q.next();
        assert_eq!(id(q.next()), Some("a"));
        assert_eq!(ids(&q), ["b", "c"]);
        assert!(q.history.is_empty());
    }

    #[test]
    fn loop_one_repeats_but_skip_advances() {
        let mut q = q(&["a", "b"], 0);
        q.looping = Loop::One;
        assert_eq!(id(q.next()), Some("a"));
        assert_eq!(id(q.next()), Some("a"));
        assert!(q.history.is_empty());
        assert_eq!(id(q.skip()), Some("b"));
    }

    #[test]
    fn empty_queue_ends_in_every_mode() {
        for looping in [Loop::Off, Loop::All, Loop::One] {
            let mut q = Queue::new();
            q.looping = looping;
            assert_eq!(q.next(), None);
            assert_eq!(q.skip(), None);
        }
    }

    #[test]
    fn loop_all_single_track_repeats() {
        let mut q = q(&["a"], 0);
        q.looping = Loop::All;
        assert_eq!(id(q.next()), Some("a"));
        assert_eq!(id(q.next()), Some("a"));
    }

    #[test]
    fn skip_on_loop_one_stops_at_end() {
        let mut q = q(&["a"], 0);
        q.looping = Loop::One;
        assert_eq!(q.skip(), None); // does not wrap like loop all
    }

    #[test]
    fn shuffle_on_applies_to_new_and_wrapped_queues() {
        let names: Vec<String> = (0..20).map(|i| i.to_string()).collect();
        let tracks: Vec<Track> = names.iter().map(|n| t(n)).collect();
        let mut q = Queue::new();
        q.rng = 7;
        q.toggle_shuffle();
        q.set(tracks.clone(), 3);
        assert_eq!(id(q.current()), Some("3")); // the chosen track plays first
        assert_eq!(q.upcoming.len(), 16);
        assert_ne!(q.upcoming, &tracks[4..]);
        q.looping = Loop::All;
        for _ in 0..17 {
            q.next();
        }
        assert_eq!(q.upcoming.len() + 1, 17); // wrapped over everything played
        q.toggle_shuffle(); // off: order is kept as is
        let before = q.upcoming.clone();
        q.set_upcoming(before.iter().cloned().collect());
        assert_eq!(q.upcoming, before);
    }

    #[test]
    fn shuffle_keeps_current_and_tracks() {
        let all: Vec<String> = (0..20).map(|i| i.to_string()).collect();
        let refs: Vec<&str> = all.iter().map(String::as_str).collect();
        let mut q = q(&refs, 0);
        q.rng = 42;
        q.shuffle_upcoming();
        assert_eq!(id(q.current()), Some("0"));
        let mut got = ids(&q);
        assert_ne!(got, refs[1..], "order unchanged");
        got.sort_unstable();
        let mut want = refs[1..].to_vec();
        want.sort_unstable();
        assert_eq!(got, want);
    }

    #[test]
    fn loop_cycles() {
        assert_eq!(Loop::Off.cycle(), Loop::All);
        assert_eq!(Loop::All.cycle(), Loop::One);
        assert_eq!(Loop::One.cycle(), Loop::Off);
    }
}
