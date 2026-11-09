//! Playback: `mpv --no-video` (yt-dlp backend) controlled over its JSON IPC socket.
use crate::youtube::Track;
use anyhow::{Context, Result};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// Start loading the next track this many seconds before the current one ends.
pub const PRELOAD_SECS: f64 = 20.0;
/// Length of the overlap in which the old track fades out and the next fades in.
pub const CROSSFADE_SECS: f64 = 4.0;
/// How often a fade step is sent over IPC.
const FADE_STEP: Duration = Duration::from_millis(200);

#[derive(Debug)]
pub struct Player {
    pub playing: bool,
    pub now_playing: Option<String>,
    /// 0..=100; owned here so every new mpv starts with it.
    pub volume: u8,
    /// Seconds into the current track.
    pub position: f64,
    pub duration: Option<f64>,
    vol_synced: bool,
    current: Option<Running>,
    /// Preloaded next track: paused, volume 0, own socket.
    next: Option<Running>,
    /// Id of the last preload attempt, so a dead preload is not retried for the same track.
    tried: Option<String>,
    fade: Option<Fade>,
    /// Counter for distinct socket names.
    spawned: u32,
    /// The mpv binary; tests point it at a stand-in.
    pub mpv: String,
}

/// An in-progress crossfade between `current` and `next`.
#[derive(Debug)]
struct Fade {
    start: Instant,
    len: f64,
    stepped: Instant,
    /// Set while paused, so the fade does not advance meanwhile.
    paused_at: Option<Instant>,
}

impl Default for Player {
    fn default() -> Self {
        Self::new(crate::settings::DEFAULT_VOLUME)
    }
}

/// mpv plus the watchdog that kills it if this app dies without running `Drop` (SIGKILL).
#[derive(Debug)]
struct Running {
    mpv: Child,
    watchdog: Child,
    sock: PathBuf,
    id: String,
    title: String,
    started: Instant,
    polled: Instant,
}

impl Running {
    /// Kill mpv and its watchdog and remove the socket.
    fn kill(mut self) {
        for c in [&mut self.mpv, &mut self.watchdog] {
            let _ = c.kill();
            let _ = c.wait();
        }
        let _ = std::fs::remove_file(&self.sock);
    }
}

/// Socket of the `n`th mpv started by app process `app_pid`.
fn sock(app_pid: u32, n: u32) -> PathBuf {
    std::env::temp_dir().join(format!("termusic-mpv-{app_pid}-{n}.sock"))
}

fn mpv_command(
    program: &str,
    url: &str,
    sock: &std::path::Path,
    loop_file: bool,
    volume: u8,
    start: u64,
    paused: bool,
) -> Command {
    let mut c = Command::new(program);
    c.args(["--no-video", "--no-terminal", "--ytdl-format=bestaudio"])
        .args(paused.then_some("--pause=yes"))
        .arg(format!("--volume={volume}"))
        .arg(format!("--input-ipc-server={}", sock.display()))
        .args(loop_file.then_some("--loop-file=inf"))
        .args((start > 0).then(|| format!("--start={start}")))
        .arg(url);
    c
}

