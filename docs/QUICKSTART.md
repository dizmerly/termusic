# termusic quickstart (branch `sprint-3-overnight`)

**Prerequisites:** macOS, `brew install mpv yt-dlp`, Rust toolchain. Google login is optional (needed only for playlists): export `TERMUSIC_CLIENT_ID` (and `TERMUSIC_CLIENT_SECRET` if your Desktop client has one) or keep `client_secret.json` in `~/Library/Application Support/termusic/`.

```sh
git checkout sprint-3-overnight && cargo run
cargo test && cargo clippy -- -D warnings      # 67 tests, clean
```

## Hand test script
1. **Search + play.** Type `daft punk`, Enter, then `j`/`k`, Enter on a track. Status bar shows `▶ title`. ![](screenshots/item2-playing.png)
2. **Queue (item 3).** On results press `a` on two other tracks: status says `queued: ...` and a `next: A · B · C` line appears above the bar; the playing song is not interrupted. `n` plays the queued track first. ![](screenshots/item3-queue-added.png)
3. **Shuffle / loop (items 2, 4).** `s` toggles ⤮ (shuffles only upcoming, never the current track). `l` cycles off → `↻` (all) → `↻1` (one: current track repeats; `n` still skips). ![](screenshots/item2-shuffle.png)
4. **Seek and volume.** `.` jumps forward 5 s, `,` back 5 s, `+`/`-` volume. ![](screenshots/item4-seek.png)
5. **Back (item 6).** `n` then `b` returns to the previous track. ![](screenshots/item6-after-prev.png)
6. **Help.** `?` lists every command, must fit without scrolling in 80x24 (`Esc` closes). ![](screenshots/item4-help.png)
7. **Playlists logged out (item 5).** On Search press `F2` (or `P` on results): status says `Playlists need you to be logged in (P/F2: sign in)`; press again to reach the Login screen. ![](screenshots/item5-logged-out.png)
8. **Playlists logged in.** Complete login in the browser (or `cargo run -- login`), press `P`, Enter on a playlist: it replaces the queue and starts playing; `n`/auto-advance follow playlist order. *Not tested by me.*
9. **No leftover mpv.** Press `q`, then `pgrep -fl "mpv --no-video"` prints nothing. Also try `kill -9` on the termusic pid: within ~2 s the watchdog removes mpv.
10. **Offline.** Turn Wi-Fi off, start the app: Offline screen, `r` retries.

Screenshots are regenerated with `scripts/capture.py` (needs `pip install pyte pillow`), e.g. `scripts/capture.py w:2 t:daft\ punk k:enter w:12 snap:demo k:q`.

## Known gaps / not verified
Real Google login, PKCE acceptance by Google, token refresh; media keys (spike notes in SPRINT.md; mpv may handle them itself, try it); playlist Enter-to-play against a real playlist; seek/loop-toggle on a real track by ear; shuffle-off does not restore order; loop-all wraps only from the started track on; stepped-back (`b`) tracks survive a new Enter; menu bar widget and cassette art (items 8, 9) not started.
