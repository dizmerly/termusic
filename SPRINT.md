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
