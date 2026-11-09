# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`termusic`: a text-only terminal YouTube Music player written in Rust with Ratatui. MVP scope: YouTube search (top-K results, no account), optional Google login for playlists, playback, keyboard-driven TUI with a help menu. Requires internet; offline must be detected and the user prompted to connect.

## Commands

- Build / run: `cargo run` (TUI), `cargo run -- <subcommand>` (e.g. `login`, `logout`, `search <q>`)
- Check / lint / format: `cargo check`, `cargo clippy -- -D warnings`, `cargo fmt`
- Tests: `cargo test`; single test: `cargo test <name>`
- Runtime deps (external binaries): `mpv` and `yt-dlp` must be on PATH for search and playback.
- Config via env: `TERMUSIC_CLIENT_ID`, `TERMUSIC_CLIENT_SECRET` (Google OAuth, "Desktop app" client type) or `client_secret.json` in the config dir.

## Architecture

- `main.rs`: clap CLI + terminal setup/teardown; no subcommand launches the TUI.
- `app.rs`: single `App` state struct + `Screen` enum (Login, Search, Results, Playlists, Help, Offline). All state mutation goes through `App`; `ui.rs` only reads it.
- `ui.rs` / `theme.rs`: Ratatui rendering; logo, icons (unicode), colors live in `theme.rs`.
- `net.rs`: connectivity check (TCP connect w/ short timeout). Called at startup and before every network action; failure switches to `Screen::Offline` with a "connect to the internet" prompt, retry on `r`.
- `auth.rs`: optional Google OAuth 2.0 loopback-redirect flow (Desktop client; device flow is rejected for it), only needed for playlists; tokens in `tokens.json` (0600) in the user config dir, refreshed automatically.
- `youtube.rs`: search via `yt-dlp` (no account; `config::TOP_K`, default 30); playlists via YouTube Data API v3 (first 50 tracks).
- `player.rs`: spawns `mpv --no-video` on a YouTube URL (yt-dlp backend), controlled over mpv's JSON IPC socket (pause, skip, volume). Each track also gets a tiny `sh` watchdog that kills mpv and removes its socket if the app dies without cleanup (SIGKILL).
- `commands/`: one file per user command (`play`, `pause`, `skip`, `shuffle`, `search`, `help`, ...). **Each file begins with `pub const ENABLED: bool` (and any other feature flags) at the very top**; `commands/mod.rs` consults them when dispatching, and `help` lists only enabled commands with descriptions.

## Conventions

- Keybindings must be mnemonic and shown in the help screen (`?`); keep help text in sync with `commands/`.
- Songs are text-only (no thumbnails); decoration is limited to unicode icons/logo.
- Never block the UI thread on network/IO; use tokio tasks and message channels back to `App`.
- Sprint planning and progress live in `SPRINT.md`; update it when finishing work.
