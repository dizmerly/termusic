# Milestone 4 design spec: control socket and macOS menu bar helper

Designer and Advisor: Opus (main session). Implementer: Sonnet. Builds on milestones 1 and 2 (merged into
`main`). Overall plan: /Users/dizmerly/.claude/plans/cached-toasting-peach.md. Where this spec
and the plan differ, this spec wins. Where it is silent, do the simplest thing and report it. 
Alternate Designer: Fable (only use for particularly challenging artistic decisions). 
Also make sure to document your work, in this same overhaul specs folder, with the respective completed overhaul specs,
the main goal is to audit the technologies, test coverage, and design of the project to the author, so that 
the author can review it. 


Goal: a small menu bar item on macOS that shows what termusic is playing and lets the user
control playback and the app's settings without switching to the terminal. The helper is a
remote: it never plays audio, never starts or quits termusic, and is useless (greyed out)
when termusic is not running.

## 1. Socket (`src/remote.rs`)

- Path: `<config dir>/termusic.sock`, where the config dir is `settings::config_dir()`
  (`$TERMUSIC_CONFIG_DIR`, else the user config dir + `termusic`). Created with mode 0600.
  If the path would exceed the platform's Unix socket path limit, skip remote control and say
  so once in the status line.
- Startup: if the file exists, try to connect. If something answers, another termusic owns it:
  leave it alone, run without remote control, status line `remote control: another termusic
  is running`. If nothing answers, remove the dead file and bind. On exit (normal quit and
  `App::shutdown`) remove the file. A crash can leave a dead file; the next start handles it.
- tokio `UnixListener` in a background task. At most 4 clients at once; extra connections are
  closed. Each line is at most 4096 bytes; longer lines close that client. Nothing a client
  sends may crash or block the app.
- Disabled entirely under `cfg(test)` for `App::blank()` (tests bind their own listener in a
  temp dir to exercise the protocol).

## 2. Protocol: one JSON object per line, UTF-8, `\n` terminated

Client to app:

| Line | Effect |
|---|---|
| `{"cmd":"play_pause"}` | `Cmd::PlayPause` |
| `{"cmd":"next"}` / `{"cmd":"prev"}` | `Cmd::Next` / `Cmd::Prev` |
| `{"cmd":"seek","secs":-5}` | `Cmd::Seek(secs)`, secs clamped to -600..=600 |
| `{"cmd":"toggle_shuffle"}` / `{"cmd":"cycle_loop"}` | the matching `Cmd` |
| `{"set":"volume","value":40}` | absolute volume 0-100 |
| `{"set":"loop","value":"off"}` | `"off"`, `"one"`, `"all"` |
| `{"set":"shuffle","value":true}` | |
| `{"set":"shell","value":"amber"}` | a known shell id |
| `{"set":"thumbnails","value":true}` | |
| `{"set":"animations","value":true}` | |
| `{"set":"hiss","value":true}` | |
| `{"set":"hiss_level","value":30}` | 0-100 |
| `{"get":"state"}` | replies with the state line |

App to client:

- State line, sent on connect, after every accepted request, and whenever the state changes
  (at most about once a second for position):
  `{"state":{"title":"…","channel":"…","id":"…","loaded":true,"paused":false,"position":72.0,
  "duration":213.0,"volume":70,"loop":"one","shuffle":false,"shell":"tps-l2",
  "shells":[{"id":"tps-l2","name":"TPS-L2"},…],"thumbnails":false,"animations":true,
  "hiss":false,"hiss_level":30}}`
  (`title`, `channel`, `id`, `duration` are `null` when unknown or nothing is loaded.)
- Error line for a rejected request: `{"error":"unknown set key: foo"}`. Unknown commands,
  wrong types and out-of-range values are rejected and change nothing. Malformed JSON gets an
  error line and the connection stays open.

