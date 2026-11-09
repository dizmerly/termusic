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

#[derive(Debug)]
pub struct Player {
    pub playing: bool,
    pub now_playing: Option<String>,
    /// 0..=100; owned here so every new mpv starts with it.
    pub volume: u8,
    /// Seconds into the current track.
    pub position: f64,
    pub duration: Option<f64>,
    running: Option<Running>,
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
    started: Instant,
    polled: Instant,
}

fn sock(app_pid: u32) -> PathBuf {
    std::env::temp_dir().join(format!("termusic-mpv-{app_pid}.sock"))
}

fn app_sock() -> PathBuf {
    sock(std::process::id())
}

fn mpv_command(url: &str, sock: &std::path::Path, loop_file: bool, volume: u8) -> Command {
    let mut c = Command::new("mpv");
    c.args(["--no-video", "--no-terminal", "--ytdl-format=bestaudio"])
        .arg(format!("--volume={volume}"))
        .arg(format!("--input-ipc-server={}", sock.display()))
        .args(loop_file.then_some("--loop-file=inf"))
        .arg(url);
    c
}

fn volume_json(vol: u8) -> String {
    format!(r#"["set_property","volume",{vol}]"#)
}

fn seek_json(delta: i32) -> String {
    format!(r#"["seek",{delta},"relative"]"#)
}

/// Polls once a second; once the app or mpv is gone, kills mpv and removes its socket.
const WATCHDOG: &str = r#"while kill -0 "$1" 2>/dev/null && kill -0 "$2" 2>/dev/null; do sleep 1; done
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
            running: None,
        }
    }

    /// Play `t`; `loop_file` repeats it until told otherwise (`set_loop_file`).
    pub fn play(&mut self, t: &Track, loop_file: bool) -> Result<()> {
        self.stop();
        let url = format!("https://www.youtube.com/watch?v={}", t.id);
        let mpv = quiet(mpv_command(&url, &app_sock(), loop_file, self.volume))
            .spawn()
            .context("cannot start mpv (mpv and yt-dlp must be on PATH)")?;
        self.attach(mpv, &t.title)
    }

    /// Track an already-spawned mpv (or stand-in) and start its watchdog.
    fn attach(&mut self, mut mpv: Child, title: &str) -> Result<()> {
        match quiet(watchdog_command(std::process::id(), mpv.id(), &app_sock())).spawn() {
            Ok(watchdog) => {
                self.running = Some(Running {
                    mpv,
                    watchdog,
                    started: Instant::now(),
                    polled: Instant::now(),
                });
                self.position = 0.0;
                self.duration = None;
                self.now_playing = Some(title.to_owned());
                self.playing = true;
                Ok(())
            }
            Err(e) => {
                let _ = mpv.kill(); // never leave an unwatched mpv behind
                let _ = mpv.wait();
                Err(e).context("cannot start watchdog (sh)")
            }
        }
    }

    pub fn stop(&mut self) {
        if let Some(mut r) = self.running.take() {
            for c in [&mut r.mpv, &mut r.watchdog] {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
        let _ = std::fs::remove_file(app_sock());
        self.playing = false;
        self.now_playing = None;
        self.position = 0.0;
        self.duration = None;
    }

    /// Send one JSON command to the running mpv (no-op when nothing is playing).
    fn ipc(&self, cmd: &str) -> Result<()> {
        if self.running.is_some() {
            UnixStream::connect(app_sock())
                .and_then(|mut s| writeln!(s, "{{\"command\":{cmd}}}"))
                .context("mpv not ready yet")?;
        }
        Ok(())
    }

    pub fn toggle_pause(&mut self) -> Result<()> {
        self.ipc(r#"["cycle","pause"]"#)?;
        if self.running.is_some() {
            self.playing = !self.playing;
        }
        Ok(())
    }

    /// Change the volume by `delta` points (clamped); the new value sticks even if mpv isn't ready.
    pub fn change_volume(&mut self, delta: i32) -> Result<()> {
        self.volume = (i32::from(self.volume) + delta).clamp(0, 100) as u8;
        self.ipc(&volume_json(self.volume))
    }

    /// Jump `delta` seconds (negative = back) in the current track.
    pub fn seek(&self, delta: i32) -> Result<()> {
        self.ipc(&seek_json(delta))
    }

    /// Repeat (or stop repeating) the track already playing.
    pub fn set_loop_file(&self, on: bool) -> Result<()> {
        let v = if on { "inf" } else { "no" };
        self.ipc(&format!(r#"["set_property","loop-file","{v}"]"#))
    }

    /// If mpv exited by itself: `Some(true)` for a normal end, `Some(false)` for an
    /// immediate exit (mpv/yt-dlp failure) so the caller doesn't skip through the list.
    pub fn tick(&mut self) -> Option<bool> {
        let r = self.running.as_mut()?;
        if r.mpv.try_wait().ok().flatten().is_none() {
            if r.polled.elapsed() >= Duration::from_secs(1) {
                r.polled = Instant::now();
                if let Some(p) = get_property("time-pos") {
                    self.position = p;
                }
                if let Some(d) = get_property("duration") {
                    self.duration = Some(d);
                }
            }
            return None;
        }
        let ok = r.started.elapsed() > Duration::from_secs(3);
        self.stop();
        Some(ok)
    }
}

/// Read one numeric mpv property; short timeouts so the UI thread never stalls.
fn get_property(name: &str) -> Option<f64> {
    let t = Duration::from_millis(50);
    let mut s = UnixStream::connect(app_sock()).ok()?;
    s.set_read_timeout(Some(t)).ok()?;
    s.set_write_timeout(Some(t)).ok()?;
    writeln!(
        s,
        r#"{{"command":["get_property","{name}"],"request_id":7}}"#
    )
    .ok()?;
    // mpv interleaves async events with replies: skip lines until ours.
    BufReader::new(s)
        .lines()
        .take(8)
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(&l).ok())
        .find(|v| v["request_id"] == 7)
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
        p.attach(sleeper("60"), "t").unwrap();
        assert!(p.playing);
        let (m, w) = {
            let r = p.running.as_ref().unwrap();
            (r.mpv.id(), r.watchdog.id())
        };
        p.stop();
        assert!(p.running.is_none() && p.now_playing.is_none() && !p.playing);
        assert!(!alive(m) && !alive(w));
    }

    #[test]
    fn tick_reports_running_and_early_exit() {
        let mut p = Player::default();
        assert_eq!(p.tick(), None); // nothing playing
        p.attach(sleeper("60"), "t").unwrap();
        assert_eq!(p.tick(), None);
        p.stop();
        p.attach(sleeper("0"), "t").unwrap();
        assert!(wait_until(|| p.running.as_mut().is_some_and(|r| r
            .mpv
            .try_wait()
            .unwrap()
            .is_some())));
        assert_eq!(p.tick(), Some(false)); // exited within 3s: failure, not a normal end
        assert!(p.running.is_none());
    }

    #[test]
    fn ipc_json() {
        assert_eq!(volume_json(80), r#"["set_property","volume",80]"#);
        assert_eq!(seek_json(5), r#"["seek",5,"relative"]"#);
        assert_eq!(seek_json(-5), r#"["seek",-5,"relative"]"#);
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
        let m = args(&mpv_command("u", s, true, 42));
        assert!(m.contains(&"--no-video".into()));
        assert!(m.contains(&"--loop-file=inf".into()));
        assert!(m.contains(&"--volume=42".into()));
        assert!(
            !args(&mpv_command("u", s, false, 42))
                .iter()
                .any(|a| a.starts_with("--loop"))
        );
        assert!(m.contains(&"--input-ipc-server=/tmp/x.sock".into()));
        assert_eq!(m.last().unwrap(), "u");
    }
}
