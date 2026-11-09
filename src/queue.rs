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

    /// Replace what plays after the current track (shuffled if shuffle is on).
    pub fn set_upcoming(&mut self, tracks: Vec<Track>) {
        self.upcoming = tracks.into();
        if self.shuffle {
            self.shuffle_upcoming();
        }
    }

    /// Queue a track at the end; never interrupts or starts playback.
    #[allow(dead_code)] // shortcut: no key binds it yet; wire an "add to queue" key when the UI gets one
    pub fn add(&mut self, t: Track) {
        self.upcoming.push_back(t);
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
        if self.upcoming.is_empty() && self.looping != Loop::Off {
            let all = std::mem::take(&mut self.history);
            self.set_upcoming(all);
        }
        self.current = self.upcoming.pop_front();
        self.current.as_ref()
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        // shortcut: turning shuffle off keeps the shuffled order; remember the original if users want it back.
        if self.shuffle {
            self.shuffle_upcoming();
        }
    }

    /// Fisher-Yates over `upcoming` only; `current` is never touched.
    pub fn shuffle_upcoming(&mut self) {
        for i in (1..self.upcoming.len()).rev() {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            self.upcoming.swap(i, (self.rng % (i as u64 + 1)) as usize);
        }
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
