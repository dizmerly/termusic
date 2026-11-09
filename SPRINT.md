# Sprint Log

## Sprint 1 — Functional MVP
**Goal:** `cargo run` → logo → (offline check) → Google login → search → scrollable top-K (30) results → play with mpv. Help menu documents every command.

### Plan (3 iterations)
1. **Iteration 1 — Skeleton:** Cargo project, module layout, TUI shell, theme/logo, command files with flags, help screen, offline screen. Must compile.
2. **Iteration 2 — Backends:** net check, OAuth device flow + token store, YouTube search/playlists, player (mpv IPC). Wire into App.
3. **Iteration 3 — Polish & verify:** scrolling, edge cases (offline, expired token, missing mpv/yt-dlp, empty results), unit tests, clippy clean.

### Delegation
Opus plans/reviews only. Implementation by Sonnet agents (Haiku for trivial tasks like docs/tests).

### Log
- Iteration 1 (done): compiling skeleton. Cargo deps added (reqwest 0.13 uses feature `rustls`), modules app/ui/theme/config/net/player/auth/youtube, `commands/` (play, pause, skip, shuffle, search, help, quit; each with `ENABLED` first, plus NAME/KEY/DESCRIPTION; `commands::enabled()` feeds the help screen). Offline screen, Login stub, Search input, Results with j/k/arrows/PgUp/PgDn/g/G scrolling over fake data, Playlists placeholder (`P`). CLI: `login`/`logout`/`search` stubs. `cargo clippy -- -D warnings` clean. Next: real net/OAuth/YouTube/mpv wiring (command keys are not yet dispatched through `commands/`; handlers live in `App`).
- Iteration 2 (done): real backends. `auth.rs` device flow + 0600 token file + auto-refresh (invalid_grant → logout → Login screen). `youtube.rs` search/playlists/playlist items via one `get` + `parse`. `player.rs` spawns `mpv --no-video`, pause over IPC socket, detects exit (fast exit = failure, no skip-cascade). `App` runs all network work in tokio tasks (`spawn` does the offline check first) and receives `Msg`s in `tick()`; nothing blocks the UI. `login`/`search` CLI are real. Skip (`n`), shuffle (`s`), auto-advance, playlists (`P`, Enter opens).
- Iteration 3 (done): colorful theme (RGB palette, gradient logo, rounded colored panels, styled status bar with busy/now-playing/shuffle), empty-list state, tests for token parsing + YouTube JSON shapes. clippy `-D warnings` clean, 3 tests pass.
- Not verified: live run against real Google/mpv (needs `TERMUSIC_CLIENT_ID/SECRET`, mpv, yt-dlp) and visual check in a terminal. Unix-only (mpv socket). Not done: volume keys; commands/ `ENABLED` flags still only drive the help screen, key dispatch lives in `App`.

## Sprint 2 — Works for everyone
- Iter 1 (done): search moved to `yt-dlp` (guest-capable); OAuth switched to loopback flow because Google "Desktop app" clients reject device flow; login optional (`P` when signed out). Credentials via env or `client_secret.json` in the config dir (git-ignored). Verified live via a pty: search → play → pause → skip → quit leaves no mpv. Judge: 7/10.
- Iter 2 (done): `ENABLED` flags now gate key dispatch; token file created 0600; startup warning when mpv/yt-dlp missing; Online-retry no longer forces Login; state-machine tests; README rewritten.
- Iter 3 (done): volume commands (+/-), single-width shuffle icon, clearer playback-failure message. Judge: 7.5/10.
- Iter 4 (done): help screen gets all rows (logo hidden; two lines still clipped until iter 5), Esc on Search returns to results, yt-dlp search timeout (30s, killed on drop), random OAuth state, `play` description fixed, key-handling tests.
- Iter 4 judge: 7.5/10 (help still clipped, long titles hid the channel).
- Iter 5 (done): shortened help text, titles truncated so channel stays visible, ♪ marks the playing row, logo hidden under 20 rows, TestBackend render tests (help 80x24, tiny terminal no panic).
- Iter 5 judge: 8.5/10. Open: playing marker compares titles not ids; Help clips below 24 rows; no PKCE; auto-advance/shuffle untested; nav keys in help are hand-written.
- Iter 6 (done): Enter on a search result loads YouTube's mix (`list=RD<id>` via yt-dlp) as the new list, so `n`/auto-advance follow related tracks; playlists keep their order. Stale related results are dropped. Not yet checked live.
- Iter 7 (done): SIGKILL no longer orphans mpv: per-track `sh` watchdog (polls app+mpv pid every 1s, kills mpv, removes socket). player.rs refactored (`Running`, `mpv_command`, `watchdog_command`, `attach`) and covered by 5 tests incl. a real kill -9; verified live via pty.
- Iter 8 (done): `l` toggles looping the current track (commands/loop_track.rs; mpv `--loop-file=inf`, also applied live over IPC); ↻ shown in the status bar.

## Sprint 3 — Foundations (branch `sprint-3-overnight`, unattended overnight run)
Plan: `~/.claude/plans/optimized-launching-eagle.md`. Each item: 3 iterations (commit each), then a fresh read-only Opus judge; below 8/10 means more iterations (max 6 extra). Implementers: Opus for auth/PKCE, queue and the Cmd/Event refactor; Sonnet for UI/wiring items. Screenshots (text + PNG, made by `scripts/capture.py` driving the real app in a pty) are in `docs/screenshots/`.

