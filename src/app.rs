//! Application state and key handling.
use crate::cmd::{self, Cmd, Event};
use crate::commands;
use crate::player::Player;
use crate::queue::{Loop, Queue};
use crate::settings::Settings;
use crate::youtube::{self, Track};
use crate::{auth, net};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::future::Future;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// Results of background network tasks, applied to `App` by `tick`.
pub enum Msg {
    Online,
    Offline,
    Err(String),
    LoginUrl(String),
    LoggedIn,
    Tracks(Vec<Track>),
    /// Related tracks for the video id they were fetched for.
    Related(String, Vec<Track>),
    Playlists(Vec<Track>),
    /// Items of the playlist (by id) the user opened: they become the queue and start playing.
    PlaylistTracks(String, Vec<Track>),
}

pub(crate) const LOGIN_HINT: &str = "Playlists need you to be logged in (P/F2: sign in)";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Login,
    Search,
    Results,
    Playlists,
    Help,
    Settings,
    History,
    Offline,
}

impl Screen {
    /// Screens opened on top of another one; Esc returns to `App::prev`.
    fn is_overlay(self) -> bool {
        matches!(self, Self::Help | Self::Settings | Self::History)
    }
}

/// Rows on the settings screen: volume, history file, clear history.
const SETTING_ROWS: usize = 3;

pub struct App {
    pub screen: Screen,
    /// Screen to return to when leaving help.
    pub prev: Screen,
    pub input: String,
    pub results: Vec<Track>,
    pub playlists: Vec<Track>,
    /// `results` is an opened playlist (keeps its order; no related lookup on play).
    from_playlist: bool,
    /// The logged-out hint was shown; asking for playlists again opens Login.
    login_prompted: bool,
    /// Test hook: overrides the on-disk login state.
    #[cfg(test)]
    logged_in: Option<bool>,
    /// Authorization page while waiting for the user to sign in.
    pub login_url: Option<String>,
    /// A network task is in flight.
    pub busy: bool,
    tx: UnboundedSender<Msg>,
    rx: UnboundedReceiver<Msg>,
    pub selected: usize,
    /// First visible row of the results list.
    pub offset: usize,
    /// Visible rows in the results list; updated by `ui` each frame.
    pub page: usize,
    pub player: Player,
    pub queue: Queue,
    pub status: String,
    pub quit: bool,
    pub settings: Settings,
    /// Last played tracks, most recent first.
    pub history: Vec<Track>,
    /// Highlighted row on the settings screen.
    pub setting_row: usize,
    /// First visible line of the help screen.
    pub help_scroll: u16,
    /// List selection and offset to restore when an overlay closes.
    saved_sel: (usize, usize),
}

impl App {
    pub fn new() -> Self {
        // Login is optional (only playlists need it), so we always start at Search.
        let screen = if net::is_online() {
            Screen::Search
        } else {
            Screen::Offline
        };
        let (tx, rx) = unbounded_channel();
        let settings = Settings::load();
        Self {
            history: settings
                .history_file()
                .map(|p| crate::history::load(&p))
                .unwrap_or_default(),
            player: Player::new(settings.volume),
            settings,
            setting_row: 0,
            help_scroll: 0,
            saved_sel: (0, 0),
            playlists: Vec::new(),
            from_playlist: false,
            login_prompted: false,
            #[cfg(test)]
            logged_in: None,
            login_url: None,
            busy: false,
            tx,
            rx,
            screen,
            prev: screen,
            input: String::new(),
            results: Vec::new(),
            selected: 0,
            offset: 0,
            page: 10,
            queue: Queue::new(),
            status: crate::player::missing_deps().unwrap_or_default(),
            quit: false,
        }
    }

