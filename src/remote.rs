//! Local control socket: a menu-bar helper (or any script) sends JSON lines and gets state back.
use crate::app::Msg;
use crate::cmd::Cmd;
use crate::queue::Loop;
use crate::theme;
use serde::Serialize;
use serde_json::{Value, json};
use std::fs::File;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc::{Sender, UnboundedSender, channel};
use tokio::sync::{Notify, watch};

const MAX_CLIENTS: usize = 4;
/// Longest request line, without its newline.
const MAX_LINE: u64 = 4096;
/// Replies waiting for one client; a full queue drops it.
const OUT_QUEUE: usize = 16;
/// A client that does not take a line in this long is closed.
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);
/// macOS `sun_path` is 104 bytes including the NUL.
const MAX_PATH: usize = 103;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Cmd(Cmd),
    Volume(u8),
    Loop(Loop),
    Shuffle(bool),
    Shell(String),
    Thumbnails(bool),
    Animations(bool),
    Hiss(bool),
    HissLevel(u8),
    GetState,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ShellInfo {
    pub id: &'static str,
    pub name: &'static str,
}

/// What clients see; built by `App::snapshot`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct State {
    pub title: Option<String>,
    pub channel: Option<String>,
    pub id: Option<String>,
    pub loaded: bool,
    pub paused: bool,
    pub position: f64,
    pub duration: Option<f64>,
    pub volume: u8,
    #[serde(rename = "loop")]
    pub looping: &'static str,
    pub shuffle: bool,
    pub shell: &'static str,
    pub shells: Vec<ShellInfo>,
    pub thumbnails: bool,
    pub animations: bool,
    pub hiss: bool,
    pub hiss_level: u8,
}

impl Default for State {
    fn default() -> Self {
        Self {
            title: None,
            channel: None,
            id: None,
            loaded: false,
            paused: false,
            position: 0.0,
            duration: None,
            volume: 0,
            looping: "off",
            shuffle: false,
            shell: theme::SHELLS[0].id,
            shells: Vec::new(),
            thumbnails: false,
            animations: false,
            hiss: false,
            hiss_level: 0,
        }
    }
}

/// One request line into a `Request`; the error text goes back to the client.
pub fn parse(line: &[u8]) -> Result<Request, String> {
    let text = std::str::from_utf8(line).map_err(|_| "not UTF-8")?;
    let v: Value = serde_json::from_str(text).map_err(|_| "not JSON")?;
    let o = v.as_object().ok_or("not an object")?;
    if let Some(c) = o.get("cmd") {
        let c = c.as_str().ok_or("cmd must be a string")?;
        return Ok(Request::Cmd(match c {
            "play_pause" => Cmd::PlayPause,
            "next" => Cmd::Next,
            "prev" => Cmd::Prev,
            "toggle_shuffle" => Cmd::ToggleShuffle,
            "cycle_loop" => Cmd::CycleLoop,
            "seek" => {
                let n = o
                    .get("secs")
                    .and_then(Value::as_f64)
                    .filter(|f| f.fract() == 0.0);
                Cmd::Seek(n.ok_or("seek needs integer secs")?.clamp(-600.0, 600.0) as i32)
            }
            _ => return Err(format!("unknown cmd: {c}")),
        }));
    }
    if let Some(k) = o.get("set") {
        let k = k.as_str().ok_or("set must be a string")?;
        let val = o.get("value");
        let b = || {
            val.and_then(Value::as_bool)
                .ok_or("value must be true or false")
        };
        let pct = || {
            val.and_then(Value::as_u64)
                .filter(|n| *n <= 100)
                .map(|n| n as u8)
                .ok_or("value must be 0-100")
        };
        let s = || val.and_then(Value::as_str).ok_or("value must be a string");
        return Ok(match k {
            "volume" => Request::Volume(pct()?),
            "hiss_level" => Request::HissLevel(pct()?),
            "shuffle" => Request::Shuffle(b()?),
            "thumbnails" => Request::Thumbnails(b()?),
            "animations" => Request::Animations(b()?),
            "hiss" => Request::Hiss(b()?),
            "loop" => Request::Loop(match s()? {
                "off" => Loop::Off,
                "one" => Loop::One,
                "all" => Loop::All,
                _ => return Err("loop must be off, one or all".into()),
            }),
            "shell" => {
                let id = s()?;
                if !theme::SHELLS.iter().any(|sh| sh.id == id) {
                    return Err(format!("unknown shell: {id}"));
                }
                Request::Shell(id.into())
            }
            _ => return Err(format!("unknown set key: {k}")),
        });
    }
    match o.get("get").map(|g| g.as_str()) {
        Some(Some("state")) => Ok(Request::GetState),
        Some(_) => Err("unknown get key".into()),
        None => Err("no cmd, set or get".into()),
    }
}