fn volume_json(vol: u8) -> String {
    format!(r#"["set_property","volume",{vol}]"#)
}

fn pause_json(on: bool) -> String {
    format!(r#"["set_property","pause",{on}]"#)
}

/// Whether to start loading the next track: it exists, none is loaded or tried yet,
/// and the current track (duration known) is within `PRELOAD_SECS` of its end.
pub fn should_preload(position: f64, duration: Option<f64>, next_exists: bool, done: bool) -> bool {
    next_exists && !done && duration.is_some_and(|d| d > 0.0 && d - position <= PRELOAD_SECS)
}

/// Equal-power crossfade at `progress` (0..=1): (old, new) volumes, with `user` as the ceiling.
pub fn fade_volumes(progress: f64, user: u8) -> (u8, u8) {
    let a = progress.clamp(0.0, 1.0) * std::f64::consts::FRAC_PI_2;
    let v = |f: f64| (f64::from(user) * f).round() as u8;
    (v(a.cos()), v(a.sin()))
}

fn seek_json(delta: i32) -> String {
    format!(r#"["seek",{delta},"relative"]"#)
}

/// Polls once a second; once the app or mpv is gone, kills mpv and removes its socket.
const WATCHDOG: &str = r#"trap '' HUP
while kill -0 "$1" 2>/dev/null && kill -0 "$2" 2>/dev/null; do sleep 1; done
kill "$2" 2>/dev/null; rm -f "$3""#;

fn watchdog_command(app_pid: u32, mpv_pid: u32, sock: &std::path::Path) -> Command {
    let mut c = Command::new("sh");
    c.args(["-c", WATCHDOG, "sh"])
        .args([app_pid.to_string(), mpv_pid.to_string()])
        .arg(sock);
    c
}

/// Startup check: a hint if mpv or yt-dlp can't be run.
pub fn missing_deps() -> Option<String> {
    let missing: Vec<_> = ["mpv", "yt-dlp"]
        .into_iter()
        .filter(|b| Command::new(b).arg("--version").output().is_err())
        .collect();
    (!missing.is_empty())
        .then(|| format!("missing {} — install to play music", missing.join(" + ")))
}

impl Player {
    pub fn new(volume: u8) -> Self {
        Self {
            playing: false,
            now_playing: None,
            volume: volume.min(100),
            position: 0.0,
            duration: None,
            vol_synced: false,
            current: None,
            next: None,
            tried: None,
            fade: None,
            spawned: 0,
            mpv: "mpv".into(),
        }
    }

    fn next_sock(&mut self) -> PathBuf {
        self.spawned += 1;
        sock(std::process::id(), self.spawned)
    }

    /// Spawn mpv for `t` (paused and silent when preloading) with its watchdog.
    fn spawn(&mut self, t: &Track, loop_file: bool, start: u64, preload: bool) -> Result<Running> {
        let sock = self.next_sock();
        let url = format!("https://www.youtube.com/watch?v={}", t.id);
        let vol = if preload { 0 } else { self.volume };
        let mpv = quiet(mpv_command(
            &self.mpv, &url, &sock, loop_file, vol, start, preload,
        ))
        .spawn()
        .context("cannot start mpv (mpv and yt-dlp must be on PATH)")?;
        wrap(mpv, sock, &t.id, &t.title)
    }

    /// Play `t` from `start` seconds; `loop_file` repeats it until told otherwise (`set_loop_file`).
    pub fn play(&mut self, t: &Track, loop_file: bool, start: u64) -> Result<()> {
        self.stop();
        let r = self.spawn(t, loop_file, start, false)?;
        self.set_current(r);
        Ok(())
    }

    fn set_current(&mut self, r: Running) {
        self.now_playing = Some(r.title.clone());
        self.current = Some(r);
        self.position = 0.0;
        self.duration = None;
        self.vol_synced = false;
        self.fade = None;
        self.tried = None;
        self.playing = true;
    }

    /// Start `t` paused and silent in the background, to be promoted when the current track ends.
    pub fn preload(&mut self, t: &Track) {
        self.discard_preload();
        self.tried = Some(t.id.clone());
        // A failed preload is silent: the cold path plays the track when its turn comes.
        self.next = self.spawn(t, false, 0, true).ok();
    }

    /// Id of the track loaded in the background, if any.
    pub fn preloaded_id(&self) -> Option<&str> {
        self.next.as_ref().map(|n| n.id.as_str())
    }

    /// Test hook: pretend `id` is preloaded (a `sleep` stands in for mpv).
    #[cfg(test)]
    pub(crate) fn fake_preload(&mut self, id: &str) {
        let sock = self.next_sock();
        let child = Command::new("sleep").arg("60").spawn().unwrap();
        self.next = Some(wrap(child, sock, id, id).unwrap());
    }

    /// Whether a preload of `id` should start now.
    pub fn wants_preload(&self, id: &str) -> bool {
        let done = self.next.is_some() || self.tried.as_deref() == Some(id);
        self.current.is_some() && should_preload(self.position, self.duration, true, done)
    }

    pub fn discard_preload(&mut self) {
        if let Some(n) = self.next.take() {
            n.kill();
        }
        if self.fade.take().is_some() {
            let _ = self.ipc(&volume_json(self.volume)); // the old track stays
        }
    }

    /// Abort a fade: the preload goes back to paused and silent, the current track to full volume.
    fn cancel_fade(&mut self) {
        if self.fade.take().is_some() {
            if let Some(n) = &self.next {
                let _ = ipc_to(&n.sock, &pause_json(true));
                let _ = ipc_to(&n.sock, &volume_json(0));
            }
            let _ = self.ipc(&volume_json(self.volume));
        }
    }

    /// Make the preloaded track `id` the current one at once (unpaused, at the user's volume).
    /// False (preload dropped, caller plays cold) if there is none or it is not ready.
    pub fn promote(&mut self, id: &str) -> bool {
        if self.next.as_ref().is_none_or(|n| n.id != id) {
            return false;
        }
        let n = self.next.take().expect("checked above");
        let sent = |c: &str| ipc_to(&n.sock, c).is_ok();
        if !(sent(&volume_json(self.volume)) && sent(&pause_json(false))) {
            n.kill(); // socket not up yet: it would stay paused
            return false;
        }
        if let Some(old) = self.current.take() {
            old.kill();
        }
        self.set_current(n);
        if let Some(r) = self.current.as_mut() {
            r.started = Instant::now(); // an exit right after promotion is a failure
        }
        self.vol_synced = true;
        true
    }

    pub fn stop(&mut self) {
        self.end_current();
        self.discard_preload();
        self.tried = None;
    }

    /// Kill the current mpv only; a preload survives to be promoted.
    fn end_current(&mut self) {
        if let Some(r) = self.current.take() {
            r.kill();
        }
        self.fade = None;
        self.playing = false;
        self.now_playing = None;
        self.position = 0.0;
        self.duration = None;
    }

    /// Send one JSON command to the running mpv (no-op when nothing is playing).
    fn ipc(&self, cmd: &str) -> Result<()> {
        self.current
            .as_ref()
            .map_or(Ok(()), |r| ipc_to(&r.sock, cmd))
    }

    pub fn toggle_pause(&mut self) -> Result<()> {
        self.ipc(r#"["cycle","pause"]"#)?;
        if self.current.is_some() {
            self.playing = !self.playing;
        }
        if let (Some(f), Some(n)) = (self.fade.as_mut(), &self.next) {
            let _ = ipc_to(&n.sock, r#"["cycle","pause"]"#); // both stay in step
            match f.paused_at.take() {
                Some(at) => f.start += at.elapsed(), // resume: the pause did not count
                None => f.paused_at = Some(Instant::now()),
            }
        }
        Ok(())
    }

    /// Change the volume by `delta` points (clamped); the new value sticks even if mpv isn't ready.
    pub fn change_volume(&mut self, delta: i32) -> Result<()> {
        self.volume = (i32::from(self.volume) + delta).clamp(0, 100) as u8;
        if self.fade.is_some() {
            return Ok(()); // the next fade step applies it, scaled
        }
        self.ipc(&volume_json(self.volume))
    }

    /// Jump `delta` seconds (negative = back) in the current track.
    pub fn seek(&mut self, delta: i32) -> Result<()> {
        self.cancel_fade(); // restarts on the next poll if still inside the window
        self.ipc(&seek_json(delta))
    }

    /// Repeat (or stop repeating) the track already playing.
    pub fn set_loop_file(&self, on: bool) -> Result<()> {
        let v = if on { "inf" } else { "no" };
        self.ipc(&format!(r#"["set_property","loop-file","{v}"]"#))
    }

    /// If mpv exited by itself, or the crossfade finished: `Some(true)` for a normal end,
    /// `Some(false)` for an immediate exit (mpv/yt-dlp failure) so the caller doesn't skip through the list.
    /// After `Some(true)` the caller starts the next track, which promotes the preload if there is one.
    pub fn tick(&mut self) -> Option<bool> {
        if let Some(n) = self.next.as_mut()
            && n.mpv.try_wait().ok().flatten().is_some()
        {
            self.discard_preload(); // dead preload: cold path later
        }
        let r = self.current.as_mut()?;
        if r.mpv.try_wait().ok().flatten().is_none() {
            if r.polled.elapsed() >= Duration::from_secs(1) {
                r.polled = Instant::now();
                let (pos, dur) = poll(&r.sock);
                if let Some(p) = pos {
                    self.position = p;
                }
                if dur.is_some() {
                    self.duration = dur;
                }
                // First answer means the socket exists: re-send a volume set before it did.
                if (pos.is_some() || dur.is_some()) && !self.vol_synced {
                    self.vol_synced = true;
                    let _ = self.ipc(&volume_json(self.volume));
                }
                self.keep_preload_fit();
                self.maybe_start_fade();
            }
            return self.step_fade();
        }
        let ok = r.started.elapsed() > Duration::from_secs(3);
        if ok {
            self.end_current();
        } else {
            self.stop();
        }
        Some(ok)
    }

    /// After a seek back: cancel a fade that is no longer in the window, drop a too-early preload.
    fn keep_preload_fit(&mut self) {
        let Some(left) = self.duration.map(|d| d - self.position) else {
            return;
        };
        if left > CROSSFADE_SECS {
            self.cancel_fade();
        }
        if left > PRELOAD_SECS && self.next.is_some() {
            self.discard_preload();
            self.tried = None; // may preload again once near the end
        }
    }

    /// Unpause the preload when the last `CROSSFADE_SECS` begin.
    fn maybe_start_fade(&mut self) {
        let (Some(n), Some(d)) = (&self.next, self.duration) else {
            return;
        };
        let left = d - self.position;
        if self.playing
            && self.fade.is_none()
            && left <= CROSSFADE_SECS
            && ipc_to(&n.sock, &pause_json(false)).is_ok()
        {
            let now = Instant::now();
            self.fade = Some(Fade {
                start: now,
                len: left.max(1.0),
                stepped: now.checked_sub(FADE_STEP).unwrap_or(now),
                paused_at: None,
            });
        }
    }

    /// Send the next fade step; `Some(true)` once the fade is complete.
    fn step_fade(&mut self) -> Option<bool> {
        let f = self.fade.as_mut()?;
        if f.paused_at.is_some() || f.stepped.elapsed() < FADE_STEP {
            return None;
        }
        f.stepped = Instant::now();
        let p = f.start.elapsed().as_secs_f64() / f.len;
        let (old, new) = fade_volumes(p, self.volume);
        let _ = self.ipc(&volume_json(old));
        if let Some(n) = &self.next {
            let _ = ipc_to(&n.sock, &volume_json(new));
        }
        (p >= 1.0).then(|| {
            self.fade = None;
            true
        })
    }
}

/// Send one JSON command to the mpv listening on `sock`.
fn ipc_to(sock: &std::path::Path, cmd: &str) -> Result<()> {
    UnixStream::connect(sock)
        .and_then(|mut s| s.write_all(format!("{{\"command\":{cmd}}}\n").as_bytes())) // one write: atomic line
        .context("mpv not ready yet")
}

/// Start the watchdog for `mpv` (killing it if that fails) and bundle them.
fn wrap(mut mpv: Child, sock: PathBuf, id: &str, title: &str) -> Result<Running> {
    match quiet(watchdog_command(std::process::id(), mpv.id(), &sock)).spawn() {
        Ok(watchdog) => Ok(Running {
            mpv,
            watchdog,
            sock,
            id: id.to_owned(),
            title: title.to_owned(),
            started: Instant::now(),
            polled: Instant::now(),
        }),
        Err(e) => {
            let _ = mpv.kill(); // never leave an unwatched mpv behind
            let _ = mpv.wait();
            Err(e).context("cannot start watchdog (sh)")
        }
    }
}

/// Query `time-pos` and `duration` in one round trip; short timeouts so the UI never stalls.
fn poll(sock: &std::path::Path) -> (Option<f64>, Option<f64>) {
    let t = Duration::from_millis(30);
    let Ok(mut s) = UnixStream::connect(sock) else {
        return (None, None);
    };
    let _ = s.set_read_timeout(Some(t));
    let _ = s.set_write_timeout(Some(t));
    let req = r#"{"command":["get_property","time-pos"],"request_id":1}
{"command":["get_property","duration"],"request_id":2}
"#;
    if s.write_all(req.as_bytes()).is_err() {
        return (None, None);
    }
    let mut lines = Vec::new();
    for l in BufReader::new(s).lines().take(8) {
        let Ok(l) = l else { break };
        let done = l.contains("\"request_id\":2");
        lines.push(l);
        if done {
            break;
        }
    }
    (parse_reply(&lines, 1), parse_reply(&lines, 2))
}

/// The numeric `data` of the reply with `request_id == id`; events and errors give `None`.
fn parse_reply<S: AsRef<str>>(lines: &[S], id: u64) -> Option<f64> {
    lines
        .iter()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l.as_ref()).ok())
        .find(|v| v["request_id"] == id)
        .filter(|v| v["error"] == "success")
        .and_then(|v| v["data"].as_f64())
}

/// Detach a child from the terminal: no stdin/stdout/stderr.
fn quiet(mut c: Command) -> Command {
    c.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    c
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alive(pid: u32) -> bool {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    fn wait_until(mut f: impl FnMut() -> bool) -> bool {
        for _ in 0..40 {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }

    fn sleeper(secs: &str) -> Child {
        Command::new("sleep").arg(secs).spawn().unwrap()
    }

    fn fake(p: &mut Player, id: &str, secs: &str) -> Running {
        let s = p.next_sock();
        wrap(sleeper(secs), s, id, id).unwrap()
    }

    fn temp_sock(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("termusic-test-{}-{tag}", std::process::id()));
        std::fs::write(&p, "").unwrap();
        p
    }

    #[test]
    fn watchdog_kills_orphaned_mpv_after_sigkill() {
        // helper plays the app: it starts the fake mpv and reports its pid
        let mut app = Command::new("sh")
            .args(["-c", "sleep 60 & echo $!; wait"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(app.stdout.take().unwrap()),
            &mut line,
        )
        .unwrap();
        let mpv: u32 = line.trim().parse().unwrap();
        let sock = temp_sock("orphan");
        let mut dog = watchdog_command(app.id(), mpv, &sock).spawn().unwrap();
        assert!(alive(mpv));
        app.kill().unwrap(); // SIGKILL
        app.wait().unwrap();
        assert!(wait_until(|| !alive(mpv)), "mpv survived its app");
        assert!(wait_until(|| !sock.exists()), "socket left behind");
        dog.wait().unwrap();
    }

    #[test]
    fn watchdog_exits_when_mpv_ends_normally() {
        let sock = temp_sock("normal");
        let mut mpv = sleeper("0.2");
        let mut dog = watchdog_command(std::process::id(), mpv.id(), &sock)
            .spawn()
            .unwrap();
        mpv.wait().unwrap();
        assert!(wait_until(|| dog.try_wait().unwrap().is_some()));
        assert!(!sock.exists());
    }

    #[test]
    fn stop_reaps_mpv_and_watchdog() {
        let mut p = Player::default();
        let r = fake(&mut p, "t", "60");
        p.set_current(r);
        let r = fake(&mut p, "n", "60");
        p.next = Some(r);
        assert!(p.playing);
        let pids: Vec<u32> = [&p.current, &p.next]
            .iter()
            .flat_map(|r| {
                let r = r.as_ref().unwrap();
                [r.mpv.id(), r.watchdog.id()]
            })
            .collect();
        p.stop();
        assert!(p.current.is_none() && p.next.is_none() && p.now_playing.is_none() && !p.playing);
        assert!(pids.iter().all(|&x| !alive(x)));
    }

    /// Stand-in for an mpv socket: records what the player sends.
    fn listener(tag: &str) -> (std::os::unix::net::UnixListener, PathBuf) {
        let p = std::env::temp_dir().join(format!("termusic-lt-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let l = std::os::unix::net::UnixListener::bind(&p).unwrap();
        l.set_nonblocking(true).unwrap();
        (l, p)
    }

    fn drain(l: &std::os::unix::net::UnixListener) -> String {
        let mut out = String::new();
        while let Ok((mut s, _)) = l.accept() {
            s.set_nonblocking(false).unwrap();
            let _ = std::io::Read::read_to_string(&mut s, &mut out);
        }
        out
    }

    /// Player with a current and a preloaded fake mpv, each behind a listener.
    fn rig(tag: &str) -> (Player, [std::os::unix::net::UnixListener; 2]) {
        let mut p = Player::new(80);
        let ((lc, sc), (ln, sn)) = (listener(&format!("{tag}c")), listener(&format!("{tag}n")));
        p.set_current(wrap(sleeper("60"), sc, "c", "c").unwrap());
        p.next = Some(wrap(sleeper("60"), sn, "n", "n").unwrap());
        (p, [lc, ln])
    }

    fn start_fade(p: &mut Player, elapsed: f64) {
        let now = Instant::now();
        p.fade = Some(Fade {
            start: now - Duration::from_secs_f64(elapsed),
            len: 4.0,
            stepped: now - FADE_STEP,
            paused_at: None,
        });
    }

    #[test]
    fn promote_sends_volume_and_unpause_and_kills_old() {
        let (mut p, [lc, ln]) = rig("promote");
        let old = p.current.as_ref().unwrap().mpv.id();
        assert!(p.promote("n"));
        let sent = drain(&ln);
        assert!(sent.contains(r#"["set_property","volume",80]"#), "{sent}");
        assert!(sent.contains(r#"["set_property","pause",false]"#), "{sent}");
        assert!(drain(&lc).is_empty());
        assert!(wait_until(|| !alive(old)));
        assert_eq!(p.current.as_ref().unwrap().id, "n");
        assert!(p.next.is_none() && p.playing);
        assert!(p.current.as_ref().unwrap().started.elapsed() < Duration::from_secs(1));
        p.stop();
    }

    #[test]
    fn step_fade_ramps_and_completes() {
        let (mut p, [lc, ln]) = rig("step");
        start_fade(&mut p, 2.0);
        assert_eq!(p.step_fade(), None);
        let (o, n) = fade_volumes(0.5, 80);
        assert!(drain(&lc).contains(&volume_json(o)));
        assert!(drain(&ln).contains(&volume_json(n)));
        start_fade(&mut p, 5.0);
        assert_eq!(p.step_fade(), Some(true));
        assert!(p.fade.is_none());
        assert!(drain(&ln).contains(&volume_json(80)));
        p.stop();
    }

    #[test]
    fn discard_during_fade_restores_volume() {
        let (mut p, [lc, _ln]) = rig("discard");
        start_fade(&mut p, 1.0);
        p.discard_preload();
        assert!(p.next.is_none() && p.fade.is_none());
        assert!(drain(&lc).contains(&volume_json(80)));
        p.stop();
    }

    #[test]
    fn pause_during_fade_pauses_both_and_freezes_it() {
        let (mut p, [lc, ln]) = rig("pause");
        start_fade(&mut p, 1.0);
        p.toggle_pause().unwrap();
        assert!(drain(&lc).contains("cycle") && drain(&ln).contains("cycle"));
        assert!(!p.playing);
        assert_eq!(p.step_fade(), None); // frozen
        p.toggle_pause().unwrap();
        assert!(p.fade.as_ref().unwrap().paused_at.is_none());
        p.stop();
    }

    #[test]
    fn seek_during_fade_cancels_it() {
        let (mut p, [lc, ln]) = rig("seek");
        start_fade(&mut p, 1.0);
        p.seek(-30).unwrap();
        assert!(p.fade.is_none());
        let (c, n) = (drain(&lc), drain(&ln));
        assert!(
            c.contains(&seek_json(-30)) && c.contains(&volume_json(80)),
            "{c}"
        );
        assert!(
            n.contains(&pause_json(true)) && n.contains(&volume_json(0)),
            "{n}"
        );
        assert!(p.next.is_some()); // still preloaded
        p.stop();
    }

    #[test]
    fn far_from_the_end_cancels_fade_and_drops_preload() {
        let (mut p, _l) = rig("far");
        start_fade(&mut p, 1.0);
        p.duration = Some(200.0);
        p.position = 100.0;
        p.keep_preload_fit();
        assert!(p.fade.is_none() && p.next.is_none() && p.tried.is_none());
    }

    #[test]
    fn socket_names_are_distinct() {
        assert_ne!(sock(1, 1), sock(1, 2));
        assert_ne!(sock(1, 1), sock(2, 1));
        let mut p = Player::default();
        assert_ne!(p.next_sock(), p.next_sock());
    }

    #[test]
    fn preload_decision() {
        let d = Some(200.0);
        assert!(!should_preload(100.0, d, true, false)); // far from the end
        assert!(should_preload(180.0, d, true, false)); // exactly PRELOAD_SECS left
        assert!(should_preload(199.0, d, true, false));
        assert!(!should_preload(199.0, d, false, false)); // empty queue / loop one
        assert!(!should_preload(199.0, d, true, true)); // never twice
        assert!(!should_preload(0.0, None, true, false)); // duration unknown
    }

    #[test]
    fn fade_math() {
        assert_eq!(fade_volumes(0.0, 80), (80, 0));
        assert_eq!(fade_volumes(1.0, 80), (0, 80));
        assert_eq!(fade_volumes(2.0, 80), (0, 80)); // clamped
        assert_eq!(fade_volumes(-1.0, 80), (80, 0));
        let (o, n) = fade_volumes(0.5, 100);
        assert_eq!((o, n), (71, 71)); // equal power, never above the ceiling
        let mut last = (101, 0);
        for i in 0..=20 {
            let (o, n) = fade_volumes(f64::from(i) / 20.0, 60);
            assert!(o <= 60 && n <= 60 && o <= last.0 && n >= last.1);
            last = (o, n);
        }
    }

    #[test]
    fn promote_needs_a_matching_ready_preload() {
        let mut p = Player::default();
        assert!(!p.promote("n")); // nothing preloaded
        let r = fake(&mut p, "n", "60");
        p.next = Some(r);
        assert!(!p.promote("other"));
        assert!(p.next.is_some());
        // No mpv socket exists: not ready, so the preload is dropped for a cold start.
        assert!(!p.promote("n"));
        assert!(p.next.is_none());
    }

    #[test]
    fn dead_preload_is_dropped_and_not_retried() {
        let mut p = Player::default();
        let r = fake(&mut p, "t", "60");
        p.set_current(r);
        let r = fake(&mut p, "n", "0");
        p.next = Some(r);
        p.tried = Some("n".into());
        assert!(wait_until(|| p.tick().is_none() && p.next.is_none()));
        assert!(p.current.is_some());
        p.position = 190.0;
        p.duration = Some(200.0);
        assert!(!p.wants_preload("n"));
        assert!(p.wants_preload("m"));
        p.stop();
    }

    #[test]
    fn natural_end_keeps_the_preload() {
        let mut p = Player::default();
        let mut r = fake(&mut p, "t", "0");
        r.started = Instant::now() - Duration::from_secs(5); // not an early failure
        p.current = Some(r);
        let r = fake(&mut p, "n", "60");
        p.next = Some(r);
        assert!(wait_until(|| p.tick() == Some(true)));
        assert!(p.current.is_none() && p.next.is_some());
        p.stop();
    }

    #[test]
    fn tick_reports_running_and_early_exit() {
        let mut p = Player::default();
        assert_eq!(p.tick(), None); // nothing playing
        let r = fake(&mut p, "t", "60");
        p.set_current(r);
        assert_eq!(p.tick(), None);
        p.stop();
        let r = fake(&mut p, "t", "0");
        p.set_current(r);
        assert!(wait_until(|| p.current.as_mut().is_some_and(|r| r
            .mpv
            .try_wait()
            .unwrap()
            .is_some())));
        assert_eq!(p.tick(), Some(false)); // exited within 3s: failure, not a normal end
        assert!(p.current.is_none());
    }

    #[test]
    fn ipc_json() {
        assert_eq!(volume_json(80), r#"["set_property","volume",80]"#);
        assert_eq!(pause_json(false), r#"["set_property","pause",false]"#);
        assert_eq!(seek_json(5), r#"["seek",5,"relative"]"#);
        assert_eq!(seek_json(-5), r#"["seek",-5,"relative"]"#);
    }

    #[test]
    fn reply_parsing() {
        let lines = [
            r#"{"event":"playback-restart"}"#,
            r#"{"data":12.5,"request_id":1,"error":"success"}"#,
            r#"{"error":"property unavailable","request_id":2}"#,
        ];
        assert_eq!(parse_reply(&lines, 1), Some(12.5));
        assert_eq!(parse_reply(&lines, 2), None);
        assert_eq!(parse_reply(&lines, 3), None);
        assert_eq!(parse_reply(&["garbage"], 1), None);
    }

    #[test]
    fn volume_clamps() {
        let mut p = Player::new(98);
        p.change_volume(5).unwrap();
        assert_eq!(p.volume, 100);
        p.change_volume(-250).unwrap();
        assert_eq!(p.volume, 0);
        assert_eq!(Player::new(200).volume, 100);
    }

    #[test]
    fn command_builders() {
        let s = std::path::Path::new("/tmp/x.sock");
        let args = |c: &Command| -> Vec<String> {
            c.get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };
        let w = args(&watchdog_command(1, 2, s));
        assert_eq!(w[3..], ["1", "2", "/tmp/x.sock"]);
        let m = args(&mpv_command("mpv", "u", s, true, 42, 0, false));
        assert!(m.contains(&"--no-video".into()));
        assert!(m.contains(&"--loop-file=inf".into()));
        assert!(m.contains(&"--volume=42".into()));
        assert!(
            !args(&mpv_command("mpv", "u", s, false, 42, 0, false))
                .iter()
                .any(|a| a.starts_with("--loop") || a.starts_with("--start"))
        );
        assert!(
            args(&mpv_command("mpv", "u", s, false, 42, 95, false)).contains(&"--start=95".into())
        );
        assert!(!m.contains(&"--pause=yes".into()));
        let pre = args(&mpv_command("mpv", "u", s, false, 0, 0, true));
        assert!(pre.contains(&"--pause=yes".into()) && pre.contains(&"--volume=0".into()));
        assert!(m.contains(&"--input-ipc-server=/tmp/x.sock".into()));
        assert_eq!(m.last().unwrap(), "u");
    }

    /// Serve `reply` to one connection on a fresh socket; returns what the client sent.
    fn serve(tag: &str, reply: &'static str) -> (PathBuf, std::thread::JoinHandle<String>) {
        let p = std::env::temp_dir().join(format!("termusic-sv-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let l = std::os::unix::net::UnixListener::bind(&p).unwrap();
        let h = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut line = String::new();
            std::io::BufRead::read_line(&mut std::io::BufReader::new(&s), &mut line).unwrap();
            s.write_all(reply.as_bytes()).unwrap();
            line
        });
        (p, h)
    }

    #[test]
    fn ipc_to_wraps_the_command_and_errors_without_a_socket() {
        let (p, h) = serve("ipc", "");
        ipc_to(&p, r#"["cycle","pause"]"#).unwrap();
        assert_eq!(h.join().unwrap(), "{\"command\":[\"cycle\",\"pause\"]}\n");
        assert!(ipc_to(&std::env::temp_dir().join("termusic-none"), "[]").is_err());
    }

    #[test]
    fn poll_reads_position_and_duration_and_tolerates_a_dead_socket() {
        let (p, h) = serve(
            "poll",
            "{\"event\":\"x\"}\n{\"data\":1.5,\"request_id\":1,\"error\":\"success\"}\n{\"data\":200.0,\"request_id\":2,\"error\":\"success\"}\n",
        );
        assert_eq!(poll(&p), (Some(1.5), Some(200.0)));
        assert!(h.join().unwrap().contains("time-pos"));
        assert_eq!(
            poll(&std::env::temp_dir().join("termusic-none")),
            (None, None)
        );
    }

    #[test]
    fn quiet_detaches_stdio() {
        // cat exits at once only because stdin is /dev/null
        assert!(quiet(Command::new("cat")).status().unwrap().success());
    }

    #[test]
    fn preloaded_id_and_end_current_keep_the_preload() {
        let mut p = Player::default();
        assert_eq!(p.preloaded_id(), None);
        let r = fake(&mut p, "c", "60");
        p.set_current(r);
        let r = fake(&mut p, "n", "60");
        p.next = Some(r);
        assert_eq!(p.preloaded_id(), Some("n"));
        p.position = 9.0;
        p.end_current();
        assert!(p.current.is_none() && !p.playing && p.now_playing.is_none());
        assert_eq!((p.position, p.duration), (0.0, None));
        assert_eq!(p.preloaded_id(), Some("n"));
        p.stop();
    }

    #[test]
    fn maybe_start_fade_only_in_the_last_seconds_while_playing() {
        let (mut p, [_lc, ln]) = rig("mfade");
        p.duration = Some(100.0);
        p.position = 50.0;
        p.maybe_start_fade();
        assert!(p.fade.is_none());
        p.position = 98.0;
        p.playing = false;
        p.maybe_start_fade();
        assert!(p.fade.is_none());
        p.playing = true;
        p.maybe_start_fade();
        let f = p.fade.as_ref().unwrap();
        assert!((f.len - 2.0).abs() < 1e-9);
        assert!(drain(&ln).contains(&pause_json(false)));
        p.stop();
    }

    #[test]
    fn cancel_fade_repauses_preload_and_restores_volume() {
        let (mut p, [lc, ln]) = rig("cancel");
        p.cancel_fade(); // no fade: nothing is sent
        assert!(drain(&lc).is_empty() && drain(&ln).is_empty());
        start_fade(&mut p, 1.0);
        p.cancel_fade();
        assert!(p.fade.is_none());
        assert!(drain(&lc).contains(&volume_json(80)));
        let n = drain(&ln);
        assert!(n.contains(&pause_json(true)) && n.contains(&volume_json(0)));
        p.stop();
    }

    #[test]
    fn set_loop_file_sends_inf_or_no() {
        let (mut p, [lc, _ln]) = rig("loopf");
        p.set_loop_file(true).unwrap();
        p.set_loop_file(false).unwrap();
        let sent = drain(&lc);
        assert!(
            sent.contains(r#"["set_property","loop-file","inf"]"#),
            "{sent}"
        );
        assert!(
            sent.contains(r#"["set_property","loop-file","no"]"#),
            "{sent}"
        );
        assert!(Player::default().set_loop_file(true).is_ok()); // nothing playing: no-op
        p.stop();
    }

    #[test]
    fn missing_deps_names_only_what_is_absent() {
        // depends on the machine's PATH: just check the message shape
        if let Some(m) = missing_deps() {
            assert!(m.starts_with("missing ") && (m.contains("mpv") || m.contains("yt-dlp")));
        }
    }
}
