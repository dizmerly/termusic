# termusic

A text-only terminal YouTube Music player written in Rust with [Ratatui](https://ratatui.rs). Sign in with Google, search YouTube, browse your playlists and play audio, all from the keyboard.

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

Type a query, press **Enter**, move with `j`/`k`, and press **Enter** on a track to play it. Playlists need Google sign-in: press `P` (`F2` on Search) and, if you are signed out, the status bar says so; press `P` again to reach the Login screen, then **Enter**: your browser opens, you approve, and the app continues on its own. You can also run `termusic login`.

## Behavior

### Startup
1. Checks connectivity. If offline, shows the **Offline** screen (`r` retries, `q` quits).
2. Otherwise goes straight to **Search**; signing in is optional.
3. If `mpv` or `yt-dlp` can't be run, the status bar says which is missing.

### Screens
| Screen | What it does |
|--------|--------------|
| Login | Optional Google sign-in (press `P` twice when signed out). Opens your browser; shows the URL while waiting. `Esc` goes back. |
| Search | Text prompt. `Enter` runs the query (via `yt-dlp`, no account needed) and shows the top 30 results. `Esc` or `Tab` goes back to the results when there are some, otherwise `Esc` quits. |
| Results | Scrollable list of tracks (title and channel). Text only, no thumbnails. |
| Playlists | Your YouTube playlists (needs sign-in). `Enter` loads the playlist (first 50 tracks) as the queue, in order, and plays the first track; the list then shows it. |
| History | The last 10 played tracks (`H`, `F4` on Search). `Enter` replays from there, resuming where you stopped, and the queue follows history order. |
| Settings | `S` (`F3` on Search): volume (`←`/`→` or `-`/`+`), color shell (`←`/`→` on its row: TPS-L2, Sunset tape, Amber hi-fi), history file path (`Enter` shows how to change it), clear history (`Enter` twice). |
| Help | Lists every enabled command and keybinding, grouped; scrolls with `j`/`k`, `PgUp`/`PgDn`, `g`/`G`. |
| Offline | Shown whenever a network action finds no connection. |

### Keys
| Key | Action |
|-----|--------|
| `Enter` | Play the highlighted track and replace the list with its YouTube mix (related tracks; `n` follows it). Playlists: play the playlist from its first track, in its own order |
| `a` | Add the highlighted result to the queue (plays after the current track, before the mix) |
| `p` | Pause / resume |
| `n` | Next track |
| `b` | Back to the previous track (the current one plays next; stops at the first track) |
| `s` | Toggle shuffle |
| `l` | Cycle loop: off / one (↻1) / all (↻) |
| `+` / `-` | Volume up / down |
| `,` / `.` | Seek back / forward 5s |
| `/` | New search |
| `P` (`F2` on the Search screen) | Your playlists; signed out, a status message asks you to log in |
| `j` / `↓`, `k` / `↑` | Move selection |
| `PgDn` / `PgUp` | Move one page |
| `g` / `G` | Jump to first / last |
| `S` (`F3` on Search) | Settings |
| `H` (`F4` on Search) | History: last 10 played tracks; the file is `history.json` in the config dir, or set `TERMUSIC_HISTORY` / `history_path` in `settings.json` |
| `Tab` | On Search: back to the results list |
| `Esc` | Back: Search to results; Help, Settings and History to the screen they were opened from |
| `?` (`F1` on the Search screen) | Help |
| `q` | Quit (`Esc` on Search; `Ctrl+C` anywhere) |

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

### CLI
```sh
termusic              # launch the TUI
termusic login        # sign in (opens your browser)
termusic logout       # delete stored tokens
termusic search <q>   # print the top results as text (no account needed)
```

## Development

```sh
cargo check
cargo clippy -- -D warnings
cargo test
cargo fmt
```

Layout: `app.rs` (all state), `ui.rs` / `theme.rs` (rendering, colors), `auth.rs`, `youtube.rs`, `player.rs`, `queue.rs`, `net.rs`, `cmd.rs` (playback `Cmd`s that keys map to and player/connection `Event`s that `App::on_event` applies; media keys or a menu-bar helper can reuse them), `commands/` (one file per command with an `ENABLED` flag; the help screen lists only enabled ones). Sprint notes live in `SPRINT.md`.

## Limitations
- Unix only (macOS/Linux): the player uses a Unix socket.
- Shuffle uses a small clock-seeded xorshift, not a cryptographic RNG.
- Google sign-in hasn't been exercised live yet; playlists show only the first 50 tracks.
