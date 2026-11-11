//! Local control socket: a menu-bar helper (or any script) sends JSON lines and gets state back.
use crate::app::Msg;
use crate::cmd::Cmd;
use crate::queue::Loop;
use crate::theme;
use serde::Serialize;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::sync::{oneshot, watch};

const MAX_CLIENTS: usize = 4;
/// Longest request line, without its newline.
const MAX_LINE: u64 = 4096;
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

/// Claim the socket path (see the ownership rule in the spec); the error is a status line.
pub fn bind(path: &Path) -> Result<UnixListener, String> {
    if path.as_os_str().len() > MAX_PATH {
        return Err("remote control: socket path too long".into());
    }
    if path.exists() {
        if std::os::unix::net::UnixStream::connect(path).is_ok() {
            return Err("remote control: another termusic is running".into());
        }
        let _ = std::fs::remove_file(path); // nothing answered: a crash left it
    }
    let err = |e: std::io::Error| format!("remote control: {e}");
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(err)?;
    }
    let l = UnixListener::bind(path).map_err(err)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(err)?;
    Ok(l)
}

/// Accept clients in the background; each request becomes a `Msg::Remote` for the UI thread.
pub fn serve(l: UnixListener, tx: UnboundedSender<Msg>, state: watch::Receiver<State>) {
    tokio::spawn(async move {
        let n = Arc::new(AtomicUsize::new(0));
        loop {
            let Ok((s, _)) = l.accept().await else {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            if n.fetch_add(1, Ordering::SeqCst) >= MAX_CLIENTS {
                n.fetch_sub(1, Ordering::SeqCst);
                continue; // dropping `s` closes it
            }
            tokio::spawn(client(s, tx.clone(), state.clone(), Guard(n.clone())));
        }
    });
}

struct Guard(Arc<AtomicUsize>);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Reads requests here; a writer task sends state changes and replies.
async fn client(
    s: UnixStream,
    tx: UnboundedSender<Msg>,
    mut state: watch::Receiver<State>,
    _guard: Guard,
) {
    let (r, mut w) = s.into_split();
    let (out, mut replies) = unbounded_channel::<Result<State, String>>();
    tokio::spawn(async move {
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
                    state.borrow_and_update(); // a reply already carries the newest state
                    let line = json!({ "state": s });
                    last = Some(s);
                    line
                }
                Err(e) => json!({ "error": e }),
            };
            if w.write_all(format!("{line}\n").as_bytes()).await.is_err() {
                break;
            }
        }
    });
    let mut r = BufReader::new(r);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        // A line over the cap leaves no newline within the limit: close.
        match (&mut r)
            .take(MAX_LINE + 1)
            .read_until(b'\n', &mut buf)
            .await
        {
            Ok(n) if n > 0 && buf.ends_with(b"\n") => {}
            _ => return,
        }
        let reply = match parse(&buf) {
            Ok(req) => {
                let (rt, rr) = oneshot::channel();
                if tx.send(Msg::Remote(req, rt)).is_err() {
                    return;
                }
                match rr.await {
                    Ok(s) => Ok(s),
                    Err(_) => return,
                }
            }
            Err(e) => Err(e),
        };
        if out.send(reply).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use tokio::io::AsyncBufReadExt;

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
        for _ in 0..5 {
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
        a.serve_remote(sock.clone());
        assert_eq!(a.sock.as_deref(), Some(sock.as_path()));
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
        a.serve_remote(sock.clone());
        let (mut r, mut w) = connect(&sock).await;
        line(&mut r).await;
        w.write_all(&[b'a'; 5000]).await.unwrap();
        assert_eq!(line(&mut r).await, ""); // EOF
        // exactly the cap is fine
        let (mut r, mut w) = connect(&sock).await;
        line(&mut r).await;
        let mut ok = vec![b' '; MAX_LINE as usize - 16];
        ok.extend_from_slice(br#"{"get":"state"}"#);
        ok.push(b'\n');
        assert_eq!(ok.len(), MAX_LINE as usize);
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
        a.serve_remote(sock.clone());
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
    async fn a_live_socket_is_left_alone_and_a_dead_file_is_replaced() {
        let dir = tmp("own");
        let sock = dir.join("t.sock");
        let live = bind(&sock).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&sock), 0o600);
        let mut a = App::blank();
        a.serve_remote(sock.clone());
        assert!(a.status.contains("another termusic"), "{}", a.status);
        assert!(a.sock.is_none() && sock.exists());
        a.shutdown();
        assert!(sock.exists(), "not ours: must not be removed");
        drop(live);
        // the file stays behind after the listener is gone
        assert!(sock.exists());
        let mut b = App::blank();
        b.serve_remote(sock.clone());
        assert!(b.sock.is_some() && !b.status.contains("remote control"));
        assert_eq!(mode(&sock), 0o600);
        UnixStream::connect(&sock).await.unwrap();
        b.shutdown();
        assert!(!sock.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_too_long_path_is_refused() {
        let long = PathBuf::from(format!("/tmp/{}", "x".repeat(MAX_PATH)));
        assert!(bind(&long).unwrap_err().contains("too long"));
        let mut a = App::blank();
        a.serve_remote(long);
        assert!(a.status.contains("socket path too long") && a.sock.is_none());
    }
}