| # | Item | Iterations | Judge | Evidence |
|---|------|-----------|-------|----------|
| 1 | PKCE (S256) in OAuth; client secret optional | 3 + 1 follow-up commit (show Google's `error_description`) | 8/10 | unit tests only (RFC 7636 vector); see below |
| 2 | `queue.rs`: current/upcoming/history, next/skip, add, shuffle_upcoming, loop off/all/one | 3 | 8.5/10 | `item2-*.png` (results, playing, loop-all, shuffle) |
| 3 | `a` queues selected result, "next:" line, `commands/queue.rs` | 3 | 8/10 | `item3-queue-added.png` |
| 4 | Seek `,` `.`; judge fixes (queued-then-Enter, loop-all wrap repeat) | 3 (+2 small inline fixes) | 8/10 | `item4-seek.png`, `item4-help.png` |
| 5 | Playlists: `P` / `F2`, Enter queues+plays, logged-out soft fail via `REQUIRES_LOGIN` | 3 + 3 extra (judge scores 7, 7, 7, then 8) | 8/10 | `item5-logged-out.png` |

- Item 1 live verification: **not done**. A real login needs a browser consent click by the user. A `client_secret.json` exists in the config dir, so `termusic login` should be tried by hand (see docs/QUICKSTART.md). Unknown: whether Google accepts PKCE with a Desktop client with and without the secret (the code sends the secret when present, since Desktop clients still demand it). Decision still open: bundling a shared client ID.
- Known gaps from judges, not fixed: loop-all only wraps over tracks from the started one onward (`Queue::set` drops earlier tracks); turning shuffle off keeps the shuffled order; one shared `busy` flag (a slow mix fetch makes Enter on a playlist a silent no-op); `Char('P')` typed on Search counts as the playlists key for hint expiry; `req_login` is only consulted by `open_playlists`, not at generic dispatch.
- Deviation from process: the 3 extra item-5 iterations were driven by the judge's real bugs (stale replies, Enter fall-through); the last Enter fall-through fix and two small item-4 fixes were made inline by the orchestrator and covered by tests.

## Sprint 4 — Integration (partial: only items needing no user input)
- Item 6 (Cmd/Event split): 3 iterations, judge **8/10**. `src/cmd.rs` (`Cmd`, `Event`, pure `from_key`), `App::dispatch` / `App::on_event`, `Queue::prev` + `b` key (`commands/prev.rs`). Evidence: `item6-after-next.png`, `item6-after-prev.txt`. Not done: position event (no polling yet). Known bug (judge): tracks stepped back with `b` count as user-queued, so they also survive a new search Enter; fixing needs a separate counter. Cmd tests assert on `Cmd`, not state.
- Item 7 (media keys), research spike only, nothing implemented:
  - mpv 0.42 here already has `--media-controls=yes` (default) and `--input-media-keys=yes`; both properties read back `true` from a running mpv over IPC. So mpv may register itself with macOS Now Playing / media keys without any helper. **Unverified** (needs real keys): whether a headless `--no-video` mpv gets the keys, and mpv is respawned per track, so ownership of the keys may flicker between tracks.
  - If it works, mpv's own mapping would act on mpv (pause works, next/prev would not reach the queue). Route them through `Cmd` by binding `MEDIA_NEXT`/`MEDIA_PREV` in an mpv input.conf to `script-message termusic-next` and listening for `client-message` events on the existing IPC socket (needs a reader thread on the socket: the current IPC is write-only).
  - Fallback is the Swift helper from the plan (`MPRemoteCommandCenter` + `MPNowPlayingInfoCenter`, Unix socket to termusic). Swift 6.4 and MediaPlayer.framework are installed on this Mac. Decision still open for the user: accept a Swift component?
  - Next step: manual test with real media keys and AirPods using `mpv --no-video` on a YouTube URL; see whether Control Center shows Now Playing.
- Item 8 (top-right widget), research only: same helper would host an `NSStatusItem` menu bar item (prev/play-pause/next/shuffle/loop/title) driven by `Cmd`/`Event` over the socket; quits when the socket drops. Nothing built; the Swift decision gates it. The `Cmd` enum from item 6 is the contract.
- Item 9 (cassette look): skipped, waits for the user's art.

## Overnight summary
- Branch `sprint-3-overnight`, 24+ commits ahead of `main`; nothing pushed, `main` untouched. `cargo clippy -- -D warnings` clean, `cargo test` 67 passing at the end.
- Judge scores: 8, 8.5, 8, 8, 8 (after 3 extra rounds), 8 (item 6).
- Verified live in a pty (real mpv, yt-dlp, network): search, play, next/prev, queue add + "next:" line, shuffle/loop icons, seek key, help at 100x30, logged-out playlists message, no mpv left running after quit (checked after every capture).
- **Not verified:** real Google login/refresh/playlists; PKCE accepted by Google; media keys; seek audibly changing position of a YouTube track (IPC JSON verified against a local mpv only); loop-file toggled live on a real track; Playlist Enter→play path (no real playlist); 80x24 visuals only through render tests, screenshots are 100x30.

## Sprint 5 — Volume and progress
- Persistent volume (`settings.json`, default 70, `--volume` on every mpv spawn, re-sent on the first successful poll), progress bar with `mm:ss / mm:ss` in the status bar (mpv `time-pos`/`duration` polled about once a second in one IPC round trip, 30ms timeouts), volume bar top-right on every screen (inset so panel corners stay intact; hidden below 40 columns).
- Settings are written atomically (tmp + rename). `TERMUSIC_CONFIG_DIR` overrides the config dir, tokens included. Tests never read or write the real config dir.
- Not verified live against a real mpv: the polling round trip.