pub fn socket_path() -> Option<PathBuf> {
    crate::settings::config_dir().map(|d| d.join("termusic.sock"))
}

/// A claimed socket: the listener, the process lock to keep until exit, and the socket's (dev, ino).
#[derive(Debug)]
pub struct Bound {
    pub listener: UnixListener,
    pub lock: File,
    pub id: (u64, u64),
}

/// The (dev, ino) of whatever is at `p` now, if anything.
pub fn file_id(p: &Path) -> Option<(u64, u64)> {
    std::fs::symlink_metadata(p)
        .ok()
        .map(|m| (m.dev(), m.ino()))
}

/// Claim the socket path (see the ownership rule in the spec); the error is a status line.
/// A lock file in the same directory says who owns it; the socket appears at its real path
/// only once it is bound and mode 0600.
pub fn bind(path: &Path) -> Result<Bound, String> {
    let err = |e: std::io::Error| format!("remote control: {e}");
    let tmp = PathBuf::from(format!("{}.{}", path.display(), std::process::id()));
    if tmp.as_os_str().len() > MAX_PATH {
        return Err("remote control: socket path too long".into());
    }
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(dir)
        .map_err(err)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .open(dir.join("termusic.lock"))
        .map_err(err)?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            return Err("remote control: another termusic is running".into());
        }
        Err(std::fs::TryLockError::Error(e)) => return Err(err(e)),
    }
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_socket() => {
            if std::os::unix::net::UnixStream::connect(path).is_ok() {
                return Err("remote control: another termusic is running".into());
            }
            std::fs::remove_file(path).map_err(err)?; // nothing answered: a crash left it
        }
        Ok(_) => {
            return Err("remote control: termusic.sock exists and is not a socket".into());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(err(e)),
    }
    let _ = std::fs::remove_file(&tmp);
    let listener = UnixListener::bind(&tmp).map_err(err)?;
    let done = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
        .and_then(|()| std::fs::rename(&tmp, path));
    if let Err(e) = done {
        let _ = std::fs::remove_file(&tmp);
        return Err(err(e));
    }
    let id = file_id(path).ok_or("remote control: socket vanished")?;
    Ok(Bound { listener, lock, id })
}

/// Where a client's replies go: its bounded outgoing queue. A full queue drops the client.
#[derive(Clone)]
pub struct Reply {
    out: Sender<Result<State, String>>,
    kill: Arc<Notify>,
}

impl Reply {
    pub fn send(&self, r: Result<State, String>) {
        if self.out.try_send(r).is_err() {
            self.kill.notify_one();
        }
    }
}