Rules:
- Every request is turned into a `Msg` on the existing channel and handled on the UI thread by
  the same `App` methods the keyboard and the Settings screen use (`App::dispatch` for `Cmd`s;
  one setter per setting). No second code path for changing state. Settings changed remotely
  are saved through the existing background save.
- `Cmd` gains serde derives only if that is the simplest way to parse; the wire names above
  are the contract either way. Parsing a line into a request is a pure function with tests.
- State is published through a `tokio::sync::watch` channel: `App::tick` builds the snapshot
  and sends it only when it differs from the last one; client tasks forward changes. Building
  the snapshot must be cheap (no IO).
- A remote command triggers the same key-press animation as the keyboard (milestone 2 hooks
  this in `App::dispatch`).

## 3. Helper (`helper/`, Swift)

- A SwiftPM package (`helper/Package.swift`, executable target `TermusicBar`, macOS 13+), no
  Xcode project, no third-party packages. Built with `swift build -c release` inside `helper/`.
  The binary is `helper/.build/release/TermusicBar`; add `helper/.build/` to `.gitignore`.
- Runs as a menu bar accessory (no Dock icon, no main window).
- Menu bar item: `♪` plus the track title cut to about 30 characters; `♪` alone when nothing
  is loaded; `♪` dimmed when termusic is not running.
- Menu, top to bottom:
  1. Title and channel (two disabled lines), or `termusic is not running` / `Nothing playing`.
  2. Play/Pause (label follows state), Previous, Next.
  3. Volume: a slider (0-100) embedded in a menu item; sends `set volume` while dragging,
     at most 10 times a second.
  4. Shuffle (checkmark), Loop submenu: Off, This track, Whole queue (checkmark on current).
  5. Settings submenu: Shell (one item per entry of `shells`, checkmark on current), Cover
     art, Animations, Tape hiss (checkmarks), Hiss level slider.
  6. Quit TermusicBar (quits only the helper).
- All controls except Quit are disabled when not connected.
- Connection: resolve the socket path the same way as the app (`TERMUSIC_CONFIG_DIR`, else
  `~/Library/Application Support/termusic`), connect, read state lines, reconnect every 2 s
  when the socket is absent or drops. All socket work off the main thread; UI updates on the
  main thread. The helper renders only what the last state line says; it keeps no state of
  its own and never guesses.
- Keep the protocol encoding/decoding in a small file with no AppKit imports so it can be
  tested with `swift test` if the installed toolchain has XCTest or Swift Testing; if neither
  is available with the command line tools, say so and skip the Swift tests.

## 4. Docs

- README: what the helper is, how to build and run it, how to start it at login (one line:
  add the binary to Login Items), the protocol table (it is a public contract; other tools
  can use it).
- `CLAUDE.md` Architecture: add `remote.rs` and `helper/`.

## 5. Verification

- Rust: unit tests for the request parser (every row of the table, plus bad input: unknown
  key, wrong type, out of range, non-JSON, oversize line, non-UTF-8); an integration-style
  test that binds a listener in a temp dir, connects a `UnixStream`, checks the state line on
  connect, sends `set volume`, and checks the next state line and `App` state; the
  second-instance rule (a live socket is not removed; a dead file is).
- Live: run the real app with a scratch config dir, and from a script connect to the socket:
  check the state line follows playback (title, paused, position advancing), that
  `play_pause`, `next`, `set volume`, `set shell`, `set hiss` take effect in the app (confirm
  by reading mpv over IPC and the app's screen), that garbage input does not disturb the app,
  and that the socket file is gone after quit.
- Helper: `swift build -c release` succeeds with no warnings worth fixing; start the helper
  with the app running and confirm from the app's side that a client connected and received
  state (for example a debug counter or by observing a command sent through a test hook), then
  quit termusic and confirm the helper stays alive and reconnects when termusic starts again.
  How the menu looks and feels can only be judged by the user; say exactly what was and was
  not checked.
