//! Application state and key handling.
use crate::anim::{Anim, Key};
use crate::cmd::{self, Cmd, Event};
use crate::commands;
use crate::history::{self, Entry};
use crate::player::Player;
use crate::queue::{Loop, Queue};
use crate::settings::Settings;
use crate::theme::{self, Shell};
use crate::youtube::{self, Track};
use crate::{auth, net};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;
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
    /// A background file write failed.
    SaveFailed(String),
}

/// Serializes background writes so they land in the order they were requested.
static IO: Mutex<()> = Mutex::new(());

/// Fire-and-forget file work off the UI thread. Without a tokio runtime (unit tests) it runs inline.
fn bg(f: impl FnOnce() + Send + 'static) {
    let run = move || {
        let _g = IO.lock().unwrap_or_else(|e| e.into_inner());
        f()
    };
    match tokio::runtime::Handle::try_current() {
        Ok(h) => {
            h.spawn_blocking(run);
        }
        Err(_) => run(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// The Lists tab while signed out: the sign-in prompt.
    Login,
    Results,
    Queue,
    Playlists,
    Help,
    Settings,
    History,
    Offline,
}

/// The J-card tabs, in strip order (`1`-`4`).
pub const TABS: [Screen; 4] = [
    Screen::Results,
    Screen::Queue,
    Screen::Playlists,
    Screen::History,
];

impl Screen {
    /// Screens opened on top of a tab; Esc returns to the screen below on `App::stack`.
    fn is_overlay(self) -> bool {
        matches!(self, Self::Help | Self::Settings)
    }

    /// Position in `TABS`; Login is the signed-out Lists tab.
    pub fn tab_index(self) -> Option<usize> {
        match self {
            Self::Login => Some(2),
            s => TABS.iter().position(|&t| t == s),
        }
    }
}

/// Rows on the settings screen, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Volume,
    Shell,
    Animations,
    Hiss,
    HissLevel,
    HistoryFile,
    ClearHistory,
}

impl Row {
    pub const ALL: [Row; 7] = [
        Row::Volume,
        Row::Shell,
        Row::Animations,
        Row::Hiss,
        Row::HissLevel,
        Row::HistoryFile,
        Row::ClearHistory,
    ];
}

pub struct App {
    pub screen: Screen,
    /// Screens under the current overlay, oldest first; Esc pops one.
    stack: Vec<Screen>,
    pub input: String,
    /// The query of the search in flight (trimmed).
    submitted: String,
    pub results: Vec<Track>,
    pub playlists: Vec<Track>,
    /// `results` is an opened playlist (keeps its order; no related lookup on play).
    from_playlist: bool,
    /// Test hook: overrides the on-disk login state.
    #[cfg(test)]
    logged_in: Option<bool>,
    /// Authorization page while waiting for the user to sign in.
    pub login_url: Option<String>,
    /// A network task is in flight.
    pub busy: bool,
    tx: UnboundedSender<Msg>,
    rx: UnboundedReceiver<Msg>,
    /// The search field has focus (typing goes there); the tab stays what it was.
    pub searching: bool,
    /// Selection in the active tab's list.
    pub selected: usize,
    /// First visible row of the active tab's list.
    pub offset: usize,
    /// (selected, offset) of each tab in `TABS` while it is not active.
    tab_sel: [(usize, usize); 4],
    /// Visible rows in the results list; updated by `ui` each frame.
    pub page: usize,
    pub player: Player,
    pub queue: Queue,
    pub status: String,
    pub quit: bool,
    pub settings: Settings,
    /// Last played tracks, most recent first.
    pub history: Vec<Entry>,
    pub history_path: Option<PathBuf>,
    /// Highlighted row on the settings screen (index into `Row::ALL`).
    pub setting_row: usize,
    /// Clear history was asked for once; the next Enter confirms.
    pub confirm_clear: bool,
    /// Id of the track mpv is playing, to store its position in the history.
    playing_id: Option<String>,
    /// Where the next `play_current` starts, in seconds (history replay).
    resume_at: u64,
    /// First visible line of the help screen.
    pub help_scroll: u16,
    pub anim: Anim,
}

impl App {
    pub fn new() -> Self {
        let settings = Settings::load();
        let path = settings.history_file(
            std::env::var_os("TERMUSIC_HISTORY"),
            crate::settings::config_dir(),
        );
        Self::with_paths(settings, path)
    }

    /// Tests use this with no path so they never touch the real config.
    #[cfg(test)]
    pub(crate) fn blank() -> Self {
        let mut a = Self::with_paths(Settings::default(), None);
        a.player.mpv = "true".into(); // exits at once: tests never start a real mpv
        a
    }

    fn with_paths(settings: Settings, history_path: Option<PathBuf>) -> Self {
        // Login is optional (only playlists need it), so we always start typing a search.
        let screen = if net::is_online() {
            Screen::Results
        } else {
            Screen::Offline
        };
        let (tx, rx) = unbounded_channel();
        let mut player = Player::new(settings.volume);
        player.meters = settings.animations;
        player.hiss_on = settings.hiss;
        player.hiss_level = settings.hiss_level;
        Self {
            history: history_path
                .as_deref()
                .map(history::load)
                .unwrap_or_default(),
            history_path,
            confirm_clear: false,
            playing_id: None,
            resume_at: 0,
            player,
            settings,
            setting_row: 0,
            help_scroll: 0,
            anim: Anim::new(Instant::now()),
            playlists: Vec::new(),
            from_playlist: false,
            #[cfg(test)]
            logged_in: None,
            login_url: None,
            busy: false,
            tx,
            rx,
            screen,
            stack: Vec::new(),
            input: String::new(),
            submitted: String::new(),
            results: Vec::new(),
            searching: true,
            tab_sel: [(0, 0); 4],
            selected: 0,
            offset: 0,
            page: 10,
            queue: Queue::new(),
            status: crate::player::missing_deps().unwrap_or_default(),
            quit: false,
        }
    }

    /// The color shell in use.
    pub fn shell(&self) -> &'static Shell {
        theme::shell(&self.settings.shell)
    }

    /// Step through `theme::SHELLS` (wrapping) and save the choice.
    fn cycle_shell(&mut self, step: isize) {
        let n = theme::SHELLS.len() as isize;
        let i = (theme::shell_index(&self.settings.shell) as isize + step).rem_euclid(n);
        self.settings.shell = theme::SHELLS[i as usize].id.into();
        self.save_settings();
    }

    /// Turn animations on or off (Settings today, the control socket later) and save the choice.
    pub fn set_animations(&mut self, on: bool) {
        self.settings.animations = on;
        self.player.meters = on;
        self.save_settings();
    }

    /// Tape hiss on or off; it starts with the next playing frame and is saved.
    pub fn set_hiss(&mut self, on: bool) {
        self.settings.hiss = on;
        self.player.hiss_on = on;
        self.player.sync_hiss();
        self.save_settings();
    }

    /// Hiss level, 0..=100 (clamped); the running hiss follows at once.
    pub fn set_hiss_level(&mut self, level: u8) {
        self.player.set_hiss_level(level);
        self.settings.hiss_level = self.player.hiss_level;
        self.save_settings();
    }

    /// Something on screen moves, so the loop runs at the frame rate.
    pub fn animating(&self) -> bool {
        self.settings.animations && (self.player.playing || self.anim.busy())
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
        self.submitted = q.clone(); // the Tracks reply moves focus only if this is still the input
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

    /// The Lists tab: signed out it shows the sign-in prompt, else (re)loads the playlists.
    fn open_playlists_as(&mut self, logged_in: bool) {
        if commands::disabled(commands::playlists::KEY) {
            return;
        }
        if commands::requires_login(commands::playlists::KEY) && !logged_in {
            self.set_tab(Screen::Login);
            return;
        }
        self.set_tab(Screen::Playlists);
        self.spawn(async { Ok(Msg::Playlists(youtube::playlists().await?)) });
    }

    /// Apply finished background work and detect the end of the current track. Called every frame.
    pub fn tick(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            if !matches!(m, Msg::LoginUrl(_) | Msg::SaveFailed(_)) {
                self.busy = false;
            }
            // Replies never move focus: whoever is typing keeps typing.
            // The one exception: results for the unchanged submitted query hand focus to the list.
            let typing = self.searching;
            let mut focus_list = false;
            match m {
                Msg::Online => self.on_event(Event::Connection(true)),
                Msg::Offline => self.on_event(Event::Connection(false)),
                Msg::Err(e) => {
                    self.status = e;
                    self.login_url = None;
                }
                Msg::LoginUrl(u) => self.login_url = Some(u),
                Msg::SaveFailed(e) => self.status = e,
                Msg::LoggedIn => {
                    self.login_url = None;
                    self.status = "signed in".into();
                    self.open_playlists();
                }
                Msg::Tracks(v) => {
                    focus_list = !v.is_empty() && self.input.trim() == self.submitted;
                    self.from_playlist = false;
                    self.status = if v.is_empty() {
                        "no results".into()
                    } else {
                        String::new()
                    };
                    self.results = v;
                    self.set_tab(Screen::Results);
                    self.select(0);
                }
                Msg::Related(id, v) => {
                    if self.queue.current().is_some_and(|t| t.id == id) && !v.is_empty() {
                        self.queue
                            .set_upcoming(v.iter().filter(|t| t.id != id).cloned().collect());
                        self.results = v;
                        self.set_sel(Screen::Results, 0);
                        self.set_sel(Screen::Queue, self.sel_of(Screen::Queue)); // clamp
                    }
                }
                Msg::Playlists(v) => {
                    self.playlists = v;
                    self.set_sel(Screen::Playlists, self.sel_of(Screen::Playlists));
                }
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
            self.searching = typing && !focus_list;
        }
        self.sync_preload();
        match self.player.tick() {
            Some(true) => self.on_event(Event::TrackEnded),
            Some(false) => self.on_event(Event::TrackFailed),
            None => {}
        }
        let p = &self.player;
        let count = if p.now_playing.is_some() {
            p.position.max(0.0) as u64 % 1000
        } else {
            0
        };
        if p.now_playing.is_none() {
            self.anim.shown = None; // the deck is empty: the next tape has nothing to eject
        }
        let levels = p.levels().filter(|_| p.playing);
        self.anim.advance(Instant::now(), p.playing, count, levels);
    }

    /// Keep the background preload matching the queue: drop a stale one, start one near the end.
    fn sync_preload(&mut self) {
        let want = self.queue.peek_next().cloned();
        let loaded = self.player.preloaded_id();
        if loaded.is_some() && loaded != want.as_ref().map(|t| t.id.as_str()) {
            self.player.drop_preload(); // skip, prev, shuffle, loop or a new queue changed it
        }
        if let Some(t) = want.filter(|t| self.player.wants_preload(&t.id)) {
            self.player.preload(&t);
        }
    }

    /// Apply a playback or connectivity event: the one place they change state.
    pub fn on_event(&mut self, e: Event) {
        match e {
            Event::TrackStarted(id) => {
                self.playing_id = Some(id.clone());
                let title = self.player.now_playing.clone().unwrap_or_default();
                let channel = self.queue.current().filter(|t| t.id == id);
                let channel = channel.map_or(String::new(), |t| t.channel.clone());
                let animate = self.settings.animations;
                self.anim.swap_to((title, channel), animate, Instant::now());
                if let Some(t) = self.queue.current().filter(|t| t.id == id).cloned() {
                    history::push(&mut self.history, t);
                    self.save_history();
                }
                // The playing track is now the top row of History and Queue.
                self.set_sel(Screen::History, 0);
                self.set_sel(Screen::Queue, 0);
                if let Some(i) = self.results.iter().position(|r| r.id == id) {
                    self.set_sel(Screen::Results, i);
                }
            }
            Event::TrackEnded => self.next(),
            Event::TrackFailed => {
                self.status =
                    "playback failed — track unavailable, or mpv/yt-dlp missing (n = next)".into()
            }
            Event::Connection(true) => {
                self.set_tab(Screen::Results);
            }
            Event::Connection(false) => {
                self.save_sel();
                self.stack.clear();
                self.searching = false;
                self.screen = Screen::Offline;
            }
        }
    }

    /// Perform a playback command (from a key today; media keys later). Errors go to the status bar.
    pub fn dispatch(&mut self, c: Cmd) {
        if self.settings.animations && matches!(c, Cmd::Next | Cmd::Prev | Cmd::Seek(_)) {
            self.anim.whirr = Some(Instant::now());
        }
        let r = self.run(c);
        if let Err(e) = r {
            self.status = e.to_string();
        }
        let key = match c {
            Cmd::PlayPause if self.player.playing => Some(Key::Play),
            Cmd::PlayPause => Some(Key::Pause),
            Cmd::Next => Some(Key::Next),
            Cmd::Prev => Some(Key::Prev),
            Cmd::ToggleShuffle => Some(Key::Shuffle),
            Cmd::CycleLoop => Some(Key::Loop),
            Cmd::Seek(_) | Cmd::Volume(_) => None,
        };
        if let Some(k) = key.filter(|_| self.settings.animations) {
            self.anim.pressed = Some((k, Instant::now()));
        }
    }

    fn run(&mut self, c: Cmd) -> anyhow::Result<()> {
        match c {
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
                self.save_settings();
                r
            }
            Cmd::ToggleShuffle => {
                self.queue.toggle_shuffle();
                Ok(())
            }
            Cmd::CycleLoop => {
                self.queue.looping = self.queue.looping.cycle();
                self.status = match self.queue.looping {
                    Loop::One => "loop: this track",
                    Loop::All => "loop: whole queue",
                    Loop::Off => "loop: off",
                }
                .into();
                self.player.set_loop_file(self.queue.looping == Loop::One)
            }
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
        self.set_tab(Screen::Results);
        self.select(0);
        true
    }

    fn play_selected(&mut self) {
        if self.results.is_empty() {
            return; // set() would clear the queue and stop playback
        }
        self.queue.set(self.results.clone(), self.selected);
        self.play_current();
    }

    /// Play the queue's current track (stop if there is none) and highlight it in the list.
    fn play_current(&mut self) {
        self.record_position();
        let start = std::mem::take(&mut self.resume_at);
        let Some(t) = self.queue.current().cloned() else {
            self.player.stop();
            return;
        };
        let one = self.queue.looping == Loop::One;
        // A ready preload starts instantly; otherwise spawn cold.
        let r = if start == 0 && !one && self.player.promote(&t.id) {
            Ok(())
        } else {
            self.player.play(&t, one, start)
        };
        match r {
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
        if self.screen == Screen::Offline {
            match key.code {
                KeyCode::Char('r') => self.spawn(async { Ok(Msg::Online) }),
                KeyCode::Char('q') => self.quit = true,
                _ => {}
            }
            return;
        }
        if self.searching {
            return self.handle_search_key(key);
        }
        if key.code == KeyCode::Char(commands::search::KEY)
            && !commands::disabled(commands::search::KEY)
        {
            return self.focus_search();
        }
        match self.screen {
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
                    KeyCode::Char('G') | KeyCode::End => self.help_scroll = u16::MAX, // clamped by ui
                    _ => {}
                }
            }
            Screen::Settings => self.handle_settings_key(key),
            _ => self.handle_list_key(key),
        }
    }

    /// Typing in the search field: every letter is text, so only Enter, Esc/Tab and F-keys act.
    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter if !self.input.trim().is_empty() && !self.busy => self.run_search(),
            KeyCode::Char(c) => self.input.push(c),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Esc | KeyCode::Tab => self.searching = false,
            KeyCode::F(1) => self.open(Screen::Help),
            KeyCode::F(2) => self.open_playlists(),
            KeyCode::F(3) => self.open(Screen::Settings),
            KeyCode::F(4) => self.goto_tab(3, tabs_off()),
            _ => {}
        }
    }

    /// Focus the search field over the current tab (overlays close).
    fn focus_search(&mut self) {
        self.close_all();
        self.searching = true;
    }

    /// Remember where the playing track is, so History can resume it.
    fn record_position(&mut self) {
        let Some(id) = self.playing_id.take() else {
            return;
        };
        let pos = history::resume_point(self.player.position, self.player.duration);
        if let Some(e) = self.history.iter_mut().find(|e| e.track.id == id) {
            e.position_secs = pos;
        }
    }

    /// Save everything synchronously; called once on the way out.
    pub fn shutdown(&mut self) {
        let _io = IO.lock().unwrap_or_else(|e| e.into_inner()); // waits for a background write already running; one still queued may land after ours
        self.record_position();
        self.settings.volume = self.player.volume;
        self.settings.save();
        if let Some(p) = &self.history_path {
            let _ = history::save(p, &self.history);
        }
    }

    fn save_history(&self) {
        let Some(p) = self.history_path.clone() else {
            return;
        };
        let (h, tx) = (self.history.clone(), self.tx.clone());
        bg(move || {
            if let Err(e) = history::save(&p, &h) {
                let _ = tx.send(Msg::SaveFailed(format!("history not saved: {e}")));
            }
        });
    }

    fn save_settings(&self) {
        let s = self.settings.clone();
        bg(move || s.save());
    }

    /// Open Help or Settings over the current screen (Esc comes back).
    fn open(&mut self, s: Screen) {
        self.searching = false;
        if self.screen == s {
            return;
        }
        if !self.screen.is_overlay() {
            self.stack.clear();
        }
        // Reopening a screen already below us goes back to it instead of nesting.
        match self.stack.iter().position(|&x| x == s) {
            Some(i) => self.stack.truncate(i),
            None => self.stack.push(self.screen),
        }
        self.screen = s;
        match s {
            Screen::Help => self.help_scroll = 0,
            Screen::Settings => {
                self.setting_row = 0;
                self.confirm_clear = false;
            }
            _ => {}
        }
    }

    /// Leave an overlay for the screen under it.
    fn close_overlay(&mut self) {
        self.screen = self.stack.pop().unwrap_or(Screen::Results);
        self.select(self.selected); // the list may have shrunk under the overlay
    }

    /// Leave every overlay.
    fn close_all(&mut self) {
        while self.screen.is_overlay() {
            self.close_overlay();
        }
    }

    /// The tab shown in the J-card (the one under any overlay).
    pub fn tab(&self) -> Screen {
        if self.screen.is_overlay() {
            self.stack.first().copied().unwrap_or(Screen::Results)
        } else {
            self.screen
        }
    }

    /// Remember the active tab's selection so switching back restores it.
    fn save_sel(&mut self) {
        if let Some(i) = self.tab().tab_index() {
            self.tab_sel[i] = (self.selected, self.offset);
        }
    }

    /// Selected row of `tab`, active or not.
    fn sel_of(&self, tab: Screen) -> usize {
        match tab.tab_index() {
            _ if self.tab().tab_index() == tab.tab_index() => self.selected,
            Some(i) => self.tab_sel[i].0,
            None => 0,
        }
    }

    /// Select row `i` of `tab`, active or not (an inactive tab clamps when shown).
    fn set_sel(&mut self, tab: Screen, i: usize) {
        if self.tab() == tab {
            self.select(i);
        } else if let Some(t) = tab.tab_index() {
            self.tab_sel[t] = (i, 0);
        }
    }

    /// Show `tab` in the J-card with its own selection; overlays and search focus close.
    fn set_tab(&mut self, tab: Screen) {
        self.close_all();
        self.searching = false;
        self.save_sel();
        self.screen = tab;
        if let Some(i) = tab.tab_index() {
            (self.selected, self.offset) = self.tab_sel[i];
            self.select(self.selected);
        }
    }

    /// Switch to `TABS[i]` unless that tab is off; the Lists tab goes through the sign-in check.
    fn goto_tab(&mut self, i: usize, off: [bool; 4]) {
        if off[i % TABS.len()] {
            return;
        }
        match TABS[i % TABS.len()] {
            Screen::Playlists => self.open_playlists(),
            t => self.set_tab(t),
        }
    }

    fn handle_settings_key(&mut self, key: KeyEvent) {
        let confirmed = std::mem::take(&mut self.confirm_clear);
        let row = Row::ALL[self.setting_row];
        match key.code {
            KeyCode::Esc | KeyCode::F(3) | KeyCode::Char(commands::settings::KEY) => {
                self.close_overlay()
            }
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') | KeyCode::F(1) => self.open(Screen::Help),
            KeyCode::Char(commands::history::KEY) | KeyCode::F(4) => self.goto_tab(3, tabs_off()),
            KeyCode::Char('j') | KeyCode::Down => {
                self.setting_row = (self.setting_row + 1).min(Row::ALL.len() - 1)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.setting_row = self.setting_row.saturating_sub(1)
            }
            KeyCode::Enter | KeyCode::Left | KeyCode::Right if row == Row::Animations => {
                self.set_animations(!self.settings.animations)
            }
            KeyCode::Enter | KeyCode::Left | KeyCode::Right if row == Row::Hiss => {
                self.set_hiss(!self.settings.hiss)
            }
            KeyCode::Left | KeyCode::Char('-') if row == Row::HissLevel => {
                self.set_hiss_level(self.settings.hiss_level.saturating_sub(cmd::STEP as u8))
            }
            KeyCode::Right | KeyCode::Char('+') if row == Row::HissLevel => {
                self.set_hiss_level(self.settings.hiss_level.saturating_add(cmd::STEP as u8))
            }
            KeyCode::Left if row == Row::Shell => self.cycle_shell(-1),
            KeyCode::Right if row == Row::Shell => self.cycle_shell(1),
            KeyCode::Left | KeyCode::Char('-') => self.dispatch(Cmd::Volume(-cmd::STEP)),
            KeyCode::Right | KeyCode::Char('+') => self.dispatch(Cmd::Volume(cmd::STEP)),
            KeyCode::Enter if row == Row::HistoryFile => {
                self.status = format!(
                    "history file: {} (change: env TERMUSIC_HISTORY or history_path in settings.json)",
                    self.history_path
                        .as_ref()
                        .map_or("none".into(), |p| p.display().to_string())
                );
            }
            KeyCode::Enter if row == Row::ClearHistory && confirmed => {
                self.history.clear();
                self.save_history();
                self.status = "history cleared".into();
            }
            KeyCode::Enter if row == Row::ClearHistory => {
                self.confirm_clear = true;
                self.status = "press Enter again to clear the history".into();
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
        let tab = self.tab().tab_index().unwrap_or(0);
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.select(self.selected.saturating_add(1)),
            KeyCode::Char('k') | KeyCode::Up => self.select(self.selected.saturating_sub(1)),
            KeyCode::PageDown => self.select(self.selected.saturating_add(self.page)),
            KeyCode::PageUp => self.select(self.selected.saturating_sub(self.page)),
            KeyCode::Char('g') | KeyCode::Home => self.select(0),
            KeyCode::Char('G') | KeyCode::End => self.select(last),
            KeyCode::Tab => self.goto_tab(next_tab(tab, 1, tabs_off()), tabs_off()),
            KeyCode::BackTab => self.goto_tab(next_tab(tab, 3, tabs_off()), tabs_off()),
            KeyCode::Char(c @ '1'..='4') => self.goto_tab(c as usize - '1' as usize, tabs_off()),
            KeyCode::Enter if self.screen == Screen::Login => {
                if !self.busy {
                    self.start_login()
                }
            }
            KeyCode::Enter if self.len() == 0 => {} // nothing to open or play
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
                // Replay from here (and from where it stopped); the queue follows history order.
                self.record_position();
                if let Some(e) = self.history.get(self.selected) {
                    self.resume_at = e.position_secs;
                    let tracks = self.history.iter().map(|e| e.track.clone()).collect();
                    self.queue.set(tracks, self.selected);
                    self.play_current();
                }
            }
            KeyCode::Enter if self.screen == Screen::Queue => {
                // Row 0 is the current track (when there is one); the rest are `upcoming`.
                let n = self
                    .selected
                    .checked_sub(usize::from(self.queue.current().is_some()));
                if let Some(n) = n
                    && self.queue.jump(n).is_some()
                {
                    self.play_current();
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
            KeyCode::Char('a') if self.screen == Screen::Results => self.queue_selected(),
            KeyCode::Char('P') | KeyCode::F(2) => self.open_playlists(),
            KeyCode::Char(commands::settings::KEY) | KeyCode::F(3) => self.open(Screen::Settings),
            KeyCode::Char(commands::history::KEY) | KeyCode::F(4) => self.goto_tab(3, tabs_off()),
            KeyCode::Esc => self.focus_search(),
            KeyCode::Char('?') | KeyCode::F(1) => self.open(Screen::Help),
            KeyCode::Char('q') => self.quit = true,
            _ => {}
        }
    }

    /// Length of the list in the active tab.
    pub fn len(&self) -> usize {
        match self.tab() {
            Screen::Playlists => self.playlists.len(),
            Screen::History => self.history.len(),
            Screen::Queue => {
                usize::from(self.queue.current().is_some()) + self.queue.upcoming().count()
            }
            Screen::Login => 0,
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

/// Tabs whose command is disabled, in `TABS` order.
fn tabs_off() -> [bool; 4] {
    [
        false,
        false,
        commands::disabled(commands::playlists::KEY),
        commands::disabled(commands::history::KEY),
    ]
}

/// The tab `step` places after `from` (3 = back one) that is not off; `from` if none.
fn next_tab(from: usize, step: usize, off: [bool; 4]) -> usize {
    (1..=TABS.len())
        .map(|k| (from + step * k) % TABS.len())
        .find(|&i| !off[i])
        .unwrap_or(from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut a = App::blank();
        a.screen = Screen::Results;
        a.results = (0..30)
            .map(|i| Track {
                title: i.to_string(),
                channel: String::new(),
                id: i.to_string(),
            })
            .collect();
        a.page = 5;
        a.searching = false;
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
        let mut a = App::blank();
        a.tx.send(Msg::Offline).unwrap();
        a.tick();
        assert_eq!(a.screen, Screen::Offline);
        a.tx.send(Msg::Online).unwrap();
        a.tick();
        assert_eq!((a.screen, a.searching), (Screen::Results, true)); // guests are never forced to log in
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
        let mut a = App::blank();
        a.screen = Screen::Results;
        a.searching = true;
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
        assert_eq!(
            (a.screen, a.searching, a.quit),
            (Screen::Results, false, false)
        );
        assert_eq!(a.input, "pq"); // leaving the field keeps the text
    }

    #[test]
    fn list_keys_and_help_roundtrip() {
        let mut a = app();
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(a.selected, 1);
        press(&mut a, KeyCode::Char('s'));
        assert!(a.queue.shuffle);
        for (want, text) in [
            (Loop::One, "loop: this track"),
            (Loop::All, "loop: whole queue"),
            (Loop::Off, "loop: off"),
        ] {
            press(&mut a, KeyCode::Char('l'));
            assert_eq!((a.queue.looping, a.status.as_str()), (want, text));
        }
        press(&mut a, KeyCode::Char('?'));
        assert_eq!(a.screen, Screen::Help);
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.screen, Screen::Results);
        press(&mut a, KeyCode::Char('q'));
        assert!(a.quit);
    }

    #[test]
    fn logged_out_lists_tab_shows_sign_in() {
        let mut a = app();
        a.select(3);
        a.open_playlists_as(false);
        assert_eq!(
            (a.screen, a.tab().tab_index(), a.len()),
            (Screen::Login, Some(2), 0)
        );
        assert!(a.status.is_empty());
        a.busy = true;
        press(&mut a, KeyCode::Enter); // busy: no second login is started
        assert_eq!(a.screen, Screen::Login);
        press(&mut a, KeyCode::Char('1'));
        assert_eq!((a.screen, a.selected), (Screen::Results, 3));
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
    fn f2_on_search_opens_lists_but_p_is_text() {
        let mut a = App::blank();
        a.screen = Screen::Results;
        a.searching = true;
        press(&mut a, KeyCode::Char('P'));
        assert_eq!((a.input.as_str(), a.screen), ("P", Screen::Results));
        a.logged_in = Some(false);
        press(&mut a, KeyCode::F(2));
        assert_eq!((a.screen, a.searching), (Screen::Login, false));
    }

    #[test]
    fn stale_playlist_reply_ignored_and_tracks_reset_flag() {
        let t = |id: &str| Track {
            title: id.into(),
            channel: String::new(),
            id: id.into(),
        };
        let mut a = app();
        a.tx.send(Msg::PlaylistTracks("pl".into(), vec![t("a")]))
            .unwrap(); // wrong screen
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
    fn p_and_digit_reach_the_lists_tab_signed_out() {
        let mut a = app();
        a.logged_in = Some(false);
        for k in [KeyCode::Char('P'), KeyCode::F(2), KeyCode::Char('3')] {
            press(&mut a, KeyCode::Char('1'));
            press(&mut a, k);
            assert_eq!(a.screen, Screen::Login, "{k:?}");
        }
        press(&mut a, KeyCode::Tab);
        assert_eq!(a.screen, Screen::History);
        press(&mut a, KeyCode::BackTab);
        assert_eq!(a.screen, Screen::Login);
        press(&mut a, KeyCode::Esc); // a list's Esc focuses the search field
        assert!(a.searching && a.screen == Screen::Login);
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
        assert!(a.queue.shuffle && a.queue.looping == Loop::One);
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
        assert_eq!(a.screen, Screen::Results);
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
    fn search_focus_tab_and_esc_never_quit() {
        let mut a = app();
        press(&mut a, KeyCode::Esc);
        assert_eq!((a.screen, a.searching), (Screen::Results, true));
        press(&mut a, KeyCode::Tab);
        assert_eq!((a.screen, a.searching), (Screen::Results, false));
        press(&mut a, KeyCode::Char('/'));
        press(&mut a, KeyCode::Esc);
        assert_eq!((a.screen, a.searching), (Screen::Results, false));
        a.results.clear(); // nothing behind the field: Esc still only leaves it
        a.searching = true;
        press(&mut a, KeyCode::Tab);
        assert!(!a.searching && !a.quit);
        a.searching = true;
        press(&mut a, KeyCode::Esc);
        assert!(!a.searching && !a.quit);
        a.searching = true;
        press(&mut a, KeyCode::Char('q')); // a letter while typing
        assert_eq!((a.input.as_str(), a.quit), ("q", false));
        a.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(a.quit);
    }

    #[test]
    fn overlays_return_to_prior_screen_and_selection() {
        let mut a = app();
        a.select(7);
        for (key, screen) in [
            (KeyCode::Char('S'), Screen::Settings),
            (KeyCode::F(3), Screen::Settings),
            (KeyCode::Char('?'), Screen::Help),
            (KeyCode::F(1), Screen::Help),
        ] {
            press(&mut a, key);
            assert_eq!(a.screen, screen);
            press(&mut a, KeyCode::Esc);
            assert_eq!((a.screen, a.selected), (Screen::Results, 7), "{key:?}");
        }
        // Toggle keys close their own screen.
        press(&mut a, KeyCode::Char('S'));
        press(&mut a, KeyCode::Char('S'));
        assert_eq!(a.screen, Screen::Results);
        press(&mut a, KeyCode::Char('S'));
        press(&mut a, KeyCode::Char('?'));
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.screen, Screen::Settings); // Help returns to the overlay it came from
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.screen, Screen::Results);
        // From the search field the F-keys open them; Esc comes back to the list.
        a.searching = true;
        press(&mut a, KeyCode::F(3));
        assert_eq!((a.screen, a.searching), (Screen::Settings, false));
        press(&mut a, KeyCode::Esc);
        assert_eq!((a.screen, a.selected), (Screen::Results, 7));
        // `/` from an overlay closes it and focuses the field.
        press(&mut a, KeyCode::Char('?'));
        press(&mut a, KeyCode::Char('/'));
        assert_eq!((a.screen, a.searching), (Screen::Results, true));
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

    fn ids(a: &App) -> Vec<&str> {
        a.history.iter().map(|e| e.track.id.as_str()).collect()
    }

    fn scratch(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("termusic-app-{}-{tag}", std::process::id()))
    }

    #[test]
    fn track_started_records_history_most_recent_first() {
        let mut a = app();
        for i in ["1", "2", "1"] {
            a.queue.set(a.results.clone(), i.parse().unwrap());
            a.on_event(Event::TrackStarted(i.into()));
        }
        assert_eq!(ids(&a), ["1", "2"]);
        a.on_event(Event::TrackStarted("not-current".into())); // not the queue's track: ignored
        assert_eq!(a.history.len(), 2);
    }

    #[test]
    fn new_loads_history_and_changes_are_written() {
        let dir = scratch("io");
        let p = dir.join("h.json");
        let t = |id: &str| Track {
            title: id.into(),
            channel: "c".into(),
            id: id.into(),
        };
        let seed = vec![Entry {
            track: t("old"),
            position_secs: 77,
        }];
        history::save(&p, &seed).unwrap();
        let mut a = App::with_paths(Settings::default(), Some(p.clone()));
        assert_eq!(a.history, seed);
        a.queue.set(vec![t("new")], 0);
        a.on_event(Event::TrackStarted("new".into()));
        let on_disk = history::load(&p);
        assert_eq!(
            on_disk
                .iter()
                .map(|e| e.track.id.as_str())
                .collect::<Vec<_>>(),
            ["new", "old"]
        );
        (a.screen, a.searching) = (Screen::Settings, false);
        a.stack = vec![Screen::Results];
        a.setting_row = Row::ALL
            .iter()
            .position(|&r| r == Row::ClearHistory)
            .unwrap();
        press(&mut a, KeyCode::Enter);
        assert_eq!(history::load(&p).len(), 2); // first Enter only asks
        press(&mut a, KeyCode::Enter);
        assert_eq!(std::fs::read_to_string(&p).unwrap().trim(), "[]");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_history_save_shows_in_status() {
        let dir = scratch("fail");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, "x").unwrap();
        let mut a = App::with_paths(Settings::default(), Some(file.join("h.json")));
        a.save_history();
        a.tick();
        assert!(a.status.starts_with("history not saved"), "{}", a.status);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn position_is_recorded_and_replay_resumes() {
        let mut a = app();
        a.queue.set(a.results.clone(), 3);
        a.on_event(Event::TrackStarted("3".into()));
        a.player.position = 95.4;
        a.player.duration = Some(300.0);
        a.record_position();
        assert_eq!(a.history[0].position_secs, 95);
        assert!(a.playing_id.is_none());
        a.record_position(); // nothing playing: no change
        assert_eq!(a.history[0].position_secs, 95);
        a.screen = Screen::History;
        a.stack = vec![Screen::Results];
        press(&mut a, KeyCode::Enter); // spawns mpv if installed; resume_at is consumed either way
        assert_eq!(a.resume_at, 0);
        assert_eq!(a.queue.current().map(|t| t.id.as_str()), Some("3"));
        a.player.stop();
    }

    #[test]
    fn history_screen_lists_and_enter_queues_from_selection() {
        let mut a = app();
        for id in ["h2", "h1", "h0"] {
            history::push(&mut a.history, track(id));
        }
        press(&mut a, KeyCode::Char('H'));
        assert_eq!((a.len(), a.selected), (3, 0));
        press(&mut a, KeyCode::Char('j'));
        press(&mut a, KeyCode::Enter);
        // Playing h1 moves it to the top; the queue keeps the old order after it.
        assert_eq!(a.queue.current().map(|t| t.id.as_str()), Some("h1"));
        assert_eq!(a.queue.upcoming().next().map(|t| t.id.as_str()), Some("h2"));
        assert_eq!(ids(&a)[0], "h1");
        assert_eq!((a.screen, a.selected), (Screen::History, 0));
        a.player.stop();
    }

    #[test]
    fn settings_volume_rows_and_confirmed_clear() {
        let mut a = app();
        history::push(&mut a.history, track("x"));
        a.player.volume = 50;
        press(&mut a, KeyCode::Char('S'));
        press(&mut a, KeyCode::Right);
        press(&mut a, KeyCode::Char('+'));
        assert_eq!((a.player.volume, a.settings.volume), (60, 60));
        press(&mut a, KeyCode::Left);
        press(&mut a, KeyCode::Char('-'));
        press(&mut a, KeyCode::Char('-'));
        assert_eq!(a.player.volume, 45);
        press(&mut a, KeyCode::Enter); // volume row: nothing cleared
        assert_eq!(a.history.len(), 1);
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(Row::ALL[a.setting_row], Row::Shell);
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(Row::ALL[a.setting_row], Row::Animations);
        assert!(a.settings.animations);
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Right);
        press(&mut a, KeyCode::Left);
        assert!(!a.settings.animations && a.player.volume == 45); // toggled, volume untouched
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(Row::ALL[a.setting_row], Row::Hiss);
        press(&mut a, KeyCode::Enter); // on: no track loaded, so no hiss mpv yet
        assert!(a.settings.hiss && a.player.hiss_on);
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(Row::ALL[a.setting_row], Row::HissLevel);
        press(&mut a, KeyCode::Right);
        press(&mut a, KeyCode::Char('+'));
        assert_eq!((a.settings.hiss_level, a.player.hiss_level), (40, 40));
        for _ in 0..12 {
            press(&mut a, KeyCode::Right); // clamps at 100
        }
        assert_eq!(a.settings.hiss_level, 100);
        for _ in 0..25 {
            press(&mut a, KeyCode::Char('-')); // and at 0
        }
        assert_eq!((a.settings.hiss_level, a.player.volume), (0, 45));
        press(&mut a, KeyCode::Char('k'));
        press(&mut a, KeyCode::Left); // off again
        assert!(!a.settings.hiss && !a.player.hiss_on);
        press(&mut a, KeyCode::Char('j'));
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(Row::ALL[a.setting_row], Row::HistoryFile);
        a.history_path = Some("/h.json".into());
        press(&mut a, KeyCode::Enter);
        assert!(a.status.contains("/h.json") && a.status.contains("TERMUSIC_HISTORY"));
        for _ in 0..5 {
            press(&mut a, KeyCode::Char('j')); // clamps at the last row
        }
        assert_eq!(Row::ALL[a.setting_row], Row::ClearHistory);
        press(&mut a, KeyCode::Enter);
        assert!(a.confirm_clear && a.history.len() == 1);
        press(&mut a, KeyCode::Char('k')); // any other key cancels the confirmation
        press(&mut a, KeyCode::Char('j'));
        press(&mut a, KeyCode::Enter);
        assert!(a.confirm_clear && a.history.len() == 1);
        press(&mut a, KeyCode::Enter);
        assert!(a.history.is_empty() && !a.confirm_clear);
        press(&mut a, KeyCode::Char('q'));
        assert!(a.quit);
    }

    #[test]
    fn each_tab_keeps_its_own_selection() {
        let mut a = app();
        for id in ["h2", "h1", "h0"] {
            history::push(&mut a.history, track(id));
        }
        a.select(7);
        press(&mut a, KeyCode::Char('H'));
        assert_eq!((a.screen, a.selected), (Screen::History, 0));
        press(&mut a, KeyCode::Char('j'));
        press(&mut a, KeyCode::Char('/')); // the field opens over History
        assert_eq!((a.screen, a.searching), (Screen::History, true));
        press(&mut a, KeyCode::Esc);
        press(&mut a, KeyCode::Char('1'));
        assert_eq!((a.screen, a.selected), (Screen::Results, 7));
        press(&mut a, KeyCode::Char('4'));
        assert_eq!((a.screen, a.selected), (Screen::History, 1));
        press(&mut a, KeyCode::Char('H')); // already there: stays
        assert_eq!((a.screen, a.selected), (Screen::History, 1));
        a.history.truncate(1); // the list shrank while away: clamped on return
        press(&mut a, KeyCode::Char('1'));
        press(&mut a, KeyCode::Char('4'));
        assert_eq!(a.selected, 0);
        a.logged_in = Some(false);
        press(&mut a, KeyCode::F(2));
        press(&mut a, KeyCode::BackTab);
        press(&mut a, KeyCode::BackTab);
        assert_eq!((a.screen, a.selected), (Screen::Results, 7));
    }

    #[test]
    fn help_and_settings_return_to_the_tab_or_overlay_they_came_from() {
        let mut a = app();
        a.select(4);
        press(&mut a, KeyCode::Char('H'));
        press(&mut a, KeyCode::Char('?'));
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.screen, Screen::History);
        press(&mut a, KeyCode::Char('S'));
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.screen, Screen::History);
        press(&mut a, KeyCode::Char('S'));
        press(&mut a, KeyCode::Char('H')); // History from Settings closes the overlay
        assert_eq!((a.screen, a.stack.len()), (Screen::History, 0));
        press(&mut a, KeyCode::Tab); // wraps to Results
        assert_eq!((a.screen, a.selected), (Screen::Results, 4));
    }

    #[test]
    fn selection_follows_track_started_during_overlay() {
        let mut a = app();
        a.select(2);
        press(&mut a, KeyCode::Char('?'));
        a.queue.set(a.results.clone(), 9);
        a.on_event(Event::TrackStarted("9".into()));
        press(&mut a, KeyCode::Esc);
        assert_eq!((a.screen, a.selected), (Screen::Results, 9));
    }

    #[test]
    fn help_g_jumps_to_bottom_and_top() {
        let mut a = app();
        press(&mut a, KeyCode::Char('?'));
        press(&mut a, KeyCode::Char('G'));
        assert_eq!(a.help_scroll, u16::MAX);
        press(&mut a, KeyCode::Home);
        assert_eq!(a.help_scroll, 0);
    }

    #[test]
    fn background_writes_run_inline_without_runtime_and_in_order() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        for i in 0..3 {
            let l = log.clone();
            bg(move || l.lock().unwrap().push(i));
        }
        assert_eq!(*log.lock().unwrap(), [0, 1, 2]);
    }

    #[test]
    fn screen_overlay_flags_and_tabs() {
        assert!(Screen::Help.is_overlay() && Screen::Settings.is_overlay());
        assert!(!Screen::Results.is_overlay() && !Screen::History.is_overlay());
        assert_eq!(
            TABS.map(|t| t.tab_index()),
            [Some(0), Some(1), Some(2), Some(3)]
        );
        assert_eq!(Screen::Login.tab_index(), Some(2));
        assert_eq!(Screen::Help.tab_index(), None);
    }

    #[test]
    fn queue_tab_lists_current_then_upcoming_and_enter_jumps() {
        let mut a = app();
        press(&mut a, KeyCode::Char('2'));
        assert_eq!((a.screen, a.len()), (Screen::Queue, 0));
        press(&mut a, KeyCode::Enter); // empty: nothing happens
        assert!(a.queue.current().is_none());
        a.queue.add(track("x")); // nothing current: row 0 is the first upcoming
        a.queue.add(track("y"));
        assert_eq!(a.len(), 2);
        press(&mut a, KeyCode::Char('j'));
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.queue.current().map(|t| t.id.as_str()), Some("y"));
        assert_eq!((a.len(), a.selected), (1, 0));
        a.queue
            .set(vec![track("a"), track("b"), track("c"), track("d")], 0);
        assert_eq!(a.len(), 4);
        press(&mut a, KeyCode::Enter); // row 0 is the playing track: unchanged
        assert_eq!(a.queue.current().map(|t| t.id.as_str()), Some("a"));
        press(&mut a, KeyCode::Char('G'));
        press(&mut a, KeyCode::Enter); // row 3 = upcoming[2]
        assert_eq!(a.queue.current().map(|t| t.id.as_str()), Some("d"));
        assert_eq!((a.len(), a.selected), (1, 0));
        assert_eq!(a.queue.prev().map(|t| t.id.as_str()), Some("c"));
        a.player.stop();
    }

    #[test]
    fn results_follow_the_playing_track_from_another_tab() {
        let mut a = app();
        press(&mut a, KeyCode::Char('2'));
        a.queue.set(a.results.clone(), 12);
        a.on_event(Event::TrackStarted("12".into()));
        assert_eq!((a.screen, a.selected), (Screen::Queue, 0));
        press(&mut a, KeyCode::Char('1'));
        assert_eq!(a.selected, 12);
        assert!((a.offset..a.offset + a.page).contains(&12)); // scrolled into view
    }

    #[test]
    fn related_reply_clamps_the_queue_selection() {
        let mut a = app();
        a.queue.set(a.results.clone(), 0);
        press(&mut a, KeyCode::Char('2'));
        press(&mut a, KeyCode::Char('G'));
        assert_eq!(a.selected, 29);
        a.tx.send(Msg::Related("0".into(), vec![track("0"), track("r")]))
            .unwrap();
        a.tick();
        assert_eq!((a.len(), a.selected), (2, 1));
    }

    #[test]
    fn tracks_reply_switches_to_results_with_list_focus() {
        let mut a = app();
        press(&mut a, KeyCode::Char('4'));
        a.input = "x".into();
        a.submitted = "x".into();
        a.searching = true;
        a.tx.send(Msg::Tracks(vec![track("n")])).unwrap();
        a.tick();
        assert_eq!(
            (a.screen, a.searching, a.selected),
            (Screen::Results, false, 0)
        );
        // Not typing when it arrives: focus stays on the list.
        a.tx.send(Msg::Tracks(vec![track("m")])).unwrap();
        a.tick();
        assert!(!a.searching);
    }

    fn submit(a: &mut App, q: &str) {
        a.searching = true;
        a.input = q.into();
        press(a, KeyCode::Enter);
    }

    #[test]
    fn submitting_a_search_keeps_the_field_and_typing_is_text() {
        // Spawned tasks never run (nothing drives this runtime), so the test stays offline.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _g = rt.enter();
        let mut a = app();
        submit(&mut a, "x");
        assert!(a.searching && a.busy);
        for c in "nq".chars() {
            press(&mut a, KeyCode::Char(c));
        }
        assert!(a.searching && !a.quit && a.input == "xnq");
    }

    #[test]
    fn results_move_focus_only_for_the_unchanged_query() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _g = rt.enter();
        let mut a = app();
        submit(&mut a, "x");
        a.tx.send(Msg::Tracks(vec![track("n")])).unwrap();
        a.tick();
        assert_eq!((a.screen, a.searching), (Screen::Results, false));

        press(&mut a, KeyCode::Char('2')); // elsewhere, then a second search with more typing
        submit(&mut a, "y");
        press(&mut a, KeyCode::Char('z'));
        a.tx.send(Msg::Tracks(vec![track("m")])).unwrap();
        a.tick();
        assert_eq!((a.screen, a.searching), (Screen::Results, true));
        assert_eq!((a.results[0].id.as_str(), a.input.as_str()), ("m", "yz"));

        // Errors and empty results leave focus in the field.
        submit(&mut a, "w");
        a.tx.send(Msg::Err("boom".into())).unwrap();
        a.tick();
        assert!(a.searching);
        a.tx.send(Msg::Tracks(Vec::new())).unwrap();
        a.tick();
        assert!(a.searching);
    }

    #[test]
    fn enter_on_the_signed_out_lists_tab_starts_login() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _g = rt.enter();
        let mut a = app();
        a.logged_in = Some(false);
        press(&mut a, KeyCode::Char('3'));
        assert_eq!(a.screen, Screen::Login);
        a.busy = false;
        press(&mut a, KeyCode::Enter);
        assert!(a.busy);
    }

    #[test]
    fn sync_preload_needs_a_playing_track_and_drops_stale_preloads() {
        let mut a = app();
        a.queue.set(vec![track("a"), track("b")], 0);
        a.sync_preload(); // nothing playing yet: no preload
        assert!(a.player.preloaded_id().is_none());
        a.queue.set(vec![track("a")], 0); // nothing follows
        a.sync_preload();
        assert!(a.player.preloaded_id().is_none());
        // a preload of "b" while the queue now says nothing follows is dropped
        a.player.fake_preload("b");
        assert_eq!(a.player.preloaded_id(), Some("b"));
        a.sync_preload();
        assert!(a.player.preloaded_id().is_none());
    }

    #[test]
    fn dropping_a_preload_for_loop_one_lets_it_be_wanted_again() {
        let mut a = app();
        a.queue.set(vec![track("a"), track("b")], 0);
        a.player.fake_preload("b");
        a.player.tried = Some("b".into()); // as `preload` records it
        a.queue.looping = Loop::One; // `l`: no next track, the preload goes
        a.sync_preload();
        assert!(a.player.preloaded_id().is_none() && a.player.tried.is_none());
    }

    #[test]
    fn shutdown_saves_history_and_takes_player_volume() {
        let dir = scratch("shutdown");
        let p = dir.join("h.json");
        let mut a = App::with_paths(Settings::default(), Some(p.clone()));
        a.player.volume = 33;
        a.queue.set(vec![track("x")], 0);
        a.on_event(Event::TrackStarted("x".into()));
        let _ = std::fs::remove_file(&p);
        a.shutdown();
        assert_eq!(a.settings.volume, 33);
        assert_eq!(history::load(&p).len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn shell_row_cycles_both_ways_and_keeps_volume() {
        let mut a = app();
        press(&mut a, KeyCode::Char('S'));
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(Row::ALL[a.setting_row], Row::Shell);
        let first = a.settings.shell.clone();
        press(&mut a, KeyCode::Right);
        assert_eq!(a.settings.shell, theme::SHELLS[1].id);
        assert_eq!(a.shell(), theme::SHELLS[1]);
        press(&mut a, KeyCode::Left);
        press(&mut a, KeyCode::Left); // wraps to the last
        assert_eq!(a.settings.shell, theme::SHELLS[theme::SHELLS.len() - 1].id);
        press(&mut a, KeyCode::Right);
        assert_eq!(a.settings.shell, first);
        assert_eq!(a.player.volume, crate::settings::DEFAULT_VOLUME);
        a.settings.shell = "bogus".into(); // unknown: drawn and stepped from the default
        assert_eq!(a.shell(), theme::SHELLS[0]);
        press(&mut a, KeyCode::Right);
        assert_eq!(a.settings.shell, theme::SHELLS[1].id);
    }

    #[test]
    fn save_settings_without_a_config_path_is_harmless() {
        let a = app(); // path() is None under test: must not touch the real config
        a.save_settings();
        a.save_history();
    }

    fn typing_app() -> App {
        let mut a = app();
        a.logged_in = Some(true);
        a.screen = Screen::Playlists;
        a.playlists = vec![track("pl")];
        a.searching = true;
        a.input = "ab".into();
        a
    }

    fn still_typing(a: &mut App) {
        assert!(a.searching && a.input == "ab");
        press(a, KeyCode::Char('q'));
        assert!(!a.quit && a.input == "abq");
    }

    #[test]
    fn replies_never_take_focus_from_the_search_field() {
        // Spawned tasks never run (nothing drives this runtime), so the test stays offline.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _g = rt.enter();
        let mut a = typing_app();
        a.tx.send(Msg::PlaylistTracks("pl".into(), vec![track("a")]))
            .unwrap();
        a.tick();
        assert_eq!(a.queue.current().map(|t| t.id.as_str()), Some("a"));
        still_typing(&mut a);
        let mut a = typing_app();
        a.tx.send(Msg::Tracks(vec![track("n")])).unwrap();
        a.tick();
        assert_eq!(a.screen, Screen::Results);
        still_typing(&mut a);
        let mut a = typing_app();
        a.tx.send(Msg::LoggedIn).unwrap();
        a.tick();
        still_typing(&mut a);
        let mut a = typing_app();
        a.tx.send(Msg::Offline).unwrap();
        a.tx.send(Msg::Online).unwrap();
        a.tick();
        still_typing(&mut a);
    }

    #[test]
    fn enter_on_an_empty_list_does_nothing() {
        // Spawned tasks never run (nothing drives this runtime), so the test stays offline.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _g = rt.enter();
        for tab in ['1', '2', '3', '4'] {
            let mut a = app();
            a.logged_in = Some(true);
            a.queue.set(vec![track("p")], 0);
            a.player.now_playing = Some("p".into());
            a.results.clear();
            press(&mut a, KeyCode::Char(tab));
            press(&mut a, KeyCode::Enter);
            assert_eq!(a.queue.current().map(|t| t.id.as_str()), Some("p"));
            assert_eq!(a.player.now_playing.as_deref(), Some("p"));
        }
        let mut a = app();
        a.results.clear();
        a.queue.set(vec![track("p")], 0);
        a.player.now_playing = Some("p".into());
        press(&mut a, KeyCode::Enter); // Results tab
        assert_eq!(a.player.now_playing.as_deref(), Some("p"));
        assert!(a.queue.current().is_some());
    }

    #[test]
    fn typing_every_printable_char_is_text() {
        let mut a = app();
        a.searching = true;
        press(&mut a, KeyCode::Esc);
        press(&mut a, KeyCode::Char('/'));
        let want: String = (' '..='~').collect();
        for c in want.chars() {
            press(&mut a, KeyCode::Char(c));
        }
        assert_eq!(a.input, want);
        assert!(a.searching && !a.quit && a.screen == Screen::Results && a.status.is_empty());
        assert!(!a.queue.shuffle && a.queue.looping == Loop::Off && !a.player.playing);
        assert_eq!(a.player.volume, App::blank().player.volume);
    }

    #[test]
    fn playlist_reply_must_match_the_selected_playlist() {
        let mut a = app();
        a.screen = Screen::Playlists;
        a.playlists = vec![track("p0"), track("p1")];
        a.selected = 1;
        a.tx.send(Msg::PlaylistTracks("p0".into(), vec![track("a")]))
            .unwrap();
        a.tick();
        assert!(a.queue.current().is_none() && a.screen == Screen::Playlists);
    }

    #[test]
    fn playlists_reply_clamps_the_selection() {
        // Spawned tasks never run (nothing drives this runtime), so the test stays offline.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _g = rt.enter();
        let mut a = app();
        a.logged_in = Some(true);
        press(&mut a, KeyCode::Char('3'));
        a.playlists = (0..9).map(|i| track(&i.to_string())).collect();
        a.select(5);
        a.tx.send(Msg::Playlists(vec![track("x"), track("y")]))
            .unwrap();
        a.tick();
        assert_eq!(a.selected, 1);
        let mut a = app();
        a.playlists = (0..9).map(|i| track(&i.to_string())).collect();
        a.tab_sel[2] = (5, 0);
        a.tx.send(Msg::Playlists(vec![track("x"), track("y")]))
            .unwrap();
        a.tick();
        a.logged_in = Some(true);
        press(&mut a, KeyCode::Char('3'));
        assert_eq!(a.selected, 1);
    }

    #[test]
    fn closing_settings_after_clearing_history_clamps_selection() {
        let mut a = app();
        a.history = (0..10)
            .map(|i| Entry {
                track: track(&i.to_string()),
                position_secs: 0,
            })
            .collect();
        press(&mut a, KeyCode::Char('4'));
        a.select(9);
        press(&mut a, KeyCode::Char('S'));
        a.setting_row = Row::ALL
            .iter()
            .position(|&r| r == Row::ClearHistory)
            .unwrap();
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Esc);
        assert!(a.history.is_empty() && a.screen == Screen::History);
        assert_eq!(a.selected, 0);
    }

    #[test]
    fn queue_jump_to_last_row_leaves_no_stale_preload() {
        let mut a = app();
        a.queue
            .set(vec![track("a"), track("b"), track("c"), track("d")], 0);
        a.player.fake_preload("b");
        a.player.tried = Some("b".into());
        press(&mut a, KeyCode::Char('2'));
        a.select(3);
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.queue.current().map(|t| t.id.as_str()), Some("d"));
        assert!(a.player.preloaded_id().is_none() && a.player.tried.is_none());
        a.player.stop();
    }

    #[test]
    fn animating_truth_table() {
        let mut a = app();
        for (on, playing, whirr, want) in [
            (true, false, false, false),
            (true, true, false, true),
            (true, false, true, true),
            (false, true, false, false),
            (false, false, true, false),
        ] {
            a.settings.animations = on;
            a.player.playing = playing;
            a.anim.whirr = whirr.then_some(a.anim.now);
            assert_eq!(a.animating(), want, "{on} {playing} {whirr}");
        }
        a.anim.whirr = Some(a.anim.now - std::time::Duration::from_secs(1)); // over
        a.settings.animations = true;
        a.player.playing = false;
        assert!(!a.animating());
    }

    #[test]
    fn dispatch_presses_the_cap_of_its_command() {
        let mut a = app();
        let key = |a: &App| a.anim.pressed.map(|(k, _)| k);
        a.dispatch(Cmd::ToggleShuffle);
        assert_eq!(key(&a), Some(Key::Shuffle));
        a.dispatch(Cmd::PlayPause); // nothing loaded: stays paused, so Pause shows the press
        assert_eq!(key(&a), Some(Key::Pause));
        a.dispatch(Cmd::CycleLoop);
        assert_eq!(key(&a), Some(Key::Loop));
        a.dispatch(Cmd::Volume(5)); // no cap: the last press stays
        assert_eq!(key(&a), Some(Key::Loop));
        a.anim.pressed = None;
        a.settings.animations = false;
        a.dispatch(Cmd::ToggleShuffle);
        assert_eq!(key(&a), None);
    }

    #[test]
    fn track_started_swaps_the_tape() {
        let mut a = app();
        a.queue.set(a.results.clone(), 3);
        a.player.now_playing = Some("3".into());
        a.on_event(Event::TrackStarted("3".into()));
        a.anim.now = a.anim.swap.as_ref().unwrap().0 + std::time::Duration::from_millis(200);
        assert_eq!(a.anim.phase(), Some(crate::anim::Phase::Insert(0))); // first tape: no eject
        assert_eq!(a.anim.shown, Some(("3".into(), String::new())));
        a.player.now_playing = None;
        a.tick(); // the deck emptied: nothing to eject next time
        assert!(a.anim.shown.is_none());
        a.settings.animations = false;
        a.anim.swap = None;
        a.on_event(Event::TrackStarted("3".into()));
        assert!(a.anim.swap.is_none());
    }

    #[test]
    fn seek_next_and_prev_whirr_only_with_animations() {
        let mut a = app();
        a.dispatch(Cmd::Seek(5));
        assert!(a.anim.whirr.is_some());
        a.anim.whirr = None;
        a.settings.animations = false;
        a.dispatch(Cmd::Prev);
        a.dispatch(Cmd::Seek(-5));
        assert!(a.anim.whirr.is_none());
    }

    #[test]
    fn tabs_skip_the_ones_that_are_off() {
        let off = [false, false, true, false];
        let visited = |step| {
            let mut v = vec![];
            let mut t = 0;
            for _ in 0..3 {
                t = next_tab(t, step, off);
                v.push(t);
            }
            v
        };
        assert_eq!(visited(1), [1, 3, 0]);
        assert_eq!(visited(3), [3, 1, 0]);
        let mut a = app();
        a.goto_tab(2, off);
        assert_eq!(a.screen, Screen::Results);
        a.goto_tab(3, off);
        assert_eq!(a.screen, Screen::History);
        assert_eq!(next_tab(1, 1, [false, true, true, true]), 0);
        assert_eq!(next_tab(0, 1, [false, true, true, true]), 0);
    }
}
