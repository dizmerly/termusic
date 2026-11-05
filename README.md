# termusic

A text-only terminal YouTube Music player written in Rust with [Ratatui](https://ratatui.rs). Sign in with Google, search YouTube, browse your playlists and play audio, all from the keyboard.

## Quickstart

**1. Install runtime dependencies** (playback shells out to these; they must be on `PATH`):

```sh
brew install mpv yt-dlp        # macOS; use your package manager elsewhere
```

**2. Create a Google OAuth client**

In the [Google Cloud console](https://console.cloud.google.com/apis/credentials):
1. Enable the **YouTube Data API v3** for your project.
2. Create credentials → **OAuth client ID** → application type **TVs and Limited Input devices**.
3. Export the values:

```sh
export TERMUSIC_CLIENT_ID=...
export TERMUSIC_CLIENT_SECRET=...
```

**3. Run**

```sh
cargo run
```

On first launch choose **Enter** on the login screen. termusic shows a URL and a code: open the URL on any device, enter the code, and the app continues on its own. Then type a query, press **Enter**, move with `j`/`k`, and press **Enter** on a track to play it.

## Behavior

### Startup
1. Checks connectivity. If offline, shows the **Offline** screen (`r` retries, `q` quits).
2. If no saved login exists, shows the **Login** screen; otherwise goes to **Search**.

### Screens
| Screen | What it does |
|--------|--------------|
| Login | Google device-code sign-in. Shows the verification URL and code while waiting. |
| Search | Text prompt. `Enter` runs the query and shows the top 30 results. `Esc` quits. |
| Results | Scrollable list of tracks (title and channel). Text only, no thumbnails. |
| Playlists | Your YouTube playlists. `Enter` opens one as a track list. |
| Help | Lists every enabled command and keybinding. |
| Offline | Shown whenever a network action finds no connection. |

### Keys
| Key | Action |
|-----|--------|
| `Enter` | Play the highlighted track (Playlists: open the playlist) |
| `p` | Pause / resume |
| `n` | Next track |
| `s` | Toggle shuffle |
| `/` | New search |
| `P` | Your playlists |
| `j` / `↓`, `k` / `↑` | Move selection |
| `PgDn` / `PgUp` | Move one page |
| `g` / `G` | Jump to first / last |
| `?` (`F1` on the Search screen) | Help |
| `q` | Quit (`Esc` on Search; `Ctrl+C` anywhere) |

### Playback
- Plays one track at a time through `mpv --no-video`, controlled over mpv's IPC socket.
- When a track ends, the next one in the list starts automatically; at the end of the list playback stops.
- With shuffle on, "next" picks a random track from the current list.
- If mpv exits within ~3 seconds (usually a missing `yt-dlp`), termusic shows an error instead of skipping through the list.
- The bottom bar shows what's playing (▶ / ⏸), shuffle state, a working indicator during network calls, and status or error messages.

### Networking and auth
- All network calls run in background tasks, so the UI never freezes. Connectivity is checked before each one.
- Tokens are stored in your config dir (`termusic/tokens.json`, mode `0600`) and refreshed automatically. If Google revokes the session, you're logged out and returned to the Login screen.
- Read-only access: the app requests only the `youtube.readonly` scope.

### CLI
```sh
termusic              # launch the TUI
termusic login        # sign in from the terminal (prints URL and code)
termusic logout       # delete stored tokens
termusic search <q>   # print the top results as text
```

## Development

```sh
cargo check
cargo clippy -- -D warnings
cargo test
cargo fmt
```

Layout: `app.rs` (all state), `ui.rs` / `theme.rs` (rendering, colors), `auth.rs`, `youtube.rs`, `player.rs`, `net.rs`, `commands/` (one file per command with an `ENABLED` flag; the help screen lists only enabled ones). Sprint notes live in `SPRINT.md`.

## Limitations
- Unix only (macOS/Linux): the player uses a Unix socket.
- No volume control yet.
- Shuffle uses a clock-based pick, not a uniform random choice.
- Not yet verified end-to-end against live Google and YouTube.