    /// Run a network task off the UI thread; offline is detected first and reported as `Msg::Offline`.
    fn spawn(&mut self, f: impl Future<Output = anyhow::Result<Msg>> + Send + 'static) {
        self.busy = true;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let msg = if !tokio::task::spawn_blocking(net::is_online)
                .await
                .unwrap_or(false)
            {
                Msg::Offline
            } else {
                f.await.unwrap_or_else(|e| Msg::Err(e.to_string()))
            };
            let _ = tx.send(msg);
        });
    }

    fn run_search(&mut self) {
        let q = self.input.trim().to_owned();
        self.spawn(async move { Ok(Msg::Tracks(youtube::search(&q).await?)) });
    }

    fn start_login(&mut self) {
        let tx = self.tx.clone();
        self.spawn(async move {
            auth::login(|url| {
                let _ = tx.send(Msg::LoginUrl(url.into()));
            })
            .await?;
            Ok(Msg::LoggedIn)
        });
    }

    fn open_playlists(&mut self) {
        self.open_playlists_as(self.logged_in());
    }

    fn logged_in(&self) -> bool {
        #[cfg(test)]
        if let Some(v) = self.logged_in {
            return v;
        }
        auth::is_logged_in()
    }

    /// Logged out: soft-fail with a hint; asking again opens the Login screen.
    fn open_playlists_as(&mut self, logged_in: bool) {
        if commands::disabled(commands::playlists::KEY) {
            return;
        }
        if commands::requires_login(commands::playlists::KEY) && !logged_in {
            if self.login_prompted {
                self.login_prompted = false;
                self.status.clear();
                self.screen = Screen::Login;
            } else {
                self.login_prompted = true;
                self.status = LOGIN_HINT.into();
            }
            return;
        }
        self.login_prompted = false;
        self.screen = Screen::Playlists;
        self.select(0);
        self.spawn(async { Ok(Msg::Playlists(youtube::playlists().await?)) });
    }

    /// Apply finished background work and detect the end of the current track. Called every frame.
    pub fn tick(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            if !matches!(m, Msg::LoginUrl(_)) {
                self.busy = false;
            }
            match m {
                Msg::Online => self.on_event(Event::Connection(true)),
                Msg::Offline => self.on_event(Event::Connection(false)),
                Msg::Err(e) => {
                    self.status = e;
                    self.login_url = None;
                }
                Msg::LoginUrl(u) => self.login_url = Some(u),
                Msg::LoggedIn => {
                    self.login_url = None;
                    self.status = "signed in".into();
                    self.open_playlists();
                }
                Msg::Tracks(v) => {
                    self.from_playlist = false;
                    self.status = if v.is_empty() {
                        "no results".into()
                    } else {
                        String::new()
                    };
                    self.results = v;
                    self.screen = Screen::Results;
                    self.select(0);
                }
                Msg::Related(id, v) => {
                    if self.queue.current().is_some_and(|t| t.id == id) && !v.is_empty() {
                        self.queue
                            .set_upcoming(v.iter().filter(|t| t.id != id).cloned().collect());
                        self.results = v;
                        self.screen = Screen::Results;
                        self.select(0);
                    }
                }
                Msg::Playlists(v) => self.playlists = v,
                // Ignored if the user left Playlists or moved the selection while it loaded.
                Msg::PlaylistTracks(id, v) => {
                    let current = self
                        .playlists
                        .get(self.selected)
                        .is_some_and(|p| p.id == id);
                    if self.screen == Screen::Playlists && current && self.load_playlist(v) {
                        self.play_current();
                    }
                }
            }
        }
        match self.player.tick() {
            Some(true) => self.on_event(Event::TrackEnded),
            Some(false) => self.on_event(Event::TrackFailed),
            None => {}
        }
    }

    /// Apply a playback or connectivity event: the one place they change state.
    pub fn on_event(&mut self, e: Event) {
        match e {
            Event::TrackStarted(id) => {
                if let Some(t) = self.queue.current().filter(|t| t.id == id).cloned() {
                    crate::history::push(&mut self.history, t);
                    self.save_history();
                    if self.screen == Screen::History {
                        self.select(0); // the playing track just moved to the top
                    }
                }
                if !matches!(self.screen, Screen::Playlists | Screen::History)
                    && let Some(i) = self.results.iter().position(|r| r.id == id)
                {
                    self.select(i);
                }
            }
            Event::TrackEnded => self.next(),
            Event::TrackFailed => {
                self.status =
                    "playback failed — track unavailable, or mpv/yt-dlp missing (n = next)".into()
            }
            Event::Connection(true) => self.screen = Screen::Search,
            Event::Connection(false) => self.screen = Screen::Offline,
        }
    }

    /// Perform a playback command (from a key today; media keys later). Errors go to the status bar.
    pub fn dispatch(&mut self, c: Cmd) {
        let r = match c {
            Cmd::PlayPause => self.player.toggle_pause(),
            // Advances even on loop-one: the user asked for another track.
            Cmd::Next => {
                self.queue.skip();
                self.play_current();
                Ok(())
            }
            Cmd::Prev => {
                if self.queue.prev().is_some() {
                    self.play_current();
                }
                Ok(())
            }
            Cmd::Seek(s) => self.player.seek(s),
            Cmd::Volume(d) => {
                let r = self.player.change_volume(d);
                self.settings.volume = self.player.volume;
                self.settings.save();
                r
            }
            Cmd::ToggleShuffle => {
                self.queue.toggle_shuffle();
                Ok(())
            }
            Cmd::CycleLoop => {
                self.queue.looping = self.queue.looping.cycle();
                self.player.set_loop_file(self.queue.looping == Loop::One)
            }
        };
        if let Err(e) = r {
            self.status = e.to_string();
        }
    }

    /// Show a playlist and make it the queue, in order. False (nothing changed) if it is empty.
    fn load_playlist(&mut self, tracks: Vec<Track>) -> bool {
        if tracks.is_empty() {
            self.status = "playlist is empty".into();
            return false;
        }
        self.status.clear();
        self.from_playlist = true;
        self.queue.set(tracks.clone(), 0);
        self.results = tracks;
        self.screen = Screen::Results;
        self.select(0);
        true
    }

    fn play_selected(&mut self) {
        self.queue.set(self.results.clone(), self.selected);
        self.play_current();
    }

    /// Play the queue's current track (stop if there is none) and highlight it in the list.
    fn play_current(&mut self) {
        let Some(t) = self.queue.current().cloned() else {
            self.player.stop();
            return;
        };
        match self.player.play(&t, self.queue.looping == Loop::One) {
            Ok(()) => self.on_event(Event::TrackStarted(t.id)),
            Err(e) => self.status = e.to_string(),
        }
    }

    /// Queue the highlighted result without starting playback.
    fn queue_selected(&mut self) {
        if let Some(t) = self.results.get(self.selected).cloned() {
            self.status = format!("queued: {}", t.title);
            self.queue.add(t);
        }
    }

    /// Track ended: advance the queue (repeats on loop-one).
    fn next(&mut self) {
        self.queue.next();
        self.play_current();
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if !matches!(key.code, KeyCode::Char('P') | KeyCode::F(2)) && self.login_prompted {
            self.login_prompted = false; // the hint expires on any other key
            if self.status == LOGIN_HINT {
                self.status.clear();
            }
        }
        match self.screen {
            Screen::Offline => match key.code {
                KeyCode::Char('r') => self.spawn(async { Ok(Msg::Online) }),
                KeyCode::Char('q') => self.quit = true,
                _ => {}
            },
            Screen::Search => match key.code {
                KeyCode::Enter if !self.input.trim().is_empty() && !self.busy => self.run_search(),
                KeyCode::Char(c) => self.input.push(c),
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Esc | KeyCode::Tab if !self.results.is_empty() => {
                    self.screen = Screen::Results
                }
                KeyCode::Esc => self.quit = true,
                KeyCode::F(1) => self.open_help(),
                KeyCode::F(2) => self.open_playlists(),
                KeyCode::F(3) => self.open(Screen::Settings),
                KeyCode::F(4) => self.open(Screen::History),
                _ => {}
            },
            Screen::Login => match key.code {
                KeyCode::Enter if !self.busy => self.start_login(),
                KeyCode::Esc => self.screen = Screen::Search,
                KeyCode::Char('q') => self.quit = true,
                KeyCode::Char('?') => self.open_help(),
                _ => {}
            },
            Screen::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('?' | 'q')) {
                    self.close_overlay();
                }
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        self.help_scroll = self.help_scroll.saturating_add(1)
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.help_scroll = self.help_scroll.saturating_sub(1)
                    }
                    KeyCode::PageDown => self.help_scroll = self.help_scroll.saturating_add(10),
                    KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(10),
                    KeyCode::Char('g') | KeyCode::Home => self.help_scroll = 0,
                    _ => {}
                }
            }
            Screen::Settings => self.handle_settings_key(key),
            Screen::Results | Screen::Playlists | Screen::History => self.handle_list_key(key),
        }
    }

    fn save_history(&self) {
        if let Some(p) = self.settings.history_file() {
            crate::history::save(&p, &self.history);
        }
    }

    /// Open Help, Settings or History over the current screen (Esc comes back).
    fn open(&mut self, s: Screen) {
        if self.screen == s {
            return;
        }
        if !self.screen.is_overlay() {
            self.prev = self.screen;
            self.saved_sel = (self.selected, self.offset);
        }
        self.screen = s;
        match s {
            Screen::Help => self.help_scroll = 0,
            Screen::Settings => self.setting_row = 0,
            Screen::History => {
                self.offset = 0;
                self.select(0);
            }
            _ => {}
        }
    }

    fn open_help(&mut self) {
        self.open(Screen::Help);
    }

    /// Leave an overlay for the screen it was opened from, restoring that list's selection.
    fn close_overlay(&mut self) {
        self.screen = self.prev;
        self.offset = self.saved_sel.1;
        self.select(self.saved_sel.0);
    }

    fn handle_settings_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::F(3) | KeyCode::Char(commands::settings::KEY) => {
                self.close_overlay()
            }
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') | KeyCode::F(1) => self.open_help(),
            KeyCode::Char('j') | KeyCode::Down => {
                self.setting_row = (self.setting_row + 1).min(SETTING_ROWS - 1)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.setting_row = self.setting_row.saturating_sub(1)
            }
            KeyCode::Left | KeyCode::Char('-') => self.dispatch(Cmd::Volume(-cmd::STEP)),
            KeyCode::Right | KeyCode::Char('+') => self.dispatch(Cmd::Volume(cmd::STEP)),
            KeyCode::Enter if self.setting_row == 2 => {
                self.history.clear();
                self.save_history();
                self.status = "history cleared".into();
            }
            _ => {}
        }
    }

    fn handle_list_key(&mut self, key: KeyEvent) {
        if let KeyCode::Char(c) = key.code
            && commands::disabled(c)
        {
            return;
        }
        if let Some(c) = cmd::from_key(key) {
            self.dispatch(c);
            return;
        }
        let last = self.len().saturating_sub(1);
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.select(self.selected.saturating_add(1)),
            KeyCode::Char('k') | KeyCode::Up => self.select(self.selected.saturating_sub(1)),
            KeyCode::PageDown => self.select(self.selected.saturating_add(self.page)),
            KeyCode::PageUp => self.select(self.selected.saturating_sub(self.page)),
            KeyCode::Char('g') | KeyCode::Home => self.select(0),
            KeyCode::Char('G') | KeyCode::End => self.select(last),
            KeyCode::Enter if self.screen == Screen::Playlists => {
                if self.busy {
                    return; // never fall through to playing a stale search result
                }
                if let Some(p) = self.playlists.get(self.selected) {
                    let id = p.id.clone();
                    self.spawn(async move {
                        Ok(Msg::PlaylistTracks(
                            id.clone(),
                            youtube::playlist_items(&id).await?,
                        ))
                    });
                }
            }
            KeyCode::Enter if self.screen == Screen::History => {
                // Replay from here; the queue follows history order.
                self.queue.set(self.history.clone(), self.selected);
                self.play_current();
            }
            KeyCode::Enter => {
                self.play_selected();
                if !self.from_playlist
                    && let Some(id) = self.queue.current().map(|t| t.id.clone())
                {
                    self.spawn(async move {
                        let v = youtube::related(&id).await?;
                        Ok(Msg::Related(id, v))
                    });
                }
            }
            KeyCode::Char('a') if self.screen == Screen::Results => self.queue_selected(),
            KeyCode::Char('P') | KeyCode::F(2) => self.open_playlists(),
            KeyCode::Esc | KeyCode::Char(commands::history::KEY) | KeyCode::F(4)
                if self.screen == Screen::History =>
            {
                self.close_overlay()
            }
            KeyCode::Char(commands::settings::KEY) | KeyCode::F(3) => self.open(Screen::Settings),
            KeyCode::Char(commands::history::KEY) | KeyCode::F(4) => self.open(Screen::History),
            KeyCode::Char('/') | KeyCode::Esc => self.screen = Screen::Search,
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Char('q') => self.quit = true,
            _ => {}
        }
    }

    /// Length of the list shown on the current screen.
    pub fn len(&self) -> usize {
        match self.screen {
            Screen::Playlists => self.playlists.len(),
            Screen::History => self.history.len(),
            _ => self.results.len(),
        }
    }

    /// Move selection (clamped) and keep it within the visible window.
    pub fn select(&mut self, idx: usize) {
        self.selected = idx.min(self.len().saturating_sub(1));
        let page = self.page.max(1);
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + page {
            self.offset = self.selected + 1 - page;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut a = App::new();
        a.screen = Screen::Results;
        a.results = (0..30)
            .map(|i| Track {
                title: i.to_string(),
                channel: String::new(),
                id: i.to_string(),
            })
            .collect();
        a.page = 5;
        a
    }

    #[test]
    fn selection_clamps_and_scrolls() {
        let mut a = app();
        a.select(100);
        assert_eq!(a.selected, 29);
        assert_eq!(a.offset, 25);
        a.select(0);
        assert_eq!((a.selected, a.offset), (0, 0));
    }

    #[test]
    fn messages_drive_screens() {
        let mut a = App::new();
        a.tx.send(Msg::Offline).unwrap();
        a.tick();
        assert_eq!(a.screen, Screen::Offline);
        a.tx.send(Msg::Online).unwrap();
        a.tick();
        assert_eq!(a.screen, Screen::Search); // guests are never forced to log in
        a.tx.send(Msg::Tracks(Vec::new())).unwrap();
        a.tick();
        assert_eq!(
            (a.screen, a.status.as_str()),
            (Screen::Results, "no results")
        );
    }

    #[test]
    fn related_replaces_list_unless_stale() {
        let t = |id: &str| Track {
            title: id.into(),
            channel: String::new(),
            id: id.into(),
        };
        let mut a = app();
        a.queue.set(vec![t("x")], 0);
        a.tx.send(Msg::Related("old".into(), vec![t("o")])).unwrap();
        a.tick();
        assert_eq!(a.results.len(), 30); // stale: ignored
        a.tx.send(Msg::Related("x".into(), vec![t("x"), t("y")]))
            .unwrap();
        a.tick();
        assert_eq!((a.results.len(), a.selected), (2, 0));
        assert_eq!(a.queue.skip().map(|t| t.id.as_str()), Some("y")); // current not re-queued
    }

    #[test]
    fn next_stops_at_end_of_list() {
        let mut a = app();
        a.queue.set(a.results.clone(), 29);
        a.select(29);
        a.next();
        assert_eq!(a.selected, 29);
        assert!(a.queue.current().is_none() && a.player.now_playing.is_none());
    }

    #[test]
    fn a_queues_selected_without_playing() {
        let mut a = app();
        press(&mut a, KeyCode::Char('j'));
        press(&mut a, KeyCode::Char('a'));
        assert_eq!(a.status, "queued: 1");
        assert!(a.queue.current().is_none() && a.player.now_playing.is_none());
        assert_eq!(a.queue.upcoming().count(), 1);
        a.results.clear();
        press(&mut a, KeyCode::Char('a')); // nothing selected: no panic, no change
        assert_eq!(a.queue.upcoming().count(), 1);
        a.screen = Screen::Playlists;
        press(&mut a, KeyCode::Char('a')); // ignored on playlists
        assert_eq!(a.queue.upcoming().count(), 1);
    }

    fn press(a: &mut App, c: KeyCode) {
        a.handle_key(KeyEvent::new(c, KeyModifiers::NONE));
    }

    #[test]
    fn search_typing_and_esc() {
        let mut a = App::new();
        a.screen = Screen::Search;
        a.results.clear();
        for c in "pq".chars() {
            press(&mut a, KeyCode::Char(c)); // command letters are plain text here
        }
        assert_eq!(a.input, "pq");
        a.results = vec![Track {
            title: "t".into(),
            channel: String::new(),
            id: "i".into(),
        }];
        press(&mut a, KeyCode::Esc);
        assert_eq!((a.screen, a.quit), (Screen::Results, false));
    }

    #[test]
    fn list_keys_and_help_roundtrip() {
        let mut a = app();
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(a.selected, 1);
        press(&mut a, KeyCode::Char('s'));
        assert!(a.queue.shuffle);
        for want in [Loop::All, Loop::One, Loop::Off] {
            press(&mut a, KeyCode::Char('l'));
            assert_eq!(a.queue.looping, want);
        }
        press(&mut a, KeyCode::Char('?'));
        assert_eq!(a.screen, Screen::Help);
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.screen, Screen::Results);
        press(&mut a, KeyCode::Char('q'));
        assert!(a.quit);
    }

    #[test]
    fn logged_out_playlists_hint_then_login() {
        let mut a = app();
        a.open_playlists_as(false);
        assert_eq!((a.screen, a.status.as_str()), (Screen::Results, LOGIN_HINT));
        assert!(a.login_prompted);
        a.open_playlists_as(false); // asking again opens the Login screen
        assert_eq!((a.screen, a.status.as_str()), (Screen::Login, ""));
        assert!(!a.login_prompted);
    }

    #[test]
    fn playlist_replaces_queue_in_order() {
        let mut a = app();
        a.screen = Screen::Playlists;
        let v: Vec<Track> = ["a", "b", "c"]
            .map(|id| Track {
                title: id.into(),
                channel: String::new(),
                id: id.into(),
            })
            .into();
        assert!(a.load_playlist(v));
        assert_eq!(
            (a.screen, a.results.len(), a.selected),
            (Screen::Results, 3, 0)
        );
        assert_eq!(a.queue.current().map(|t| t.id.as_str()), Some("a"));
        assert_eq!(a.queue.upcoming().count(), 2);
        assert!(a.from_playlist); // Enter on a track won't swap in the related mix
        a.screen = Screen::Playlists;
        a.playlists = a.results[..1].to_vec();
        a.playlists[0].id = "pl".into();
        a.tx.send(Msg::PlaylistTracks("pl".into(), Vec::new()))
            .unwrap(); // empty: queue untouched
        a.tick();
        assert_eq!(
            (a.status.as_str(), a.queue.upcoming().count()),
            ("playlist is empty", 2)
        );
    }

    #[test]
    fn f2_on_search_asks_for_login_but_p_is_text() {
        let mut a = App::new();
        a.screen = Screen::Search;
        press(&mut a, KeyCode::Char('P'));
        assert_eq!((a.input.as_str(), a.screen), ("P", Screen::Search));
        a.input.clear();
        a.logged_in = Some(false);
        press(&mut a, KeyCode::F(2));
        assert_eq!((a.status.as_str(), a.screen), (LOGIN_HINT, Screen::Search));
        press(&mut a, KeyCode::F(2));
        assert_eq!(a.screen, Screen::Login);
    }

    #[test]
    fn stale_playlist_reply_ignored_and_tracks_reset_flag() {
        let t = |id: &str| Track {
            title: id.into(),
            channel: String::new(),
            id: id.into(),
        };
        let mut a = app();
        a.screen = Screen::Search;
        a.tx.send(Msg::PlaylistTracks("pl".into(), vec![t("a")]))
            .unwrap();
        a.tick();
        assert!(a.queue.current().is_none() && !a.from_playlist);
        assert_eq!(a.results.len(), 30);
        a.screen = Screen::Playlists; // right screen, but another playlist is selected
        a.playlists = vec![t("other")];
        a.tx.send(Msg::PlaylistTracks("pl".into(), vec![t("a")]))
            .unwrap();
        a.tick();
        assert!(a.queue.current().is_none());
        a.from_playlist = true;
        a.tx.send(Msg::Tracks(vec![t("x")])).unwrap(); // a search reply clears the flag
        a.tick();
        assert!(!a.from_playlist);
    }

    #[test]
    fn login_hint_expires_on_other_keys() {
        let mut a = app();
        a.logged_in = Some(false);
        press(&mut a, KeyCode::Char('P'));
        assert!(a.login_prompted);
        press(&mut a, KeyCode::Char('j'));
        assert!(!a.login_prompted && a.status.is_empty());
        press(&mut a, KeyCode::Char('P'));
        assert_eq!(a.screen, Screen::Results); // hint again, not Login
    }

    #[test]
    fn dispatch_drives_queue() {
        // Only paths that never start mpv: Queue::prev's moves are tested in queue.rs.
        let mut a = app();
        a.queue.set(a.results.clone(), 29);
        a.dispatch(Cmd::Prev); // at the start: nothing changes
        assert_eq!(a.queue.current().map(|t| t.id.as_str()), Some("29"));
        a.dispatch(Cmd::Next); // last track: the queue ends
        assert!(a.queue.current().is_none() && a.player.now_playing.is_none());
        a.dispatch(Cmd::ToggleShuffle);
        a.dispatch(Cmd::CycleLoop);
        assert!(a.queue.shuffle && a.queue.looping == Loop::All);
    }

    #[test]
    fn events_drive_state() {
        let mut a = app();
        a.on_event(Event::TrackStarted("7".into()));
        assert_eq!(a.selected, 7);
        a.on_event(Event::TrackStarted("unknown".into()));
        assert_eq!(a.selected, 7);
        a.screen = Screen::Playlists; // the playlist list keeps its own selection
        a.on_event(Event::TrackStarted("3".into()));
        assert_eq!(a.selected, 7);
        a.screen = Screen::Results;
        a.on_event(Event::TrackFailed);
        assert!(a.status.starts_with("playback failed"));
        a.on_event(Event::Connection(false));
        assert_eq!(a.screen, Screen::Offline);
        a.on_event(Event::Connection(true));
        assert_eq!(a.screen, Screen::Search);
        a.queue.set(a.results.clone(), 29);
        a.on_event(Event::TrackEnded); // last track: queue ends
        assert!(a.queue.current().is_none());
    }

    #[test]
    fn busy_enter_on_playlists_does_not_play_results() {
        let mut a = app();
        a.screen = Screen::Playlists;
        a.busy = true;
        press(&mut a, KeyCode::Enter);
        assert!(a.queue.current().is_none());
    }

    #[test]
    fn search_tab_and_esc_return_to_results_or_quit() {
        let mut a = app();
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.screen, Screen::Search);
        press(&mut a, KeyCode::Tab);
        assert_eq!(a.screen, Screen::Results);
        press(&mut a, KeyCode::Char('/'));
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.screen, Screen::Results);
        a.screen = Screen::Search;
        a.results.clear();
        press(&mut a, KeyCode::Tab);
        assert_eq!((a.screen, a.quit), (Screen::Search, false));
        press(&mut a, KeyCode::Esc);
        assert!(a.quit);
    }

    #[test]
    fn overlays_return_to_prior_screen_and_selection() {
        let mut a = app();
        a.select(7);
        for (key, screen) in [
            (KeyCode::Char('S'), Screen::Settings),
            (KeyCode::F(3), Screen::Settings),
            (KeyCode::Char('H'), Screen::History),
            (KeyCode::F(4), Screen::History),
            (KeyCode::Char('?'), Screen::Help),
        ] {
            press(&mut a, key);
            assert_eq!(a.screen, screen);
            press(&mut a, KeyCode::Esc);
            assert_eq!((a.screen, a.selected), (Screen::Results, 7), "{key:?}");
        }
        // Toggle keys close their own screen; Help opened from Settings still returns to the list.
        press(&mut a, KeyCode::Char('H'));
        press(&mut a, KeyCode::Char('H'));
        assert_eq!(a.screen, Screen::Results);
        press(&mut a, KeyCode::Char('S'));
        press(&mut a, KeyCode::Char('?'));
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.screen, Screen::Results);
        // From Search the F-keys open them and Esc goes back to Search.
        a.screen = Screen::Search;
        press(&mut a, KeyCode::F(4));
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.screen, Screen::Search);
    }

    #[test]
    fn help_scrolls_and_resets() {
        let mut a = app();
        press(&mut a, KeyCode::Char('?'));
        press(&mut a, KeyCode::Char('j'));
        press(&mut a, KeyCode::PageDown);
        assert_eq!(a.help_scroll, 11);
        press(&mut a, KeyCode::Char('k'));
        press(&mut a, KeyCode::PageUp);
        assert_eq!(a.help_scroll, 0);
        press(&mut a, KeyCode::Down);
        press(&mut a, KeyCode::Char('g'));
        assert_eq!(a.help_scroll, 0);
        press(&mut a, KeyCode::Down);
        press(&mut a, KeyCode::Esc);
        press(&mut a, KeyCode::Char('?'));
        assert_eq!(a.help_scroll, 0);
    }

    fn track(id: &str) -> Track {
        Track {
            title: id.into(),
            channel: "c".into(),
            id: id.into(),
        }
    }

    #[test]
    fn track_started_records_history_most_recent_first() {
        let mut a = app();
        a.history.clear();
        for i in ["1", "2", "1"] {
            a.queue.set(a.results.clone(), i.parse().unwrap());
            a.on_event(Event::TrackStarted(i.into()));
        }
        let ids: Vec<_> = a.history.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["1", "2"]);
        a.on_event(Event::TrackStarted("not-current".into())); // not the queue's track: ignored
        assert_eq!(a.history.len(), 2);
    }

    #[test]
    fn history_screen_lists_and_enter_queues_from_selection() {
        let mut a = app();
        a.history = ["h0", "h1", "h2"].map(track).into();
        press(&mut a, KeyCode::Char('H'));
        assert_eq!((a.len(), a.selected), (3, 0));
        press(&mut a, KeyCode::Char('j'));
        press(&mut a, KeyCode::Enter);
        // Playing h1 moves it to the top; the queue keeps the old order after it.
        assert_eq!(a.queue.current().map(|t| t.id.as_str()), Some("h1"));
        assert_eq!(a.queue.upcoming().next().map(|t| t.id.as_str()), Some("h2"));
        assert_eq!(a.history[0].id, "h1");
        assert_eq!((a.screen, a.selected), (Screen::History, 0));
        a.player.stop();
    }

    #[test]
    fn settings_volume_and_clear_history() {
        let mut a = app();
        a.history = vec![track("x")];
        a.player.volume = 50;
        press(&mut a, KeyCode::Char('S'));
        press(&mut a, KeyCode::Right);
        press(&mut a, KeyCode::Char('+'));
        assert_eq!((a.player.volume, a.settings.volume), (60, 60));
        press(&mut a, KeyCode::Left);
        press(&mut a, KeyCode::Char('-'));
        press(&mut a, KeyCode::Char('-'));
        assert_eq!(a.player.volume, 45);
        press(&mut a, KeyCode::Enter); // row 0: nothing cleared
        assert_eq!(a.history.len(), 1);
        for _ in 0..5 {
            press(&mut a, KeyCode::Char('j')); // clamps at the last row
        }
        assert_eq!(a.setting_row, SETTING_ROWS - 1);
        press(&mut a, KeyCode::Enter);
        assert!(a.history.is_empty());
        press(&mut a, KeyCode::Char('k'));
        assert_eq!(a.setting_row, 1);
        press(&mut a, KeyCode::Char('q'));
        assert!(a.quit);
    }

    #[test]
    fn screen_overlay_flags() {
        assert!(Screen::Help.is_overlay() && Screen::History.is_overlay());
        assert!(!Screen::Results.is_overlay() && !Screen::Search.is_overlay());
    }
}
