# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`termusic`: a terminal YouTube Music player (text lists, optional pixel-art cover on the cassette label) written in Rust with Ratatui. MVP scope: YouTube search (top-K results, no account), optional Google login for playlists, playback, keyboard-driven TUI with a help menu, styled as a cassette deck (see `docs/SPRINT3-OVERVIEW.md`). Requires internet; offline must be detected and the user prompted to connect.

## Commands

- Build / run: `cargo run` (TUI), `cargo run -- <subcommand>` (e.g. `login`, `logout`, `search <q>`)
- Check / lint / format: `cargo check`, `cargo clippy -- -D warnings`, `cargo fmt`
- Tests: `cargo test`; single test: `cargo test <name>`
- Runtime deps (external binaries): `mpv` and `yt-dlp` must be on PATH for search and playback.
- Config via env: `TERMUSIC_CLIENT_ID`, `TERMUSIC_CLIENT_SECRET` (Google OAuth, "Desktop app" client type) or `client_secret.json` in the config dir.

## Architecture

- `main.rs`: clap CLI + terminal setup/teardown; no subcommand launches the TUI.
- `app.rs`: single `App` state struct + `Screen` enum (Results, Queue, Playlists, History are the four tabs; Help and Settings are overlays on the `stack`; Login and Offline draw in the list pane). Typing a search is a focus flag (`searching`), not a screen: Enter keeps focus in the field; the results reply moves it to the list only if the query is unchanged, and no other async reply may change focus. Each tab keeps its own selection. All state mutation goes through `App`; `ui` only reads it.
- `ui/` + `theme.rs`: Ratatui rendering as "The Deck": `ui/mod.rs` (frame, tier choice: Deck side by side from 76x22, Pocket stacked, Compact list + one-line bar), `ui/deck.rs` (cassette, tape counter, transport keys, status row; pure functions from plain inputs), `ui/jcard.rs` (tab strip, search field, lists), `ui/overlay.rs` (help, settings). Every color comes from the active `theme::Shell` (TPS-L2 default, Sunset tape, Amber hi-fi; `shell` in settings.json).
- `art.rs`: cover art: decode a thumbnail, pixelize it in the shell's inks, choose the draw path (crisp image protocol, half-blocks, off; `TERMUSIC_ART`), fetch (one silent request per track).
- `net.rs`: connectivity check (TCP connect w/ short timeout). Called at startup and before every network action; failure switches to `Screen::Offline` with a "connect to the internet" prompt, retry on `r`.
- `auth.rs`: optional Google OAuth 2.0 loopback-redirect flow (Desktop client; device flow is rejected for it), only needed for playlists; tokens in `tokens.json` (0600) in the user config dir, refreshed automatically.
- `youtube.rs`: search via `yt-dlp` (no account; `config::TOP_K`, default 30); playlists via YouTube Data API v3 (first 50 tracks).
- `player.rs`: spawns `mpv --no-video` on a YouTube URL (yt-dlp backend), controlled over mpv's JSON IPC socket (pause, skip, volume). Each track also gets a tiny `sh` watchdog that kills mpv and removes its socket if the app dies without cleanup (SIGKILL).
- `remote.rs`: control socket: JSON lines over a Unix socket in the config dir (0600; `termusic.lock` so one instance owns it). Each request becomes `Msg::Remote`, handled in `App::tick` by the same setters and `dispatch` as the keys; playback commands get the same offline and disabled gates (`cmd_gate`); state goes out on a watch channel when it changes.
- `helper/`: SwiftPM `TermusicBar` menu bar app. The `TermusicProtocol` library has no AppKit. `swift test` does not build under the command line tools here, so `TermusicBar --selftest` runs the same checks.
- `commands/`: one file per user command (`play`, `pause`, `skip`, `shuffle`, `search`, `help`, ...). **Each file begins with `pub const ENABLED: bool` (and any other feature flags) at the very top**; `commands/mod.rs` consults them when dispatching, and `help` lists only enabled commands with descriptions.

## Conventions

- Keybindings must be mnemonic and shown in the help screen (`?`); keep help text in sync with `commands/`.
- Cover art is optional and off by default, shown only as pixel art on the cassette label for the playing track (half-blocks everywhere, crisp via kitty/sixel/iTerm2 when the terminal reports it); lists stay text-only. Other decoration is limited to unicode icons/logo.
- Never block the UI thread on network/IO; use tokio tasks and message channels back to `App`.
- Sprint planning and progress live in `SPRINT.md`; update it when finishing work.
