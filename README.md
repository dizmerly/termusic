# termusic

A terminal YouTube Music player written in Rust with [Ratatui](https://ratatui.rs). Sign in with Google, search YouTube, browse your playlists and play audio, all from the keyboard.

## Quickstart

**1. Install runtime dependencies** (playback shells out to these; they must be on `PATH`):

```sh
brew install mpv yt-dlp        # macOS; use your package manager elsewhere
```

**2. (Optional) Set up Google sign-in** — only needed to see your own playlists. Search and playback work without any account.

In the [Google Cloud console](https://console.cloud.google.com/apis/credentials):
1. Enable the **YouTube Data API v3** for your project.
2. Create credentials → **OAuth client ID** → application type **Desktop app**, and add yourself as a test user on the consent screen.
3. Either download the JSON and save it as `client_secret.json` in termusic's config dir (`~/Library/Application Support/termusic/` on macOS, `~/.config/termusic/` on Linux), or export `TERMUSIC_CLIENT_ID` and `TERMUSIC_CLIENT_SECRET`. The secret is optional (sign-in uses PKCE), but Google's Desktop-app clients currently still require it, so include it if your client has one.

**3. Run**

```sh
cargo run
```

Type a query, press **Enter**, move with `j`/`k`, and press **Enter** on a track to play it. Playlists need Google sign-in: press `P` or `3` (`F2` while typing a search) and, if you are signed out, the Lists tab shows a sign-in prompt; press **Enter**: your browser opens, you approve, and the app continues on its own. You can also run `termusic login`.

## Behavior

### Startup
1. Checks connectivity. If offline, shows the **Offline** screen (`r` retries, `q` quits).
2. Otherwise starts on the **Results** tab with the search field focused; signing in is optional.
3. If `mpv` or `yt-dlp` can't be run, the status line says which is missing.

### Screens
The window is a cassette deck: on the left the tape (title on the label, reels that wind with the track), a tape counter with progress, and transport keys that light up for play, pause, shuffle and loop; on the right the list pane, like the tape's J-card. Volume and the shuffle/loop modes sit in the top border, key hints in the bottom one. Below 76x22 the deck moves above the list (from 38x30), and smaller still the list fills the window with a one-line status bar. Colors come from the shell picked in Settings.

The list pane has four tabs (`Tab`/`Shift-Tab` or `1`-`4`); each keeps its own selection. Settings and Help open over the list.

| Screen | What it does |
|--------|--------------|
| Search field | At the top of every tab (`/`). `Enter` runs the query (via `yt-dlp`, no account needed) and shows the top 30 results on the Results tab. `Esc` or `Tab` leaves the field and keeps the text; it never quits. |
| Results (`1`) | Scrollable list of tracks (title and channel). Text only; thumbnails never appear in lists. Follows the playing track. |
| Queue (`2`) | The playing track, then what plays next. `Enter` jumps to the highlighted track; the ones before it count as played (`b` goes back through them). |
| Lists (`3`, `P`) | Your YouTube playlists. `Enter` loads the playlist (first 50 tracks) as the queue, in order, and plays the first track; the Results tab then shows it. Signed out, the tab shows a sign-in prompt instead: `Enter` opens your browser and shows the URL while waiting. |
| History (`4`, `H`) | The last 10 played tracks. `Enter` replays from there, resuming where you stopped, and the queue follows history order. |
| Settings | `S` (`F3` while typing): volume (`←`/`→` or `-`/`+`), color shell (`←`/`→` on its row: TPS-L2, Sunset tape, Amber hi-fi), history file path (`Enter` shows how to change it), clear history (`Enter` twice). |
| Help | Lists every enabled command and keybinding, grouped; scrolls with `j`/`k`, `PgUp`/`PgDn`, `g`/`G`. |
| Offline | Shown whenever a network action finds no connection. |

### Keys
| Key | Action |
|-----|--------|
| `Enter` | Play the highlighted track and replace the list with its YouTube mix (related tracks; `n` follows it). Playlists: play the playlist from its first track, in its own order. Queue: jump to the highlighted track |
| `a` | Add the highlighted result to the queue (plays after the current track, before the mix) |
| `p` | Pause / resume |
| `n` | Next track |
| `b` | Back to the previous track (the current one plays next; stops at the first track) |
| `s` | Toggle shuffle |
| `l` | Cycle loop: off / one (↻1) / all (↻) |
| `+` / `-` | Volume up / down |
| `,` / `.` | Seek back / forward 5s |
| `/` | Focus the search field (from any tab; closes Settings and Help) |
| `P` (`F2` while typing) | Lists tab; signed out, it shows the sign-in prompt |
| `j` / `↓`, `k` / `↑` | Move selection |
| `PgDn` / `PgUp` | Move one page |
| `g` / `G` | Jump to first / last |
| `S` (`F3` while typing) | Settings |
| `H` (`F4` while typing) | History tab: last 10 played tracks; the file is `history.json` in the config dir, or set `TERMUSIC_HISTORY` / `history_path` in `settings.json` |
| `Tab` / `Shift-Tab` | Next / previous tab (wraps). In the search field `Tab` leaves it |
| `1` `2` `3` `4` | Results, Queue, Lists, History tab |
| `Esc` | Leave the search field (keeps the text); close Help or Settings; in a list, focus the search field |
| `?` (`F1` while typing) | Help |
| `q` | Quit (a letter while typing; `Ctrl+C` anywhere) |

### Playback
- Plays one track at a time through `mpv --no-video`, controlled over mpv's IPC socket.
- A line above the status bar shows `next:` and the next three tracks. Tracks you add with `a` keep their place when the mix replaces the rest; with nothing playing they only wait (`n` starts the first).
- Playing a track queues the tracks after it in the list; when a track ends the next one starts. At the end the queue stops (loop off), starts over (loop all), or the track repeats (loop one; `n` still skips).
- `,` / `.` seek 5 seconds back / forward in the current track (nothing happens when idle).
- With loop all and shuffle, each lap is reshuffled and never starts with the track that just ended. Pressing `Enter` on a track you queued plays it once, not again from the queue.
- Shuffle reorders the upcoming tracks, never the one playing. Turning it off keeps the shuffled order.
- If mpv exits within ~3 seconds (usually a missing `yt-dlp`), termusic shows an error instead of skipping through the list.
- The bottom bar shows what's playing (▶ / ⏸), shuffle and loop state, a working indicator during network calls, and status or error messages.

### Networking and auth
- All network calls run in background tasks, so the UI never freezes. Connectivity is checked before each one.
- Sign-in uses Google's loopback-redirect flow (Desktop-app client) with PKCE (S256); the client secret is sent only when configured. Tokens are stored in your config dir (`termusic/tokens.json`, mode `0600`) and refreshed automatically. If Google revokes the session you're logged out.
- Read-only access: the app requests only the `youtube.readonly` scope.

### Settings

The volume is saved to `settings.json` in the config dir (same dir as `tokens.json`) and restored at startup (default 70). Set `TERMUSIC_CONFIG_DIR` to use another directory for settings and tokens.

### Cover art

The **Cover art** setting (Settings screen, off by default) shows the playing track's thumbnail as pixel art on the cassette label, drawn in the active shell's colors. The image-protocol query only runs in terminals known to support images (kitty, Ghostty, WezTerm, iTerm2, detected from `TERM_PROGRAM`, `TERM` or `KITTY_WINDOW_ID`), because the query library leaves a thread reading the keyboard if a terminal never answers. Those terminals get a crisp, hard-edged image (WezTerm through the iTerm2 protocol: the image crate blacklists its kitty and sixel paths). All other terminals get half-block pixels, and switching the setting on says so once per session. Set `TERMUSIC_ART=detect` to force the query anyway (for example in a sixel terminal the hints miss). Set `TERMUSIC_ART=halfblocks` to force half-blocks (for a terminal that misreports) or `TERMUSIC_ART=off` to never draw or fetch art; either skips the startup terminal query.

### CLI
```sh
termusic              # launch the TUI
termusic login        # sign in (opens your browser)
termusic logout       # delete stored tokens
termusic search <q>   # print the top results as text (no account needed)
```

## Menu bar helper (macOS)

A small menu bar item that shows what termusic is playing and controls it: play/pause, previous, next, volume, shuffle, loop, and the cover art, animations, tape hiss and shell settings. It is a remote, not a player. It never plays audio and never starts or quits termusic. When termusic is not running, its menu bar item is dimmed and its controls are greyed out. Quit TermusicBar quits only the helper.

Build it (macOS 13 or later, with the Swift toolchain from the Xcode command line tools):

```sh
cd helper && swift build -c release
```

The binary is `helper/.build/release/TermusicBar`. Run it from the repository root:

```sh
helper/.build/release/TermusicBar
```

To start it at login, add that binary in System Settings > General > Login Items.

### Control socket

termusic listens on `termusic.sock` in its config dir: `$TERMUSIC_CONFIG_DIR` if set, else `~/Library/Application Support/termusic`. The socket is mode 0600, so only your user can connect. A lock file, `termusic.lock`, in the same directory records which termusic owns the socket. A second termusic runs without remote control, and its status line says so. A socket left behind by a crash is replaced on the next start.

The protocol is one JSON object per line, UTF-8, ended by `\n`. Other programs can use it too. Send integers: `40`, not `40.5`.

| Line | Effect |
|---|---|
| `{"cmd":"play_pause"}` | play or pause |
| `{"cmd":"next"}` / `{"cmd":"prev"}` | next or previous track |
| `{"cmd":"seek","secs":-5}` | seek by `secs`, clamped to -600..=600 |
| `{"cmd":"toggle_shuffle"}` / `{"cmd":"cycle_loop"}` | toggle shuffle; cycle loop (off, this track, whole queue, off) |
| `{"set":"volume","value":40}` | absolute volume, 0-100 |
| `{"set":"loop","value":"off"}` | `"off"`, `"one"` (this track) or `"all"` (whole queue) |
| `{"set":"shuffle","value":true}` | shuffle on or off |
| `{"set":"shell","value":"amber"}` | a shell id from the `shells` list in the state line |
| `{"set":"thumbnails","value":true}` | cover art on or off |
| `{"set":"animations","value":true}` | animations on or off |
| `{"set":"hiss","value":true}` | tape hiss on or off |
| `{"set":"hiss_level","value":30}` | hiss level, 0-100 |
| `{"get":"state"}` | replies with a state line |

termusic sends a state line on connect, after every accepted request, and whenever its state changes (while playing, the position changes about once a second):

```json
{"state":{"title":"…","channel":"…","id":"…","loaded":true,"paused":false,"position":72.0,"duration":213.0,"volume":70,"loop":"one","shuffle":false,"shell":"tps-l2","shells":[{"id":"tps-l2","name":"TPS-L2"},{"id":"sunset","name":"SUNSET TAPE"},{"id":"amber","name":"AMBER HI-FI"}],"thumbnails":false,"animations":true,"hiss":false,"hiss_level":30}}
```

`title`, `channel`, `id` and `duration` are `null` when unknown or when nothing is loaded. `position` is whole seconds.

A rejected request gets an error line and changes nothing:

- `{"error":"offline"}`: a playback command while termusic is offline. Settings still work offline.
- `{"error":"disabled"}`: a playback command whose command is disabled in `commands/`.
- Any other message, such as `{"error":"unknown set key: foo"}` or `{"error":"value must be 0-100"}`.

A line that is not JSON gets an error line, and the connection stays open. A line longer than 4096 bytes closes the connection. At most 4 clients can be connected at once; a fifth is closed.

## Development

```sh
cargo check
cargo clippy -- -D warnings
cargo test
cargo fmt
```

Layout: `app.rs` (all state), `ui.rs` / `theme.rs` (rendering, colors), `auth.rs`, `youtube.rs`, `player.rs`, `queue.rs`, `net.rs`, `cmd.rs` (playback `Cmd`s that keys map to and player/connection `Event`s that `App::on_event` applies; the control socket in `remote.rs` and the [menu bar helper](#menu-bar-helper-macos) drive the same ones), `commands/` (one file per command with an `ENABLED` flag; the help screen lists only enabled ones). Sprint notes live in `SPRINT.md`.

## Limitations
- Unix only (macOS/Linux): the player uses a Unix socket.
- Shuffle uses a small clock-seeded xorshift, not a cryptographic RNG.
- Google sign-in hasn't been exercised live yet; playlists show only the first 50 tracks.
