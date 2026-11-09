//! Application state and key handling.
use crate::commands;
use crate::player::Player;
use crate::queue::{Loop, Queue};
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
    /// Items of the playlist the user opened: they become the queue and start playing.
    PlaylistTracks(Vec<Track>),
}

const LOGIN_HINT: &str = "Playlists need you to be logged in — press P again to sign in";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Login,
    Search,
    Results,
    Playlists,
    Help,
    Offline,
}

pub struct App {
    pub screen: Screen,
    /// Screen to return to when leaving help.
    pub prev: Screen,
    pub input: String,
    pub results: Vec<Track>,
    pub playlists: Vec<Track>,
    /// `results` is an opened playlist (keeps its order; no related lookup on play).
    from_playlist: bool,
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
        Self {
            playlists: Vec::new(),
            from_playlist: false,
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
            player: Player::default(),
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
        self.from_playlist = false;
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
        self.open_playlists_as(auth::is_logged_in());
    }

    /// Logged out: soft-fail with a hint; asking again while the hint shows opens the Login screen.
    fn open_playlists_as(&mut self, logged_in: bool) {
        if commands::disabled(commands::playlists::KEY) {
            return;
        }
        if commands::requires_login(commands::playlists::KEY) && !logged_in {
            if self.status == LOGIN_HINT {
                self.status.clear();
                self.screen = Screen::Login;
            } else {
                self.status = LOGIN_HINT.into();
            }
            return;
        }
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
                Msg::Online => self.screen = Screen::Search,
                Msg::Offline => self.screen = Screen::Offline,
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
                Msg::PlaylistTracks(v) => {
                    if self.load_playlist(v) {
                        self.play_current();
                    }
                }
            }
        }
        match self.player.tick() {
            Some(true) => self.next(),
            Some(false) => {
                self.status =
                    "playback failed — track unavailable, or mpv/yt-dlp missing (n = next)".into()
            }
            None => {}
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
        if self.screen != Screen::Playlists
            && let Some(i) = self.results.iter().position(|r| r.id == t.id)
        {
            self.select(i);
        }
        if let Err(e) = self.player.play(&t, self.queue.looping == Loop::One) {
            self.status = e.to_string();
        }
    }

    /// Player command failures go to the status bar.
    fn show_err(&mut self, r: anyhow::Result<()>) {
        if let Err(e) = r {
            self.status = e.to_string();
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

    /// User asked for the next track: advance even on loop-one.
    fn skip(&mut self) {
        self.queue.skip();
        self.play_current();
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
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
                KeyCode::Esc if !self.results.is_empty() => self.screen = Screen::Results,
                KeyCode::Esc => self.quit = true,
                KeyCode::F(1) => self.open_help(),
                KeyCode::F(2) => self.open_playlists(),
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
                    self.screen = self.prev;
                }
            }
            Screen::Results | Screen::Playlists => self.handle_list_key(key),
        }
    }

    fn open_help(&mut self) {
        self.prev = self.screen;
        self.screen = Screen::Help;
    }

    fn handle_list_key(&mut self, key: KeyEvent) {
        if let KeyCode::Char(c) = key.code
            && commands::disabled(c)
        {
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
                if let Some(p) = self.playlists.get(self.selected) {
                    let id = p.id.clone();
                    self.spawn(async move {
                        Ok(Msg::PlaylistTracks(youtube::playlist_items(&id).await?))
                    });
                }
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
            KeyCode::Char('p') => {
                let r = self.player.toggle_pause();
                self.show_err(r);
            }
            KeyCode::Char('n') => self.skip(),
            KeyCode::Char('a') if self.screen == Screen::Results => self.queue_selected(),
            KeyCode::Char(c @ ('+' | '-')) => {
                let r = self.player.volume(if c == '+' { 5 } else { -5 });
                self.show_err(r);
            }
            KeyCode::Char(c @ (',' | '.')) => {
                let r = self.player.seek(if c == '.' { 5 } else { -5 });
                self.show_err(r);
            }
            KeyCode::Char('s') => self.queue.toggle_shuffle(),
            KeyCode::Char('l') => {
                self.queue.looping = self.queue.looping.cycle();
                let r = self.player.set_loop_file(self.queue.looping == Loop::One);
                self.show_err(r);
            }
            KeyCode::Char('P') | KeyCode::F(2) => self.open_playlists(),
            KeyCode::Char('/') | KeyCode::Esc => self.screen = Screen::Search,
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Char('q') => self.quit = true,
            _ => {}
        }
    }

    /// Length of the list shown on the current screen.
    pub fn len(&self) -> usize {
        if self.screen == Screen::Playlists {
            self.playlists.len()
        } else {
            self.results.len()
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
        a.open_playlists_as(false); // asking again opens the Login screen
        assert_eq!((a.screen, a.status.as_str()), (Screen::Login, ""));
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
        a.tx.send(Msg::PlaylistTracks(Vec::new())).unwrap(); // empty: queue untouched
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
        a.status.clear();
        a.open_playlists_as(false);
        assert_eq!(a.status, LOGIN_HINT);
    }
}
