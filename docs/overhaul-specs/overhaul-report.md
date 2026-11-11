# Sprint 3 overhaul: report

This report puts the four milestone reports of the Sprint 3 overhaul in one place: the deck layout, motion, cover art, and the control socket with its menu bar helper. Each milestone section says what was built, how it was checked, and what nobody has checked yet. The items that need your decision or your eyes are collected at the end.

## Where things stand

- **Main:** at `c98c749`, a merge of your four newer commits on main with the overhaul (milestones 2 to 4). Not pushed.
- **Checks on the merged tree:** `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean, and `cargo test` passes 262 tests with 2 ignored. The helper's `TermusicBar --selftest` passes. `swift test` does not build under the command line tools here.
- **Branches:**
  - `overhaul-2-motion` and `overhaul-4-helper` hold the work. `overhaul-4-helper` was rebased onto the milestone 2 fixes, so the commit hashes quoted inside the milestone 3 and 4 sections are the pre-rebase ones.
  - `overhaul-3-art` is behind, because your checkout has it open. Once you leave it, `git branch -f overhaul-3-art fdd9727` brings it level, or delete it, since main has everything.
- **Your checkout:** your edits to `.gitignore`, `m3-art-spec.md` and `m4-helper-spec.md` are still uncommitted.
- **Cleanup:** the copy at `.claude/worktrees/agent-ab0720a7d649296ea` was used for the milestone 2 fixes and the merge. It can be removed with `git worktree remove`.
- **Older notes:** `docs/SPRINT3-OVERVIEW.md` keeps the full history, but its milestone 2 section is an older handoff note. This report supersedes it.

| Milestone | State |
|---|---|
| 1. The Deck (layout, shells, tabs) | Done, reviewed, merged earlier |
| 2. Motion (reels, key presses, tape swap, level meters, hiss) | Built and fixed; not independently reviewed; partly checked live |
| 3. Cover art | Done, reviewed by Opus and Sonnet, checked live except the crisp image |
| 4. Control socket and menu bar helper | Done, reviewed by Opus and Sonnet, checked live except by a human at the menu |

## Milestone 1: The Deck

Commits on main, oldest first: 40a06b9 shell color roles; 3319afa tabs, search focus flag, Queue tab, per-tab selection; 78fe731 the Deck layout, Pocket and Compact tiers, TPS-L2 default; 4122fc2 design tweaks; 70f01e2 review fixes; 26c597b search focus rule.

The user sees one Walkman frame. On the left (the deck) is a cassette drawn in text: a paper label with the track title and channel, a window with two reels whose tape packs show the position, a tape counter with a progress line, and six transport keys with their key letters. On the right is the J-card: a tab strip (Results, Queue, Lists, History), a search field, and the list for the active tab. The frame has three color shells (TPS-L2, the default, Sunset tape and Amber hi-fi), chosen in Settings.

### The screen at 80x24

Text render of the playing state, from the app's render test:

```
╭─ TERMUSIC ────────────────────────────────────── ⤮ ↻1 ── VOL ▮▮▮▮▮▮▮▯▯▯ 70% ─╮
│                                    │ 1 RESULTS  2 QUEUE  3 LISTS  4 HISTORY  │
│ ╭────────────────────────────────╮ │ ♪ never gonna give              8 found │
│ │ ┌────────────────────────────┐ │ │─────────────────────────────────────────│
│ │ │ A ──────────────────────── │ │ │▸01 Never Gonna Give You Up  Rick Astley │
│ │ │ Never Gonna Give You Up    │ │ │ 02 Take On Me                      a-ha │
│ │ │                            │ │ │ 03 Africa                          Toto │
│ │ │ Rick Astley                │ │ │ 04 Sweet Dreams (Are Made …  Eurythmics │
│ │ │ ────────────────────── C90 │ │ │ 05 Everybody Wants To R…  Tears For Fe… │
│ │ └────────────────────────────┘ │ │ 06 Don't You (Forget Abo…  Simple Minds │
│ │     ╭────────────────────╮     │ │ 07 Tainted Love               Soft Cell │
│ │     │ ▓▓(◐)▓▓     ▓(◓)▓  │     │ │ 08 Blue Monday                New Order │
│ │     ╰────────────────────╯     │ │                                         │
│ ╰─◦──────────▔▔▔▔▔▔▔▔──────────◦─╯ │                                         │
│                                    │                                         │
│ 072  ━━━━━━●───────────────   1:12 │                                         │
│                                    │                                         │
│   ┌───┐┌───┐┌───┐┌───┐┌───┐┌───┐   │                                         │
│   │◀◀ ││ ▶ ││▮▮ ││▶▶ ││ ⤮ ││↻1 │   │                                         │
│   └─b─┘└─p─┘└─p─┘└─n─┘└─s─┘└─l─┘   │                                         │
│                                    │                                         │
│                                    │                                         │
│  queued: Take On Me                │                                         │
╰─ / search · Tab next tab · Enter play · ? help ───────────────────── TPS-L2 ─╯
```

### How it is built

The colors are named roles in one struct. Each shell is a constant, and every color the UI draws comes from the active shell. Changing the shell in Settings changes only data.

```rust
// src/theme.rs, struct Shell and const TPS_L2
/// A named color scheme: every color the UI draws comes from the active shell's roles.
#[derive(Debug, PartialEq, Eq)]
pub struct Shell {
    /// Shown in the UI.
    pub name: &'static str,
    /// Stored in settings.json.
    pub id: &'static str,
    pub body: Color,
    // ...
    pub select_bg: Color,
}
// ...
pub const TPS_L2: Shell = Shell {
    name: "TPS-L2",
    id: "tps-l2",
    body: rgb(0x16284F),
    frame: rgb(0xB9C2D3),
    // ...
    select_bg: rgb(0x2A4687),
};
```

The tier is chosen from the terminal size alone. Deck needs 76 columns and 22 rows, Pocket needs 38 by 30, and everything else gets the Compact layout (the J-card with a one-line bar).

```rust
// src/ui/mod.rs, enum Tier and fn tier and draw
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tier {
    Deck,
    Pocket,
    Compact,
}

fn tier(w: u16, h: u16) -> Tier {
    if w >= 76 && h >= 22 {
        Tier::Deck
    } else if w >= 38 && h >= 30 {
        Tier::Pocket
    } else {
        Tier::Compact
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    // ...
    let t = tier(area.width, area.height);
```

The cassette is a pure function. It takes the shell and a plain view struct, and returns 12 lines of 34 columns. The view has no reference to `App`, so the drawing can be unit tested.

```rust
// src/ui/deck.rs, struct CassetteView and fn cassette
/// What the cassette shows.
pub struct CassetteView<'a> {
    pub title: &'a str,
    pub channel: &'a str,
    /// position / duration; `None` when the duration is unknown.
    pub progress: Option<f64>,
    /// Reel frame number (0 when paused or idle).
    pub spin: usize,
    /// A track is in the deck (playing or paused).
    pub loaded: bool,
    /// Leave the label's left 10 x 5 cells for the thumbnail.
    pub art: bool,
}

/// The cassette: 12 rows of exactly 34 columns.
pub fn cassette(s: &Shell, v: &CassetteView) -> Vec<Line<'static>> {
    // ...
```

Each reel's tape pack is a level from 0 to 6. The level comes from the playback position, so the left reel empties as the right one fills. Unknown duration gives equal reels (3 and 3), and nothing playing gives two empty reels.

```rust
// src/ui/deck.rs, fn pack and fn levels
/// Tape pack beside a hub, 3 cells read from the outside in: `level / 2` full cells next to
/// the hub and a half cell outside them when the level is odd.
pub fn pack(level: usize) -> String {
    let level = level.min(6);
    let (full, half) = (level / 2, level % 2);
    " ".repeat(3 - full - half) + &"▒".repeat(half) + &"▓".repeat(full)
}
// ...
pub fn levels(v: &CassetteView) -> (usize, usize) {
    match (v.loaded, v.progress) {
        (false, _) => (0, 0), // no tape in the deck
        (true, None) => (3, 3),
        (true, Some(p)) => {
            let p = p.clamp(0.0, 1.0);
            (
                (6.0 * (1.0 - p)).round() as usize,
                (6.0 * p).round() as usize,
            )
        }
    }
}
```

The tab strip is one line that spreads the labels over the pane width. The active tab is reversed. Labels shrink (short names, then digits) until they fit, so the strip never overflows a narrow pane.

```rust
// src/ui/jcard.rs, fn tab_strip
pub fn tab_strip(s: &Shell, active: Option<usize>, w: usize) -> Line<'static> {
    const NAMES: [(&str, &str); 4] = [
        ("RESULTS", "RES"),
        ("QUEUE", "QUE"),
        ("LISTS", "LST"),
        ("HISTORY", "HIS"),
    ];
    // ...
    let mut spans = vec![Span::raw(" ".repeat(edge))];
    for (i, l) in labels.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" ".repeat(gap)));
        }
        let style = if active == Some(i) { on } else { fg(s.dim) };
        spans.push(Span::styled(l, style));
    }
    spans.push(Span::raw(" ".repeat(free.saturating_sub(edge + 3 * gap))));
    Line::from(spans)
}
```

The search-focus rule lives in `tick`. A reply from a playlist or search never takes focus from the search field, so letters typed while a search runs stay text. The one exception is the results of the query that is still in the field, which hand focus to the list. `next_tab` skips tabs that are turned off, so Tab cannot get stuck on a disabled tab.

```rust
// src/app.rs, fn tick (excerpt) and fn next_tab
    pub fn tick(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            // ...
            // Replies never move focus: whoever is typing keeps typing.
            // The one exception: results for the unchanged submitted query hand focus to the list.
            let typing = self.searching;
            let mut focus_list = false;
            // ...
                Msg::Tracks(v) => {
                    focus_list = !v.is_empty() && self.input.trim() == self.submitted;
            // ...
            self.searching = typing && !focus_list;
            // ...
}

/// The tab `step` places after `from` (3 = back one) that is not off; `from` if none.
fn next_tab(from: usize, step: usize, off: [bool; 4]) -> usize {
    (1..=TABS.len())
        .map(|k| (from + step * k) % TABS.len())
        .find(|&i| !off[i])
        .unwrap_or(from)
}
```

`Queue::jump` makes a chosen upcoming track the current one. The tracks before it move to the played side and the rest stay upcoming, so Enter on the Queue tab does not rebuild the queue.

```rust
// src/queue.rs, fn jump
    /// Make `upcoming[n]` current; the current track and those before it count as played.
    /// `None` (no change) if there is no such track.
    pub fn jump(&mut self, n: usize) -> Option<&Track> {
        if n >= self.upcoming.len() {
            return None;
        }
        self.history.extend(self.current.take());
        self.history.extend(self.upcoming.drain(..n));
        self.user_added = self.user_added.saturating_sub(n + 1);
        self.current = self.upcoming.pop_front();
        self.current.as_ref()
    }
```

### Keys

| Key | Where | Does |
|---|---|---|
| `Tab` / `Shift-Tab` | list focus | next / previous tab, wrapping |
| `1` `2` `3` `4` | list focus | jump to that tab |
| `/` | list focus | focus the search field |
| `Esc` | search focus | leave the field, keep the text (no longer quits) |
| `Enter` | Queue tab | play the selected track (Queue jump) |

Enter on any empty list does nothing. Typed letters in the search field are text, not commands.

### Who did what

- Fable wrote the design spec (cassette 34x12, six key caps, three shells, J-card tabs, three size tiers) and reviewed renders and color screenshots.
- An Opus agent implemented the spec in three commits plus one tweak commit.
- Fable made design tweaks after the first renders: History rows show only the resume time, with the youtu.be link in the deck's status row; the result count moved from the tab strip to the search row ("30 found"); the Compact bar drops its hint, then the volume, before cutting the title; an idle cassette shows two empty reels.
- A Sonnet agent fixed the review findings and added the tests.

### Review

A fresh Opus agent reviewed the milestone. The first verdict was 7/10, "fix first". After the fixes, the re-review gave 8.5/10, "ship".

Review findings that were fixed:
1. A playlist or search reply arriving while the user typed took focus from the search field, so typed letters ran as commands (q quit the app). Now no reply changes focus except the results of an unchanged query.
2. Enter on an empty Results tab cleared the queue and stopped playback. Enter on any empty list now does nothing.
3. Tab cycling got stuck when a tab's command was disabled.
4. `fit`/`pad` could return text wider than asked for emoji with a variation selector. Control characters in the status or search input are now replaced with spaces when drawn.
5. Selection is clamped when an overlay closes.

One instruction from Fable was wrong and was reversed after the re-review: dropping search focus the moment Enter is pressed. It made keys typed during a running search act as commands.

### Verified

- `cargo fmt --check` clean, `cargo clippy --all-targets -- -D warnings` clean, `cargo test` 174 passed, 0 failed, 1 ignored (the ignored test is `dump_renders`, which prints the screens). Tests went from 135 to 174.
- The reviewer's mutation checks: 13 of 17 mutations were caught on the first pass; the 4 survivors got tests.
- Live in a pty against real mpv and yt-dlp with a scratch config dir at 5% volume: search, play, typed letters after Enter stay text, Enter on an empty tab does nothing, Queue tab jump plays the chosen track, History tab shows the link in the status row, all three shells cycle from Settings, no mpv left running after quit.
- Color screenshots of all three shells were checked by Fable.

### Not verified

- Real Google sign-in and playlists.
- The Pocket and Compact tiers in a real terminal (render tests only).
- How glyphs such as the shuffle icon and the pause bars look in the user's own terminal font.
- Right-to-left text in a real terminal.

### Removed

- The big ASCII TERMUSIC logo.
- The previous 13-line cassette art (`theme::CASSETTE`).
- The `next:` line (replaced by the Queue tab).
- The two-step login hint.

## Milestone 2: Motion, meters, hiss

The spec is `docs/overhaul-specs/m2-motion-spec.md`. An Opus agent built it in five commits (`7516142` clock and reels, `782ad73` key caps and tape swap, `783037f` level meters, `2de9b4c` hiss, `09c6f1b` SPRINT.md) and was stopped before it reported. The fixes below came later in this session.

### What was built

| Spec section | Where |
|---|---|
| 1 Animation clock, `animations` setting | `src/anim.rs` (`Anim`, `FRAME`), `App::animating`, `run_tui` in `src/main.rs`, `Row::Animations` |
| 2 Reels: a step every 150 ms while playing, a 400 ms whirr after seek, next or prev | `Anim::advance`, `reel_steps`, `Anim::whirring`, `App::dispatch` |
| 3 Key caps that press down | `key_caps` in `src/ui/deck.rs`, `Anim::pressed_key` |
| 4 Tape swap on track change (eject, insert, seat: 200 ms each) | `swap_phase`, `Anim::swap_to`, `slide` and `cassette` in `deck.rs` |
| 5 Counter digits blink dim when they change | `counter_row`, `Anim::dim_digits` |
| 6 Real level meters (mpv `astats`, 10 reads a second, 14 segments, -42 to 0 dB, 500 ms peak hold) | `LEVEL_FILTER`, `meter_loop`, `read_levels`, `parse_levels` in `src/player.rs`; `segments`, `by_volume`, `peak_fall` in `anim.rs`; `meter_row` in `deck.rs` |
| 7 Tape hiss: a second mpv playing pink noise through a 1 kHz high-pass, off by default | `sync_hiss`, `hiss_command`, `hiss_volume`, `HISS_*` in `player.rs`; `Row::Hiss`, `Row::HissLevel` |

Other changes:
- With animations off, the deck is the milestone 1 deck, the meter row is blank, and the loop runs every 200 ms instead of 50 ms.
- The Pocket tier now needs 31 rows, up from 30, because the key block grew by a row. The spec allowed this.

### Fixed after the handoff

| Commit | Change |
|---|---|
| `e96f492` | **Reels hold when paused or idle** (approved by you). Seek, next and prev spin the reels only while a track plays, and the swap's seat phase no longer spins them on a paused start. Test: `idle_or_paused_deck_does_not_whirr`. |
| `2fadd44` | **A hiss mpv that dies is noticed.** `sync_hiss` checks its exit, cleans up its watchdog and socket, and starts it again at most 3 times a session, then stays off without a message. Test: `dead_hiss_is_respawned_three_times_then_given_up`, which uses a fake mpv. |
| `b00e4f1` | **Hiss loudness text corrected.** mpv's volume is cubic, so hiss level 30 is about 31 dB under the music from the volume scale alone, and about 37 dB counting `HISS_AMPLITUDE` 0.5. This is arithmetic, not a measurement. |
| `9673b5d` | **Meters scale with the app volume** (your request). From 80 % up the meter can reach the red top. Below that, its reach shrinks in a straight line: about half at 40 %, a fifth at 16 %. `FULL_SCALE_VOLUME` in `src/anim.rs` sets the 80 %. |

### Verified

- Unit and render tests for each section. The truth table for which states animate. The meter scaling at 100, 80, 70, 40, 16 and 0 % volume.
- From the live runs of milestones 3 and 4, with a real mpv and yt-dlp:
  - the `astats` levels read correctly from a real YouTube stream;
  - the hiss mpv starts;
  - nothing is left running after a normal quit.
- The hiss source keeps playing without `--loop-file`: a silent mpv was still alive after 84 s.

### Not verified

- Preload and crossfade with the level filter on both mpvs, and the meters following the promoted track.
- The hiss following pause (including a pause from outside the app), the volume slider and track changes.
- Cleanup after `kill -9` of the app.
- The animations-off loop at 200 ms in a real run.
- How the hiss sounds, and its real level measured rather than computed.
- CPU use of the 50 ms loop and the 10 Hz meter thread.
- An independent review of milestone 2 (milestones 1, 3 and 4 each had one).
- The milestone 2 entry in `SPRINT.md` says "verified live" for the meter and hiss behaviour. That claim came from the implementer and nobody has checked it.

### Decisions in the code that the spec did not dictate

1. The hiss amplitude is 0.5, not the spec's 0.05, and the source has no `duration` and no `--loop-file`.
2. Level meters follow the Animations setting: with animations off there are no reads and the row is blank.
3. The meter row is 34 columns wide, not the 36 the spec draws (open question, see the end).
4. The lit segments and the peak both come from the RMS level; Peak_level is requested but unused.
5. The meter thread lives until the app exits once started (it idles at 100 ms steps).
6. The hiss ignores a crossfade's volume ramp. It stops when no track is loaded and keeps playing across a track change.
7. The first track after start skips the eject phase.

## Milestone 3: Cover art
Branch `overhaul-3-art`, cut from `overhaul-2-motion`. This report describes what was built, the libraries used, the design choices, what the tests cover, what the review found, and what was and was not checked.

### What was built

- A **Cover art** setting (Settings screen, off by default, saved in `settings.json` as `thumbnails`). When on, the playing track's YouTube thumbnail is drawn on the cassette label as pixel art in the active shell's colors.
- The thumbnail is fetched once per track from `https://i.ytimg.com/vi/{id}/mqdefault.jpg` (`src/art.rs`, `fetch`), with a 5 s timeout and a 1 MiB cap. The bytes are decoded as JPEG on a blocking thread, center-cropped to a 40x40 RGB square (`decode`, `center_square`).
- Pixelizing (`pixelize_with`): the 40x40 square is resized to an n x n grid, stretched onto the palette's brightness range (`stretch`), then each cell gets the nearest of the shell's inks after a 4x4 Bayer dither (`DITHER_SPREAD`, currently 32).
- Two draw paths, chosen once at startup (`art::detect`, `art::choose`):
  - **Half-blocks** (every terminal): a 10x10 grid drawn as 10 columns by 5 rows of `▀` cells, one pixel per half cell.
  - **Crisp** (only when the terminal reports kitty, sixel or iTerm2): a 20x20 grid upscaled to 10 x font-width by 5 x font-height pixels and handed to `ratatui-image` (`art::protocol`, `Resize::Fit`).
- The art is cached per track in `App::art` (8 entries, oldest evicted first, `ART_CACHE`). The crisp protocol is cached in `App::cover_proto`, keyed by track id, shell id and font size (`CoverKey`).

Where the code lives:

- `src/art.rs`: decode, dither, stretch, path choice, terminal detection, fetch, protocol build.
- `src/app.rs`: `request_art` (fetch, once per id), `set_thumbnails` (the toggle and the one-time notice), `request_current_art`, `cover_key` and `sync_cover` (protocol cache), and the `Msg::Art` reply handling.
- `src/ui/deck.rs`: `label` (the paper label, with the art area), `cassette` (the whole cassette), `slot_rect` (where the 10x5 slot lands), `key_caps` (transport and mode keys).
- `src/ui/mod.rs`: the frame; hands the cover to the deck.

Relevant excerpt, the request path in `src/app.rs`:

```rust
fn request_art(&mut self, id: &str) {
    if !self.settings.thumbnails
        || self.art_path == art::ArtPath::Off
        || self.screen == Screen::Offline
        || !art::valid_id(id)
        || !self.art_tried.insert(id.to_owned())
    {
        return;
    }
```

The request is skipped when the setting is off, when the path is off, when the app is offline, when the id is not a valid YouTube id, and when the id was already requested (`art_tried`, which doubles as the in-flight set).

### Technologies

| Crate | Version (Cargo.toml) | Features | Why | Cost |
|---|---|---|---|---|
| `image` | 0.25 | `default-features = false`, `jpeg` only (dev-dependencies add `png` for the contact sheet test) | Decode the JPEG thumbnail, crop, resize, convert to RGB | Only the JPEG decoder is built. `ratatui-image` also depends on `image` 0.25.10, and cargo unifies the features, so the `jpeg` feature is shared. |
| `ratatui-image` | 11.1 | `default-features = false`, `crossterm` only | Kitty graphics, sixel and iTerm2 protocols; terminal capability query | Pulls the transitive crates listed below. Without the default features, the crate's other backends are off. |
| `reqwest` | existing (0.13.5, rustls) | none added | The thumbnail HTTP fetch | None new; already used by `youtube.rs` and `auth.rs`. |

Transitive dependencies of `ratatui-image` 11.1 (from `cargo tree -p ratatui-image --depth 1`):

- `base64-simd` 0.8.0
- `flate2` 1.1.11
- `icy_sixel` 0.5.1
- `image` 0.25.10
- `rand` 0.8.8
- `ratatui` 0.30.2 (already a direct dependency)
- `rustix` 0.38.44
- `self_cell` 1.3.0
- `thiserror` 1.0.69

The list is the direct dependency set only; each crate's own dependencies were not expanded. The binary size and compile time impact has not been measured.

### Design decisions

1. **Pixelize into the active shell's own inks (10 colors), with 4x4 Bayer dithering, spread 32.** The palette is `art::shell_palette`: the shell's body, select, dim, frame, text, accent, label, label-ink, meter-low and meter-high colors, with duplicates removed. The spread was chosen from a contact sheet of 4 real thumbnails, 3 shells, and spreads 0, 32 and 64 (`contact_sheet`, an ignored test that writes a PNG). Spread 64 was too noisy at 10x10.
2. **Tonal stretch**, added by the designer after the contact sheet. Dark covers collapsed into the darkest ink. `stretch` maps the image's luma range onto the palette's luma range with one affine map on all channels, so hue is kept. A flat image or a flat palette is left alone.
3. **Half-blocks everywhere; the crisp image only where a protocol is reported.** Half-blocks work in every terminal and need no query. The crisp image is only worth the risk where the terminal really supports an image protocol.
4. **The terminal query runs only when the environment names an image-capable terminal.** `art::should_query` checks `TERM_PROGRAM` (ghostty, iterm.app, wezterm, kitty), `TERM` (contains kitty or ghostty) and `KITTY_WINDOW_ID`. The reason: `ratatui-image`'s query thread outlives its timeout, and in a terminal that never answered it swallowed typed input (found in a live pty check). `TERMUSIC_ART` is read case-insensitively with surrounding whitespace removed (`normalize`): `detect` forces the query, `halfblocks` forces half-blocks, `off` turns art off, and `halfblocks` and `off` never query. The query has a 1 s timeout. A query that hits the timeout counts as failed, even if the library returned a guessed picker (`.filter(|_| start.elapsed() < timeout)`).
5. **One-time status notice.** When Cover art is switched on in a terminal without an image protocol, the status line says so once per session (`art_noted`). Off never shows the notice on the crisp path. This was requested as the error handling for that case.
6. **Covers: one request in flight per track, silent on failure.** A track's cover is requested on track start, on the preloaded track, on the toggle, and on reconnect. A failed or evicted id is removed from `art_tried`, so the next natural trigger asks again. Failures never write a status message and never switch to the Offline screen. The cache holds 8 covers. While streaming, the body is capped at 1 MiB (`push_capped`), so a chunked response without a content length cannot grow without limit.
7. **Art hidden during the tape swap's eject and insert phases, shown when seated.** The label slides as paper while the tape moves, so the crisp image (which the terminal draws over the cells) is only drawn in the seated phase. In `deck.rs`: `if let (Some(p), None | Some(Phase::Seat), Some(r)) = (crisp, v.phase, slot_rect(stack))`.
8. **Transport key caps widened to 6 and a one-column gap before the mode keys.** The prev, pause and next caps are 6 columns wide so their two-column glyphs center. A single column of space after next separates the transport bank from shuffle and loop, and makes the key row exactly the 34-column cassette width. The gap was decided by the Fable designer. Code: `key_caps` in `src/ui/deck.rs`, which pushes one space after the `Key::Next` cap.
9. **Branch `overhaul-3-art` was cut from `overhaul-2-motion`.** Milestone 2 is not merged into `main`, so this branch carries milestone 2's changes too.

### Test coverage

All tests pass. The full suite is 235 passed, 2 ignored. The ignored tests are `contact_sheet` in `src/art.rs` (writes a PNG for design review; reads its inputs from `TERMUSIC_THUMBS`) and `dump_renders` in `src/ui/mod.rs` (not part of this milestone's table).

#### `src/art.rs`

| Requirement | Test(s) |
|---|---|
| A solid color maps to the nearest palette ink | `solid_color_maps_to_nearest_palette_color` |
| A gradient uses both inks and only those | `gradient_uses_both_colors_and_only_those` |
| A dark image still uses the light ink (tonal stretch) | `a_dark_image_still_uses_the_light_ink` |
| Non-square input is center-cropped | `non_square_input_is_center_cropped` |
| Output has the requested dimensions | `output_dimensions` |
| Garbage bytes decode to `None` | `garbage_bytes_give_none` |
| A one-pixel image decodes | `one_pixel_image_decodes` |
| Shell palettes have no duplicate inks | `shell_palettes_have_no_duplicates` |
| Every row gets a path choice (env and query combinations) | `choose_covers_every_row` |
| The terminal query runs only for hinted terminals | `should_query_only_for_hinted_terminals` |
| `TERMUSIC_ART` is case and whitespace insensitive | `art_env_is_case_and_whitespace_insensitive` |
| The byte cap stops past the limit | `push_capped_stops_past_the_cap` |
| Upscaling gives solid blocks | `upscale_gives_solid_blocks` |
| A YouTube id is exactly 11 URL-safe characters | `valid_id_is_eleven_url_safe_characters` |
| Contact sheet for design review (ignored) | `contact_sheet` |

#### `src/app.rs`

| Requirement | Test(s) |
|---|---|
| Nothing is fetched when off, offline, the id is invalid, or already tried | `request_art_skips_when_off_offline_invalid_or_tried` |
| Track start fetches the current id | `track_started_alone_requests_the_current_id` |
| Preload fetches the next id; both together make one request each | `preload_alone_requests_the_next_id_and_both_make_one_request_each` |
| An in-flight id is not requested twice | `an_in_flight_id_is_not_requested_twice` |
| An evicted id can be requested again | `an_evicted_id_can_be_requested_again` |
| A failed id is asked again on its next track start | `a_failed_id_is_asked_again_on_its_next_track_started` |
| Reconnect fetches the current cover | `reconnecting_fetches_the_current_cover` |
| Turning Cover art on fetches the loaded track | `turning_cover_art_on_requests_the_loaded_track` |
| Turning Cover art on also fetches the preloaded track | `turning_cover_art_on_also_requests_the_preloaded_track` |
| Replies are cached oldest first, capped at 8 | `art_replies_are_cached_oldest_first_and_capped` |
| The notice shows once on half-blocks; never on crisp; Off stays silent | `notice_shows_once_on_half_blocks_and_off_never_on_crisp` |
| The off path requests nothing | `off_path_requests_nothing` |
| The protocol is built once per key and rebuilt on shell change | `cover_protocol_is_built_once_per_key_and_rebuilt_on_shell_change` |
| A failed protocol build is remembered, not retried | `failed_cover_build_is_remembered_not_retried` |
| No protocol without the crisp path or a cached source | `no_cover_protocol_without_the_crisp_path_or_a_cached_source` |

#### `src/ui/deck.rs`

| Requirement | Test(s) |
|---|---|
| The art slot hides in eject and insert only | `art_slot_hides_in_eject_and_insert_only` |
| The placeholder is a two-tone paper checker | `placeholder_is_a_two_tone_paper_checker` |
| Color mixing moves toward the second color | `mix_moves_toward_the_second_color` |
| The label text leaves the art area when asked | `label_leaves_the_art_area_when_asked` |
| The crisp image lands on the label slot | `crisp_image_lands_on_the_label_slot` |
| A failed protocol falls back to half-blocks; Off draws nothing | `failed_protocol_falls_back_to_half_blocks_and_off_draws_nothing` |
| Key caps are 6 and 5 columns wide, 4 rows, and press down | `key_caps_are_6_and_5_wide_4_rows_and_press_down` |
| Cassette rows are 34 columns for any title | `cassette_rows_are_34_columns_for_any_title` |

#### `src/ui/mod.rs`

| Requirement | Test(s) |
|---|---|
| The cover is drawn in the shell's inks | `cover_is_drawn_in_the_shells_inks` |
| The placeholder shows until the cover is cached | `placeholder_until_the_cover_is_cached` |
| Setting off draws the plain label | `cover_setting_off_draws_the_plain_label` |
| Half-block cover hides in eject and insert, shows when seated | `cover_hides_in_eject_and_insert_and_shows_when_seated` |
| Crisp image hides in eject and insert, shows when seated | `crisp_image_hides_in_eject_and_insert_and_shows_when_seated` |
| Changing the shell recolors the cover | `changing_shell_recolors_the_cover` |
| Nothing panics at any size, with art off, half-block and crisp | `nothing_panics_at_any_size` |
| The 80x24 deck renders | `deck_at_80x24` |

#### Not covered by tests

- The live network fetch of the thumbnail (`art::fetch` against i.ytimg.com).
- The real terminal query and its 1 s timeout branch.
- The crisp image in a real terminal (kitty, sixel, iTerm2, Ghostty, WezTerm).
- Sixel and iTerm2 image overflow after a font-size change. This is a known limit, marked `shortcut:` in `art::protocol`.
- That exactly one request is made per track. The tests and code reading show this; real requests were not counted.

### Review

Two independent reviews were run: an Opus review and a Sonnet edge-case review.

Fixed:

- Covers were never fetched again after eviction or after a failed fetch. Now the id leaves `art_tried` and the next natural trigger asks again.
- No refetch after reconnect. Reconnect now fetches the current cover.
- A preloaded next track was not fetched when the setting was turned on. Turning it on now requests both.
- `TERMUSIC_ART` was an exact match. It is now case and whitespace insensitive.
- A late terminal reply could be read as keystrokes. Mitigated by the 1 s timeout and by the hint gate (the query only runs in hinted terminals).
- A query that timed out could still pick the crisp path. A timed-out query now counts as failed.
- The chunked body was not capped while streaming. It is now capped at 1 MiB.
- Some tests were weak. They were strengthened.

Accepted, not changed:

- A wrong cover is possible when two queue tracks share a title. The channel line uses the same rule, so the cover follows it.
- The placeholder stays for non-YouTube ids. The spec says no status message for this case.
- Sixel and iTerm2 images can overflow the slot after a font-size change (see the `shortcut:` note).

### Verified live

Checked in a pty with a real `mpv` and `yt-dlp`, using a terminal that does not answer image queries:

- Startup took under 1 s.
- A typed search reached the app.
- A real track played.
- The label slot showed the cover as 10x5 half-block cells in 8 shell colors.
- No `mpv` was left running after quit.
- Milestone 2's `astats` level meters read correctly from a real YouTube stream. This is the first real-stream check of milestone 2.

### Known limit (checked live)

In a pty that never answers image queries but whose environment says `TERM_PROGRAM=ghostty`, the query runs and times out: the first frame took 1.33 s, and the search text typed right after startup never reached the app (the crate's query thread reads one more chunk of input after its timeout, then exits). Later input works. Real Ghostty, kitty, iTerm2 and WezTerm answer, so this only bites when an image-terminal hint reaches a terminal path that drops the reply (for example `TERM=xterm-ghostty` over ssh). Workaround: `TERMUSIC_ART=halfblocks`. Without a hint (any other terminal) no query runs and nothing is lost (also checked live).

### Not verified

- The crisp path in Ghostty, kitty or iTerm2. Only the author can look at this.
- That exactly one request is made per track. This is shown by tests and code reading, not by counting real requests.
- How it looks by eye in the author's terminal.

### Items for the author to review

- The dithering look per shell: `DITHER_SPREAD` in `src/art.rs` (32), and the shell palettes in `shell_palette`.
- The tonal stretch in `stretch` (`src/art.rs`).
- The key row spacing: the 6-column transport caps and the one-column gap in `key_caps` (`src/ui/deck.rs`).
- The README "Cover art" section. It already describes the setting, the detection hints, `TERMUSIC_ART`, and the one-time notice. It also says that WezTerm goes through the iTerm2 protocol because the `image` crate blacklists its kitty and sixel paths. That claim was not re-checked in this report.

## Milestone 4: Remote control and menu bar helper
Branch `overhaul-4-helper`, cut from `overhaul-3-art` (four commits on top of `9194e4e`). This report describes what was built, the technologies, the design choices, what the tests cover, what the review found, and what was and was not checked. The design is in `docs/overhaul-specs/m4-helper-spec.md`.

### What was built

- **A control socket** in termusic (`src/remote.rs`). It is a Unix socket at `<config dir>/termusic.sock`, mode 0600. Clients send one JSON object per line; termusic answers with state lines and error lines.
- **The protocol** (the table is in the README, section "Menu bar helper (macOS)"): playback commands (`play_pause`, `next`, `prev`, `seek`, `toggle_shuffle`, `cycle_loop`), settings (`volume`, `loop`, `shuffle`, `shell`, `thumbnails`, `animations`, `hiss`, `hiss_level`), and `get state`.
- **State lines** carry the title, channel, id, loaded and paused flags, position (whole seconds), duration, volume, loop, shuffle, the shell id and list, and the settings. They are sent on connect, after each accepted request, and whenever the state changes.
- **A menu bar helper** (`helper/`, SwiftPM package, executable `TermusicBar`). It shows `♪` and the title in the menu bar, and a menu with title and channel, Play/Pause, Previous, Next, a volume slider, Shuffle, a Loop submenu, and a Settings submenu (Shell, Cover art, Animations, Tape hiss, Hiss level). Quit quits only the helper.

Where the code lives:

- `src/remote.rs`: `parse` (line to `Request`), `bind` (ownership rule), `serve` and `client` (accept loop and per-client tasks), `Reply`, `State` and `ShellInfo` (what clients see), `socket_path`.
- `src/app.rs`: `serve_remote` (binds and starts the server), `tick` (handles `Msg::Remote`, at most `REMOTE_PER_TICK` = 32 per tick), `apply` (maps a `Request` onto the setters and `dispatch`), `cmd_gate` (offline and disabled checks for playback commands), `snapshot` (builds the state, no IO), `publish` (sends it on the watch channel when it changed), `shutdown` (removes only our own socket), and the new setters `set_volume`, `set_shell`, `set_loop`, `set_shuffle`.
- `helper/Sources/TermusicProtocol/Protocol.swift`: decoding of state and error lines, request encoders, socket path, and the menu title. No AppKit import.
- `helper/Sources/TermusicBar/Connection.swift`: the socket client (POSIX sockets, one background queue, reconnect every 2 s).
- `helper/Sources/TermusicBar/main.swift`: the menu (`Bar` class) and the throttled sliders.
- `helper/Sources/TermusicBar/SelfTest.swift`: `--selftest`, the same checks as the Swift test target.

### Technologies

- **Rust, no new crates.** `Cargo.toml` is unchanged from `overhaul-3-art`. The code uses what the project already had:
  - `tokio` (already `full`): `UnixListener` and `UnixStream` for the socket, `watch` for the state snapshot, `mpsc` for replies and the request queue, `Notify` to close a client.
  - `serde` (derive) and `serde_json`: the state object and the error lines are built with `json!` and `Serialize`. Requests are parsed from `serde_json::Value`.
  - `std::fs::File::try_lock` (exclusive `flock` on Unix) for `termusic.lock`. The lock is released when the process exits, including after a crash.
- **Swift, no packages.** `helper/Package.swift` declares swift-tools-version 5.9 and a macOS 13 minimum. The toolchain here is Swift 6.4 (`swift --version`). The helper uses AppKit (`NSStatusBar`, `NSMenu`, `NSSlider`) and POSIX sockets (`socket`, `connect`, `poll`, `read`, `write` through `Darwin`). No third-party package and no Xcode project.

### Design decisions

1. **Built on milestone 3.** The branch is cut from `overhaul-3-art` because the protocol includes `set thumbnails`, which is milestone 3's setting. The spec header says the work builds on milestones 1 and 2 and that they are merged into `main`; that header is out of date. Milestone 2 is not merged into `main` either (see the m3 report, design decision 9), so this branch carries milestones 2 and 3 with it.
2. **One code path.** Every request becomes a `Msg::Remote` and is applied on the UI thread by `App::apply`, which calls the same setters and `dispatch` as the keys. The setters that did not exist before are `set_volume`, `set_shell`, `set_loop` and `set_shuffle`; `set_animations`, `set_thumbnails`, `set_hiss` and `set_hiss_level` were already there. Playback commands go through `cmd_gate` first: while offline they get `offline`, and when their key is disabled in `commands/` they get `disabled`. The keyboard gets the same results in practice: the Offline screen takes no playback keys, and `handle_list_key` skips disabled keys. Settings requests are not gated by offline, because the spec asks that settings work offline; the keyboard cannot change settings offline. A remote `shuffle` or `loop` change plays the same key-press animation as the key (`press`, through `apply` and `dispatch`).
3. **Ownership.** `termusic.lock` is taken with an exclusive lock before anything else. Then the socket path is checked: a socket that answers belongs to another termusic and is left alone, a socket that does not answer is a crash leftover and is removed, and a path that is not a socket (a regular file, for example) is left alone with a status line. The new socket is bound under a temporary name (`termusic.sock.<pid>`), chmod 0600, and renamed into place, so the socket never exists with looser permissions. The config dir is created with mode 0700 if it is missing. On shutdown, the socket is removed only if its (dev, ino) still matches the socket this process bound, so a socket that another bind put in place survives.
4. **Flow control.** A client does not have to wait for replies: the reader keeps reading and never blocks on the writer. At most 4 clients are served at once (`MAX_CLIENTS`); extra connections are closed. A client's reply queue holds 16 lines (`OUT_QUEUE`); a full queue closes that client. A write that takes longer than 2 s closes the writer (`WRITE_TIMEOUT`). A line over 4096 bytes closes the client (`MAX_LINE`). A slot is freed only when both the reader and the writer task of the client have ended (the `Guard` counter). The app handles at most 32 remote messages per tick (`REMOTE_PER_TICK`), so one flooding client cannot stall a frame.
5. **State lines.** The snapshot is sent only when it differs from the last one (`send_if_modified`). Position is floored to whole seconds, so while playing there is about one state line a second. Each accepted request gets exactly one reply: a state line for a valid request (`get state` and every setter), or an error line. A state change can add an unsolicited state line, so a client may see two state lines for one request. Malformed JSON gets an error line and the connection stays open.
6. **The helper renders only the last state line.** It keeps no state of its own and never guesses: if the connection is down, the title reads `termusic is not running`, the menu bar item is dimmed, and all controls except Quit are disabled. It reconnects every 2 s. The volume and hiss-level sliders send at most 10 values a second while dragging (`throttled`, 0.1 s), and the release always sends the final value. A slider is not overwritten from the state while the mouse is dragging it. The Shell submenu is rebuilt only when the `shells` list changes, because replacing items under an open submenu breaks it. Shell names are shown as the app sends them (`SUNSET TAPE`, `AMBER HI-FI`); the helper does not change their case.

### Test coverage

Verified in this session: `cargo test` reports 258 passed and 2 ignored (milestone 3 was 235 passed). `remote.rs` has 23 tests, which is the difference.

#### `src/remote.rs`

| Requirement | Test(s) |
|---|---|
| Every row of the request table parses to the right request | `parse_every_request` |
| Bad input is rejected: unknown cmd, unknown set key, wrong types, out of range, non-JSON, empty, non-object, non-UTF-8 | `parse_rejects_bad_input` |
| Boundaries: seek clamped to -600..=600, shell ids, float volume rejected | `parse_boundaries` |
| The state object has the spec's field names and nulls | `state_has_the_spec_field_names_and_nulls` |
| Snapshot: nothing loaded, loaded, paused, odd positions (NaN, infinity, negative) | `snapshot_loaded_empty_paused_and_odd_positions` |
| Socket: state on connect, a request changes the app and gets state back, garbage gets an error and the connection stays open, `toggle_shuffle` changes shuffle | `requests_change_the_app_and_get_state_back` |
| Every request arm changes the app | `every_request_arm_changes_the_app` |
| Two `get state` requests make two state lines | `two_get_states_make_two_state_lines` |
| A burst of 30 requests is applied in two ticks | `a_burst_of_requests_is_applied_in_a_tick_or_two` |
| A change from one client, or from the app itself, reaches every client | `other_clients_and_the_app_itself_reach_every_client` |
| Line limit: 4097 bytes closes the client, 4096 is accepted | `oversize_line_closes_the_client` |
| At most 4 clients; a fifth is closed; a freed slot can be taken again | `a_fifth_client_is_closed_and_four_stay` |
| Slots are freed when a client never reads (flood test) | `slots_free_when_a_client_that_never_reads_is_gone` |
| Offline: playback commands get `offline`, settings still apply | `offline_rejects_commands_but_not_settings` |
| Disabled: playback keys get `disabled`, as on the keyboard | `disabled_commands_are_rejected_like_the_keyboard` |
| Remote shuffle and loop changes press their cap; no change, no press | `remote_mode_changes_press_their_cap` |
| Socket path longer than the limit is refused with a status line | `a_too_long_path_is_refused` |
| No config dir: no remote control | `no_config_dir_means_no_remote_control` |
| A second app is locked out; a quit frees the lock | `a_second_app_is_locked_out_and_a_quit_frees_the_lock` |
| A held lock skips remote control and leaves the socket alone | `a_held_lock_skips_remote_control_and_leaves_the_socket_alone` |
| A non-socket file at the path is left alone | `a_path_that_is_not_a_socket_is_left_alone` |
| A dead socket is replaced; the socket is mode 0600; a missing config dir is created 0700; only the socket and the lock are left | `a_dead_socket_is_replaced_with_mode_0600` |
| Shutdown keeps a socket that is no longer ours | `shutdown_keeps_a_socket_that_is_no_longer_ours` |

Helpers used by the socket tests (not tests themselves): `tmp`, `line`, `until`, `connect`, `pump`, `setup`, `ask`.

#### Swift

Two sets of checks, with the same content:

- `helper/Tests/TermusicProtocolTests/ProtocolTests.swift`: 10 Swift Testing tests (`decodesRealLine`, `decodesValuesAndUnknownField`, `decodesErrorAndRejectsJunk`, `requestBytes`, `socketPaths`, `titles`, `nullsDecode`, `appErrorsDecode`, `missingFieldIsNil`, `nullPositionDecodes`).
- `TermusicBar --selftest` (`helper/Sources/TermusicBar/SelfTest.swift`): 35 checks, the same cases plus the request bytes for every setter. Verified in this session: the existing release binary prints `ALL PASS` and exits 0. I did not rebuild it; its timestamp is newer than the sources.

`swift test` does not run under the command line tools here. Verified in this session, it fails while building the test target with `external macro implementation type 'TestingMacros.TestDeclarationMacro' could not be found for macro 'Test'; plugin for module 'TestingMacros' not found`. The test target should run under Xcode.

#### Not covered by tests

- Rust: the per-tick cap (`REMOTE_PER_TICK`). The burst test sends 30 requests, below the cap of 32, so no test reaches it. The accept-error retry in `serve`.
- Swift: `Connection` (socket I/O, reconnect, the 10 per second slider throttle) and the menu (`Bar`). They are not covered by any check; the encoders and decoders are.
- Live behaviour of the menu (layout, wording, the dimmed title, slider feel).

### Review

Two independent reviews were run: an Opus review and a Sonnet edge-case review.

Fixed:

- The 4-client cap could be bypassed by a client that never reads: its writer task and file descriptor leaked. Now a slot is held until both the reader and the writer end, and a full reply queue closes the client.
- Slider lag: one request in flight per client was served once per 200 ms loop. The in-flight limit is gone, and up to 32 requests are handled per tick.
- A stale-reply race: a reply could be older than a change already written. The writer now follows a reply with the newest state.
- Two instances could delete each other's socket, and a non-socket file at the path could be deleted. A lock file now decides ownership, a non-socket file is left alone, and shutdown removes only the socket this process bound.
- The socket was briefly looser than 0600: it is now bound under a temporary name and chmod'd before the rename.
- Remote commands skipped the offline and disabled checks. They now go through `cmd_gate`.
- Remote `set shuffle` and `set loop` did not show the key-press animation. They do now.
- A non-finite position (NaN, infinity) was reported as-is. It is now 0.
- With no config dir, the error was odd. It now reads `remote control: no config directory`.
- A flaky ownership test was rewritten; it passed 30 of 30 runs.
- The helper rebuilt the Shell submenu every second and capitalized shell names. It rebuilds only on change and shows names as the app sends them.
- Many tests were added.

Not changed: none of note.

### Verified live

Checked by the implementing session (Opus) after the review fixes, in a pty with a real termusic, real `mpv` and `yt-dlp`, and a script that drove the socket; the helper check ran the real `TermusicBar` binary against the real app.

- The socket was created with mode 0600 and removed after quit.
- A state line arrived on connect. State followed playback (title, loaded, position advancing).
- `play_pause` paused and resumed mpv. `set volume` reached mpv.
- `set shell` changed the screen. `set hiss` started the hiss mpv.
- Garbage input got error lines, and the connection stayed open.
- `next` changed the track.
- No mpv was left running after quit.
- The helper, run against the real app: it connected (one more Unix peer on the app), stayed alive when termusic quit, reconnected when termusic started again, and left nothing running.

### Not verified

- How the menu looks: layout, wording, the dimmed title. Only the author can judge this.
- Slider feel and the 10 per second throttle.
- Clicking each menu item. The encoders are checked by `--selftest` and the Rust parser tests, not by clicking.
- A second OS user trying to connect. The socket is 0600 and the directory is 0700; this was not tried with another user.
- Swift Testing under Xcode: `swift test` was not run successfully here.

### Items for the author to review

- The menu layout and wording.
- Whether 32 remote messages per tick and the 10 per second slider rate feel right: `REMOTE_PER_TICK` in `src/app.rs` and `throttled` in `helper/Sources/TermusicBar/main.swift`.
- The README protocol section, which is the public contract.

Found while reading the code for this report (not changed):

- **Requests can queue without limit.** The request channel into the app (`tx`, in `src/app.rs`) is unbounded. `REMOTE_PER_TICK` limits how fast requests are handled, not how many wait. A client that sends faster than the UI handles them grows the queue.
- **Fixed after the docs were written: a client that stops reading kept its slot.** After a write timeout the writer ended but the reader stayed open, so an idle client held one of the 4 slots indefinitely. The writer now ends the reader whenever it exits; test `a_write_timeout_frees_the_slot_of_an_idle_client` (fails without the fix, passes with it).
- **`disabled` is checked only for `cmd` requests.** `set shuffle`, `set loop` and `set volume` do not check whether the matching key is disabled in `commands/`. No command is disabled today (`ENABLED` is true everywhere), so this does not show yet. It would matter if a command were ever disabled.
- **`seek` accepts `5.0`.** The parser accepts any whole-valued number for `secs`, so `{"cmd":"seek","secs":5.0}` works, while `{"set":"volume","value":40.0}` is rejected. The README says send integers; the code is looser for `seek`.
- **The spec's root `.gitignore` entry was not added.** `helper/.gitignore` already ignores `.build/`, so `helper/.build` stays out of git. The root `.gitignore` was left alone, as asked.
- **The spec's test-mode rule differs.** The spec says the socket is disabled under `cfg(test)` for `App::blank()`. The code does not start the socket in `App::blank()`, and the tests call `serve_remote` themselves. The effect for tests is the same.

## Items for you to review

### Decide
1. **Meter row width.** You approved 36 columns for milestone 2. Milestone 3 then set the key row to 34 to line up with the cassette, and the meter row is 34 now. Keep 34 so the rows align, or widen to 36 as the spec draws it.
2. **Push main.** Main is merged and tested, and has not been pushed.

### Look at in your own terminal
- **Cover art:** the look in each shell, and the crisp image in Ghostty, which nobody has seen. Turn it on in Settings with Cover art.
- **The menu bar helper:** its menu, sliders and clicks. Build and run it with:
  ```sh
  cd helper && swift build -c release && cd .. && helper/.build/release/TermusicBar &
  ```
  Start termusic as well.
- **Motion:** the key press, the tape swap, the reels, and the meters at a few volumes.
- **Hiss:** how it sounds, and whether the default of 30 % suits you. The constants to tune it are at the top of `src/player.rs`.
- **Milestone 1 glyphs:** `⤮`, `▮▮`, `◀◀` and `▶▶` in your font, and the Pocket and Compact layouts.

### Decided without you
- Milestone 4 was built without the short design-approval plan, because you said to decide from the documents.
- The milestone 3 and 4 items are at the end of their own sections.

### Milestone 1 calls, carried over

Design calls made without you:
1. **The cassette was redrawn, and your 13-line cassette art was removed.** The new one is a front-facing tape (34 x 12) built around a paper label and a reel window, because the label has to hold the cover art in milestone 3 and the reels have to show the tape winding. Alternative I rejected: keeping your art and squeezing a label into it. Your art is still in git history (`git show e7bd768:src/theme.rs`); it can come back as an alternate look.
2. **The big TERMUSIC logo is gone.** It is 85 columns wide and no layout has room for it beside the Deck. The name is now in the frame's top-left corner. Alternative: show it as a startup splash for a second.
3. **Shell colors.** TPS-L2 (default): navy body, silver frame, orange accent, cream label. Sunset tape: your previous pink/orange/purple, refined. Amber hi-fi: brown, cream, amber. I checked all three in color screenshots, not in your terminal. Look at each (Settings, `S`, then the Shell row) and tell me what to change.
4. **Two caps for one key.** Play and Pause are separate caps that both answer to `p`, like a real deck; the one that is "down" shows the state.
5. **History rows show only the resume time.** The `youtu.be/...` link moved to the status row under the keys when a History row is selected, because it left about 8 columns for the title.
6. **The result count moved** from the tab strip to the search row ("30 found").
7. **Esc in the search field no longer quits** when there are no results; it only leaves the field. `q` (outside the field) and Ctrl-C quit.
8. **The two-step login hint is gone.** The Lists tab shows the sign-in prompt directly; Enter signs in.
9. **Search focus rule.** After Enter the field keeps focus until the results for that exact query arrive; if you kept typing, focus stays with you. No other background reply can move focus. (My first version of this rule was wrong and the reviewer caught it.)

Needs your eyes in your own terminal:
- The shuffle glyph `⤮` showed as a missing-character box in my screenshot font (Menlo). It is the glyph the app already used, so it may be fine in yours.
- The pause cap `▮▮`: in some fonts the two bars touch. `❚❚` is the fallback.
- `◀◀` and `▶▶` on the key caps: some terminals draw these wider than one column each, which would push the cap's right edge out.
- The Pocket layout (a narrow, tall window, under 76 columns and at least 30 rows) and the Compact layout (anything smaller) were only checked by render tests.

Not verified:
- Real Google sign-in and playlists in the new Lists tab (needs your account).
- Right-to-left titles in a real terminal.

Known and left alone:
- Pressing `/` keeps the previous query in the field instead of clearing it, so a new search means deleting the old text first. Easy to change to "select all" or "clear" if you prefer.
- With a signed-in account, every visit to the Lists tab refetches playlists (also when tabbing past it).
