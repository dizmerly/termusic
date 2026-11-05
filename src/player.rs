//! Playback: `mpv --no-video` (yt-dlp backend) controlled over its JSON IPC socket.
use crate::youtube::Track;
use anyhow::{Context, Result};
use std::{
    io::Write,
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug, Default)]
pub struct Player {
    pub playing: bool,
    pub shuffle: bool,
    pub now_playing: Option<String>,
    child: Option<(Child, Instant)>,
}

fn sock() -> PathBuf {
    std::env::temp_dir().join(format!("termusic-mpv-{}.sock", std::process::id()))
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
    pub fn play(&mut self, t: &Track) -> Result<()> {
        self.stop();
        let child = Command::new("mpv")
            .args(["--no-video", "--no-terminal", "--ytdl-format=bestaudio"])
            .arg(format!("--input-ipc-server={}", sock().display()))
            .arg(format!("https://www.youtube.com/watch?v={}", t.id))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("cannot start mpv (mpv and yt-dlp must be on PATH)")?;
        self.child = Some((child, Instant::now()));
        self.now_playing = Some(t.title.clone());
        self.playing = true;
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some((mut c, _)) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        let _ = std::fs::remove_file(sock());
        self.playing = false;
        self.now_playing = None;
    }

    pub fn toggle_pause(&mut self) -> Result<()> {
        if self.child.is_some() {
            UnixStream::connect(sock())
                .and_then(|mut s| s.write_all(b"{\"command\":[\"cycle\",\"pause\"]}\n"))
                .context("mpv not ready yet")?;
            self.playing = !self.playing;
        }
        Ok(())
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
    }

    /// If mpv exited by itself: `Some(true)` for a normal end, `Some(false)` for an
    /// immediate exit (mpv/yt-dlp failure) so the caller doesn't skip through the list.
    pub fn tick(&mut self) -> Option<bool> {
        let (c, started) = self.child.as_mut()?;
        c.try_wait().ok().flatten()?;
        let ok = started.elapsed() > Duration::from_secs(3);
        self.stop();
        Some(ok)
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}
