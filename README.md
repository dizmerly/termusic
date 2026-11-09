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

Type a query, press **Enter**, move with `j`/`k`, and press **Enter** on a track to play it. To use playlists, press `P` and choose **Enter** to sign in: your browser opens, you approve, and the app continues on its own.

## Behavior

### Startup
1. Checks connectivity. If offline, shows the **Offline** screen (`r` retries, `q` quits).
2. Otherwise goes straight to **Search**; signing in is optional.
3. If `mpv` or `yt-dlp` can't be run, the status bar says which is missing.

### Screens
| Screen | What it does |
|--------|--------------|
| Login | Optional Google sign-in (opened with `P` when signed out). Opens your browser; shows the URL while waiting. `Esc` goes back. |
| Search | Text prompt. `Enter` runs the query (via `yt-dlp`, no account needed) and shows the top 30 results. `Esc` quits. |
| Results | Scrollable list of tracks (title and channel). Text only, no thumbnails. |
| Playlists | Your YouTube playlists (needs sign-in). `Enter` opens one (first 50 tracks). |
| Help | Lists every enabled command and keybinding. |
| Offline | Shown whenever a network action finds no connection. |

### Keys
| Key | Action |
|-----|--------|
| `Enter` | Play the highlighted track and replace the list with its YouTube mix (related tracks; `n` follows it). Playlists: open the playlist, which keeps its own order |
| `a` | Add the highlighted result to the queue (plays after the current track, before the mix) |
| `p` | Pause / resume |
| `n` | Next track |
| `s` | Toggle shuffle |
| `l` | Cycle loop: off / all (↻) / one (↻1) |
| `+` / `-` | Volume up / down |
| `/` | New search |
| `P` | Your playlists |
| `j` / `↓`, `k` / `↑` | Move selection |
| `PgDn` / `PgUp` | Move one page |
| `g` / `G` | Jump to first / last |
| `?` (`F1` on the Search screen) | Help |
| `q` | Quit (`Esc` on Search; `Ctrl+C` anywhere) |

### Playback
- Plays one track at a time through `mpv --no-video`, controlled over mpv's IPC socket.
- A line above the status bar shows `next:` and the next three tracks. Tracks you add with `a` keep their place when the mix replaces the rest; with nothing playing they only wait (`n` starts the first).
- Playing a track queues the tracks after it in the list; when a track ends the next one starts. At the end the queue stops (loop off), starts over (loop all), or the track repeats (loop one; `n` still skips).
- Shuffle reorders the upcoming tracks, never the one playing. Turning it off keeps the shuffled order.
- If mpv exits within ~3 seconds (usually a missing `yt-dlp`), termusic shows an error instead of skipping through the list.
- The bottom bar shows what's playing (▶ / ⏸), shuffle and loop state, a working indicator during network calls, and status or error messages.

### Networking and auth
- All network calls run in background tasks, so the UI never freezes. Connectivity is checked before each one.
- Sign-in uses Google's loopback-redirect flow (Desktop-app client) with PKCE (S256); the client secret is sent only when configured. Tokens are stored in your config dir (`termusic/tokens.json`, mode `0600`) and refreshed automatically. If Google revokes the session you're logged out.
- Read-only access: the app requests only the `youtube.readonly` scope.

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

Layout: `app.rs` (all state), `ui.rs` / `theme.rs` (rendering, colors), `auth.rs`, `youtube.rs`, `player.rs`, `queue.rs`, `net.rs`, `commands/` (one file per command with an `ENABLED` flag; the help screen lists only enabled ones). Sprint notes live in `SPRINT.md`.

## Limitations
- Unix only (macOS/Linux): the player uses a Unix socket.
- Shuffle uses a small clock-seeded xorshift, not a cryptographic RNG.
- Google sign-in hasn't been exercised live yet; playlists show only the first 50 tracks.