/// Accept clients in the background; each request line becomes a `Msg::Remote` for the UI thread.
/// Returns the live-client counter (for tests).
pub fn serve(
    l: UnixListener,
    tx: UnboundedSender<Msg>,
    state: watch::Receiver<State>,
) -> Arc<AtomicUsize> {
    let n = Arc::new(AtomicUsize::new(0));
    let live = n.clone();
    tokio::spawn(async move {
        loop {
            let Ok((s, _)) = l.accept().await else {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            if n.load(Ordering::SeqCst) >= MAX_CLIENTS {
                continue; // dropping `s` closes it
            }
            n.fetch_add(1, Ordering::SeqCst); // only this task adds, so the cap holds
            tokio::spawn(client(
                s,
                tx.clone(),
                state.clone(),
                Arc::new(Guard(n.clone())),
            ));
        }
    });
    live
}

/// The slot is freed when the last task of a client (reader or writer) drops its copy.
struct Guard(Arc<AtomicUsize>);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Reads requests here, never waiting for replies; a writer task sends state changes and replies.
async fn client(
    s: UnixStream,
    tx: UnboundedSender<Msg>,
    mut state: watch::Receiver<State>,
    guard: Arc<Guard>,
) {
    let (r, mut w) = s.into_split();
    let (out, mut replies) = channel::<Result<State, String>>(OUT_QUEUE);
    let kill = Arc::new(Notify::new());
    let reply = Reply {
        out,
        kill: kill.clone(),
    };
    let writer_guard = guard.clone();
    tokio::spawn(async move {
        let _guard = writer_guard;
        let mut next = Some(Ok(state.borrow_and_update().clone()));
        let mut last = None;
        loop {
            let item = match next.take() {
                Some(i) => i,
                None => tokio::select! {
                    c = state.changed() => {
                        if c.is_err() { break }
                        let s = state.borrow_and_update().clone();
                        if last.as_ref() == Some(&s) { continue } // the reply just carried it
                        Ok(s)
                    }
                    m = replies.recv() => match m { Some(m) => m, None => break },
                },
            };
            let line = match item {
                Ok(s) => {
                    // A reply can be older than a change already written: follow up with the newest.
                    let cur = state.borrow().clone();
                    next = (cur != s).then_some(Ok(cur));
                    last = Some(s.clone());
                    json!({ "state": s })
                }
                Err(e) => json!({ "error": e }),
            };
            let bytes = format!("{line}\n");
            let write = w.write_all(bytes.as_bytes());
            if !matches!(tokio::time::timeout(WRITE_TIMEOUT, write).await, Ok(Ok(()))) {
                break;
            }
        }
    });
    let mut r = BufReader::new(r);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        // A line over the cap leaves no newline within the limit: close.
        let mut limited = (&mut r).take(MAX_LINE + 1);
        let read = tokio::select! {
            _ = kill.notified() => return,
            n = limited.read_until(b'\n', &mut buf) => n,
        };
        match read {
            Ok(n) if n > 0 && buf.ends_with(b"\n") => {}
            _ => return,
        }
        // Errors go through the app too, so replies keep the order of the requests.
        if tx.send(Msg::Remote(parse(&buf), reply.clone())).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Key;
    use crate::app::{App, Screen};
    use tokio::io::AsyncBufReadExt;
    use tokio::net::unix::OwnedWriteHalf;

    type Reader = BufReader<tokio::net::unix::OwnedReadHalf>;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tm-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn parse_every_request() {
        let ok = |s: &str| parse(s.as_bytes()).unwrap();
        for (line, want) in [
            (r#"{"cmd":"play_pause"}"#, Request::Cmd(Cmd::PlayPause)),
            (r#"{"cmd":"next"}"#, Request::Cmd(Cmd::Next)),
            (r#"{"cmd":"prev"}"#, Request::Cmd(Cmd::Prev)),
            (r#"{"cmd":"seek","secs":-5}"#, Request::Cmd(Cmd::Seek(-5))),
            (
                r#"{"cmd":"seek","secs":9999}"#,
                Request::Cmd(Cmd::Seek(600)),
            ),
            (
                r#"{"cmd":"seek","secs":-9999}"#,
                Request::Cmd(Cmd::Seek(-600)),
            ),
            (
                r#"{"cmd":"seek","secs":99999999999999999999}"#,
                Request::Cmd(Cmd::Seek(600)),
            ),
            (
                r#"{"cmd":"toggle_shuffle"}"#,
                Request::Cmd(Cmd::ToggleShuffle),
            ),
            (r#"{"cmd":"cycle_loop"}"#, Request::Cmd(Cmd::CycleLoop)),
            (r#"{"set":"volume","value":40}"#, Request::Volume(40)),
            (r#"{"set":"volume","value":0}"#, Request::Volume(0)),
            (r#"{"set":"volume","value":100}"#, Request::Volume(100)),
            (r#"{"set":"loop","value":"off"}"#, Request::Loop(Loop::Off)),
            (r#"{"set":"loop","value":"one"}"#, Request::Loop(Loop::One)),
            (r#"{"set":"loop","value":"all"}"#, Request::Loop(Loop::All)),
            (r#"{"set":"shuffle","value":true}"#, Request::Shuffle(true)),
            (
                r#"{"set":"shell","value":"amber"}"#,
                Request::Shell("amber".into()),
            ),
            (
                r#"{"set":"thumbnails","value":true}"#,
                Request::Thumbnails(true),
            ),
            (
                r#"{"set":"animations","value":false}"#,
                Request::Animations(false),
            ),
            (r#"{"set":"hiss","value":true}"#, Request::Hiss(true)),
            (r#"{"set":"hiss_level","value":30}"#, Request::HissLevel(30)),
            (r#"{"get":"state"}"#, Request::GetState),
            ("{\"get\":\"state\"}\n", Request::GetState),
        ] {
            assert_eq!(ok(line), want, "{line}");
        }
    }

    #[test]
    fn parse_rejects_bad_input() {
        for line in [
            r#"{"cmd":"explode"}"#,
            r#"{"cmd":5}"#,
            r#"{"cmd":"seek"}"#,
            r#"{"cmd":"seek","secs":"5"}"#,
            r#"{"cmd":"seek","secs":1.5}"#,
            r#"{"set":"foo","value":1}"#,
            r#"{"set":"volume"}"#,
            r#"{"set":"volume","value":"40"}"#,
            r#"{"set":"volume","value":101}"#,
            r#"{"set":"volume","value":-1}"#,
            r#"{"set":"hiss_level","value":101}"#,
            r#"{"set":"hiss","value":1}"#,
            r#"{"set":"shell","value":"bogus"}"#,
            r#"{"set":"shell","value":3}"#,
            r#"{"set":"loop","value":"twice"}"#,
            r#"{"get":"volume"}"#,
            r#"{"get":1}"#,
            r#"{}"#,
            r#"[1]"#,
            "not json",
            "",
            "\n",
        ] {
            assert!(parse(line.as_bytes()).is_err(), "{line}");
        }
        assert_eq!(parse(b"{\"cmd\":\"\xff\"}"), Err("not UTF-8".into()));
    }

    #[test]
    fn state_has_the_spec_field_names_and_nulls() {
        let s = State {
            shells: theme::SHELLS
                .iter()
                .map(|s| ShellInfo {
                    id: s.id,
                    name: s.name,
                })
                .collect(),
            ..State::default()
        };
        let line = json!({ "state": s });
        println!("{line}");
        let want = json!({"state": {
            "title": null, "channel": null, "id": null, "loaded": false, "paused": false,
            "position": 0.0, "duration": null, "volume": 0, "loop": "off", "shuffle": false,
            "shell": "tps-l2",
            "shells": [{"id":"tps-l2","name":"TPS-L2"},{"id":"sunset","name":"SUNSET TAPE"},{"id":"amber","name":"AMBER HI-FI"}],
            "thumbnails": false, "animations": false, "hiss": false, "hiss_level": 0
        }});
        assert_eq!(line, want);
    }

    async fn line(r: &mut Reader) -> String {
        let mut s = String::new();
        tokio::time::timeout(Duration::from_secs(5), r.read_line(&mut s))
            .await
            .expect("timed out")
            .unwrap();
        s
    }

    /// The next line containing `needle`; a state line may arrive twice (reply and change).
    async fn until(r: &mut Reader, needle: &str) -> String {
        for _ in 0..60 {
            let l = line(r).await;
            if l.contains(needle) {
                return l;
            }
        }
        panic!("no line with {needle}");
    }

    async fn connect(p: &Path) -> (Reader, tokio::net::unix::OwnedWriteHalf) {
        let (r, w) = UnixStream::connect(p).await.unwrap().into_split();
        (BufReader::new(r), w)
    }

    /// Let the reader task forward the request, then apply it like the main loop would.
    async fn pump(a: &mut App) {
        tokio::time::sleep(Duration::from_millis(100)).await;
        a.tick();
    }

    #[tokio::test]
    async fn requests_change_the_app_and_get_state_back() {
        let dir = tmp("int");
        let sock = dir.join("t.sock");
        let mut a = App::blank();
        a.serve_remote(Some(sock.clone()));
        assert_eq!(a.sock.as_ref().map(|s| &s.0), Some(&sock));
        let (mut r, mut w) = connect(&sock).await;
        let v: Value = serde_json::from_str(&line(&mut r).await).unwrap();
        assert_eq!(v["state"]["volume"], 70);
        w.write_all(b"{\"set\":\"volume\",\"value\":40}\n")
            .await
            .unwrap();
        pump(&mut a).await;
        let v: Value = serde_json::from_str(&until(&mut r, r#""volume":40"#).await).unwrap();
        assert_eq!(v["state"]["volume"], 40);
        assert_eq!(a.player.volume, 40);
        w.write_all(b"{\"set\":\"shell\",\"value\":\"amber\"}\n")
            .await
            .unwrap();
        pump(&mut a).await;
        assert_eq!(a.settings.shell, "amber");
        until(&mut r, r#""shell":"amber""#).await;
        // a request that changes nothing still gets one state line
        w.write_all(b"{\"get\":\"state\"}\n").await.unwrap();
        pump(&mut a).await;
        assert!(line(&mut r).await.starts_with(r#"{"state""#));
        // garbage: error line, connection stays open, nothing changes
        w.write_all(b"\xff\xfe\n{\"set\":\"volume\",\"value\":500}\n")
            .await
            .unwrap();
        pump(&mut a).await; // errors travel through the app to keep reply order
        until(&mut r, r#"{"error""#).await;
        until(&mut r, r#"{"error""#).await;
        assert_eq!(a.player.volume, 40);
        // a command takes the key-press path
        w.write_all(b"{\"cmd\":\"toggle_shuffle\"}\n")
            .await
            .unwrap();
        pump(&mut a).await;
        assert!(a.queue.shuffle);
        until(&mut r, r#""shuffle":true"#).await;
        a.shutdown();
        assert!(!sock.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn oversize_line_closes_the_client() {
        let dir = tmp("big");
        let sock = dir.join("t.sock");
        let mut a = App::blank();
        a.serve_remote(Some(sock.clone()));
        let (mut r, mut w) = connect(&sock).await;
        line(&mut r).await;
        w.write_all(&[b'a'; 5000]).await.unwrap();
        assert_eq!(line(&mut r).await, ""); // EOF
        // 4097 content bytes: one over, closed even though a newline follows
        let (mut r, mut w) = connect(&sock).await;
        line(&mut r).await;
        let mut big = vec![b' '; MAX_LINE as usize + 1];
        big.push(b'\n');
        w.write_all(&big).await.unwrap();
        assert_eq!(line(&mut r).await, "");
        // exactly the cap is fine
        let (mut r, mut w) = connect(&sock).await;
        line(&mut r).await;
        let mut ok = vec![b' '; MAX_LINE as usize - 15];
        ok.extend_from_slice(br#"{"get":"state"}"#);
        ok.push(b'\n');
        assert_eq!(ok.len(), MAX_LINE as usize + 1); // 4096 content bytes and the newline
        w.write_all(&ok).await.unwrap();
        pump(&mut a).await;
        assert!(line(&mut r).await.starts_with(r#"{"state""#));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_fifth_client_is_closed_and_four_stay() {
        let dir = tmp("five");
        let sock = dir.join("t.sock");
        let mut a = App::blank();
        a.serve_remote(Some(sock.clone()));
        let mut four = Vec::new();
        for _ in 0..MAX_CLIENTS {
            let (mut r, w) = connect(&sock).await;
            assert!(line(&mut r).await.starts_with(r#"{"state""#));
            four.push((r, w));
        }
        let (mut r5, _w5) = connect(&sock).await;
        assert_eq!(line(&mut r5).await, "");
        for (_, w) in &mut four {
            w.write_all(b"{\"get\":\"state\"}\n").await.unwrap();
        }
        pump(&mut a).await;
        for (r, _) in &mut four {
            assert!(line(r).await.starts_with(r#"{"state""#));
        }
        // a freed slot can be taken again
        drop(four);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let (mut r, _w) = connect(&sock).await;
        assert!(line(&mut r).await.starts_with(r#"{"state""#));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_too_long_path_is_refused() {
        let long = PathBuf::from(format!("/tmp/{}", "x".repeat(MAX_PATH)));
        assert!(bind(&long).unwrap_err().contains("too long"));
        let mut a = App::blank();
        a.serve_remote(Some(long));
        assert!(a.status.contains("socket path too long") && a.sock.is_none());
    }

    /// Serve `a` on a fresh socket; returns the dir, the socket and a connected client.
    async fn setup(tag: &str, a: &mut App) -> (PathBuf, PathBuf, Reader, OwnedWriteHalf) {
        let dir = tmp(tag);
        let sock = dir.join("t.sock");
        a.serve_remote(Some(sock.clone()));
        let (mut r, w) = connect(&sock).await;
        line(&mut r).await; // the initial state
        (dir, sock, r, w)
    }

    async fn ask(a: &mut App, r: &mut Reader, w: &mut OwnedWriteHalf, req: &str, want: &str) {
        w.write_all(format!("{req}\n").as_bytes()).await.unwrap();
        pump(a).await;
        until(r, want).await;
    }

    #[test]
    fn parse_boundaries() {
        let ok = |s: &str| parse(s.as_bytes());
        assert_eq!(
            ok(r#"{"set":"hiss_level","value":0}"#),
            Ok(Request::HissLevel(0))
        );
        assert_eq!(
            ok(r#"{"set":"hiss_level","value":100}"#),
            Ok(Request::HissLevel(100))
        );
        for (n, want) in [(600, 600), (-600, -600), (601, 600), (-601, -600)] {
            let line = format!(r#"{{"cmd":"seek","secs":{n}}}"#);
            assert_eq!(ok(&line), Ok(Request::Cmd(Cmd::Seek(want))), "{n}");
        }
        for sh in theme::SHELLS {
            let line = format!(r#"{{"set":"shell","value":"{}"}}"#, sh.id);
            assert_eq!(ok(&line), Ok(Request::Shell(sh.id.into())));
        }
        assert!(ok(r#"{"set":"shell","value":"AMBER"}"#).is_err());
        // the documented contract is integers: a float volume stays rejected
        assert!(ok(r#"{"set":"volume","value":40.0}"#).is_err());
    }

    #[tokio::test]
    async fn every_request_arm_changes_the_app() {
        let mut a = App::blank();
        let (dir, _s, mut r, mut w) = setup("arms", &mut a).await;
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"set":"volume","value":33}"#,
            r#""volume":33"#,
        )
        .await;
        assert_eq!(a.player.volume, 33);
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"set":"loop","value":"all"}"#,
            r#""loop":"all""#,
        )
        .await;
        assert_eq!(a.queue.looping, Loop::All);
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"set":"shuffle","value":true}"#,
            r#""shuffle":true"#,
        )
        .await;
        assert!(a.queue.shuffle);
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"set":"thumbnails","value":true}"#,
            r#""thumbnails":true"#,
        )
        .await;
        assert!(a.settings.thumbnails);
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"set":"animations","value":true}"#,
            r#""animations":true"#,
        )
        .await;
        assert!(a.settings.animations && a.player.meters);
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"set":"hiss","value":true}"#,
            r#""hiss":true"#,
        )
        .await;
        assert!(a.settings.hiss && a.player.hiss_on);
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"set":"hiss_level","value":45}"#,
            r#""hiss_level":45"#,
        )
        .await;
        assert_eq!((a.settings.hiss_level, a.player.hiss_level), (45, 45));
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"set":"shell","value":"sunset"}"#,
            r#""shell":"sunset""#,
        )
        .await;
        assert_eq!(a.settings.shell, "sunset");
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"cmd":"cycle_loop"}"#,
            r#""loop":"off""#,
        )
        .await;
        assert_eq!(a.queue.looping, Loop::Off); // all -> off
        ask(&mut a, &mut r, &mut w, r#"{"get":"state"}"#, r#"{"state""#).await;
        a.shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn two_get_states_make_two_state_lines() {
        let mut a = App::blank();
        let (dir, _s, mut r, mut w) = setup("two", &mut a).await;
        w.write_all(b"{\"get\":\"state\"}\n{\"get\":\"state\"}\n")
            .await
            .unwrap();
        pump(&mut a).await;
        assert!(line(&mut r).await.starts_with(r#"{"state""#));
        assert!(line(&mut r).await.starts_with(r#"{"state""#));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_burst_of_requests_is_applied_in_a_tick_or_two() {
        let mut a = App::blank();
        let (dir, _s, mut r, mut w) = setup("burst", &mut a).await;
        let burst: String = (31..=60)
            .map(|v| format!("{{\"set\":\"volume\",\"value\":{v}}}\n"))
            .collect();
        w.write_all(burst.as_bytes()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        a.tick();
        a.tick();
        assert_eq!(a.player.volume, 60);
        until(&mut r, r#""volume":60"#).await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn other_clients_and_the_app_itself_reach_every_client() {
        let mut a = App::blank();
        let (dir, sock, mut ra, _wa) = setup("fan", &mut a).await;
        let (mut rb, mut wb) = connect(&sock).await;
        line(&mut rb).await;
        ask(
            &mut a,
            &mut rb,
            &mut wb,
            r#"{"set":"volume","value":12}"#,
            r#""volume":12"#,
        )
        .await;
        // A sent nothing and still ends up showing B's value
        until(&mut ra, r#""volume":12"#).await;
        // the app changing itself (a key, a setter) with no request anywhere
        a.set_volume(25);
        a.tick();
        until(&mut ra, r#""volume":25"#).await;
        until(&mut rb, r#""volume":25"#).await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn offline_rejects_commands_but_not_settings() {
        let mut a = App::blank();
        a.screen = Screen::Offline;
        let (dir, _s, mut r, mut w) = setup("off", &mut a).await;
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"cmd":"toggle_shuffle"}"#,
            r#"{"error":"offline"}"#,
        )
        .await;
        assert!(!a.queue.shuffle);
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"set":"volume","value":20}"#,
            r#""volume":20"#,
        )
        .await;
        ask(&mut a, &mut r, &mut w, r#"{"get":"state"}"#, r#"{"state""#).await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn remote_mode_changes_press_their_cap() {
        let mut a = App::blank();
        a.settings.animations = true;
        let (dir, _s, mut r, mut w) = setup("cap", &mut a).await;
        let key = |a: &App| a.anim.pressed.map(|(k, _)| k);
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"cmd":"toggle_shuffle"}"#,
            r#""shuffle":true"#,
        )
        .await;
        assert_eq!(key(&a), Some(Key::Shuffle));
        a.anim.pressed = None;
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"set":"shuffle","value":true}"#,
            r#"{"state""#,
        )
        .await;
        assert_eq!(key(&a), None); // no change, no press
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"set":"shuffle","value":false}"#,
            r#""shuffle":false"#,
        )
        .await;
        assert_eq!(key(&a), Some(Key::Shuffle));
        a.anim.pressed = None;
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"set":"loop","value":"off"}"#,
            r#"{"state""#,
        )
        .await;
        assert_eq!(key(&a), None);
        ask(
            &mut a,
            &mut r,
            &mut w,
            r#"{"set":"loop","value":"one"}"#,
            r#""loop":"one""#,
        )
        .await;
        assert_eq!(key(&a), Some(Key::Loop));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn snapshot_loaded_empty_paused_and_odd_positions() {
        let mut a = App::blank();
        let s = a.snapshot();
        assert!(!s.loaded && !s.paused);
        assert_eq!(
            (s.title, s.channel, s.id, s.duration, s.position),
            (None, None, None, None, 0.0)
        );
        a.queue.set(
            vec![crate::youtube::Track {
                title: "T".into(),
                channel: "C".into(),
                id: "I".into(),
            }],
            0,
        );
        a.player.now_playing = Some("T".into());
        a.player.playing = true;
        a.player.position = 12.9;
        a.player.duration = Some(100.0);
        let s = a.snapshot();
        assert!(s.loaded && !s.paused);
        assert_eq!(s.position, 12.0);
        assert_eq!(s.duration, Some(100.0));
        assert_eq!(
            (s.title.as_deref(), s.channel.as_deref(), s.id.as_deref()),
            (Some("T"), Some("C"), Some("I"))
        );
        a.player.playing = false;
        assert!(a.snapshot().paused);
        for bad in [f64::NAN, f64::INFINITY, -3.0] {
            a.player.position = bad;
            assert_eq!(a.snapshot().position, 0.0, "{bad}");
        }
        // nothing loaded: the queue's track is not reported
        a.player.now_playing = None;
        let s = a.snapshot();
        assert_eq!((s.title, s.id, s.duration), (None, None, None));
    }

    #[test]
    fn disabled_commands_are_rejected_like_the_keyboard() {
        let a = App::blank();
        for (c, key) in [
            (Cmd::PlayPause, 'p'),
            (Cmd::Next, 'n'),
            (Cmd::Prev, 'b'),
            (Cmd::Seek(-5), ','),
            (Cmd::Seek(5), '.'),
            (Cmd::ToggleShuffle, 's'),
            (Cmd::CycleLoop, 'l'),
        ] {
            let off = |k| k == key;
            assert_eq!(a.cmd_gate(c, off), Err("disabled".into()), "{c:?}");
            assert_eq!(a.cmd_gate(c, |_| false), Ok(()));
        }
    }

    #[tokio::test]
    async fn slots_free_when_a_client_that_never_reads_is_gone() {
        let dir = tmp("slots");
        let sock = dir.join("t.sock");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (_st, state) = watch::channel(State::default());
        let live = serve(UnixListener::bind(&sock).unwrap(), tx, state);
        tokio::spawn(async move {
            while let Some(m) = rx.recv().await {
                if let Msg::Remote(_, reply) = m {
                    reply.send(Ok(State::default()));
                }
            }
        });
        let peak = Arc::new(AtomicUsize::new(0));
        let (l, p) = (live.clone(), peak.clone());
        tokio::spawn(async move {
            loop {
                p.fetch_max(l.load(Ordering::SeqCst), Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        });
        let flood = b"{\"get\":\"state\"}\n".repeat(400);
        let mut kept = Vec::new();
        for _ in 0..12 {
            let (r, mut w) = connect(&sock).await;
            let _ = w.write_all(&flood).await;
            let _ = w.shutdown().await;
            kept.push(r); // never read from
        }
        let start = std::time::Instant::now();
        while live.load(Ordering::SeqCst) > 0 {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "slots never freed"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(peak.load(Ordering::SeqCst) <= MAX_CLIENTS);
        // the server side is closed: the unread client reaches EOF
        let mut rest = Vec::new();
        let mut first = kept.remove(0);
        tokio::time::timeout(Duration::from_secs(5), first.read_to_end(&mut rest))
            .await
            .expect("socket still open")
            .unwrap();
        let (mut r, _w) = connect(&sock).await;
        assert!(line(&mut r).await.starts_with(r#"{"state""#));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_second_app_is_locked_out_and_a_quit_frees_the_lock() {
        let mut a = App::blank();
        let (dir, sock, _r, _w) = setup("lock", &mut a).await;
        let mut b = App::blank();
        b.serve_remote(Some(sock.clone()));
        assert!(b.status.contains("another termusic"), "{}", b.status);
        assert!(b.sock.is_none());
        b.shutdown();
        assert!(sock.exists(), "not ours: must not be removed");
        a.shutdown();
        assert!(!sock.exists());
        let mut c = App::blank();
        c.serve_remote(Some(sock.clone()));
        assert!(c.sock.is_some() && c.status.is_empty(), "{}", c.status);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_held_lock_skips_remote_control_and_leaves_the_socket_alone() {
        let dir = tmp("held");
        let sock = dir.join("t.sock");
        let other = File::create(dir.join("termusic.lock")).unwrap();
        other.try_lock().unwrap();
        let mut a = App::blank();
        a.serve_remote(Some(sock.clone()));
        assert_eq!(a.status, "remote control: another termusic is running");
        assert!(a.sock.is_none() && !sock.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_path_that_is_not_a_socket_is_left_alone() {
        let dir = tmp("notsock");
        let sock = dir.join("t.sock");
        std::fs::write(&sock, "mine").unwrap();
        let mut a = App::blank();
        a.serve_remote(Some(sock.clone()));
        assert_eq!(
            a.status,
            "remote control: termusic.sock exists and is not a socket"
        );
        assert!(a.sock.is_none());
        assert_eq!(std::fs::read_to_string(&sock).unwrap(), "mine");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_dead_socket_is_replaced_with_mode_0600() {
        let dir = tmp("dead").join("fresh"); // the config dir is created too
        let sock = dir.join("t.sock");
        std::fs::create_dir_all(&dir).unwrap();
        drop(std::os::unix::net::UnixListener::bind(&sock).unwrap()); // file stays behind
        assert!(sock.exists());
        let mut a = App::blank();
        a.serve_remote(Some(sock.clone()));
        assert!(a.sock.is_some() && a.status.is_empty(), "{}", a.status);
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&sock), 0o600);
        UnixStream::connect(&sock).await.unwrap();
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(names.len(), 2, "socket and lock only: {names:?}");
        a.shutdown();
        // a missing config dir is created 0700
        let made = dir.join("deeper");
        let mut b = App::blank();
        b.serve_remote(Some(made.join("t.sock")));
        assert_eq!(mode(&made), 0o700);
        b.shutdown();
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[tokio::test]
    async fn shutdown_keeps_a_socket_that_is_no_longer_ours() {
        let mut a = App::blank();
        let (dir, sock, _r, _w) = setup("swap", &mut a).await;
        std::fs::rename(&sock, dir.join("old.sock")).unwrap(); // keeps the old inode in use
        let theirs = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        a.shutdown();
        assert!(sock.exists(), "another bind's socket must survive");
        drop(theirs);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn no_config_dir_means_no_remote_control() {
        let mut a = App::blank();
        a.serve_remote(None);
        assert_eq!(a.status, "remote control: no config directory");
        assert!(a.sock.is_none());
    }
}
