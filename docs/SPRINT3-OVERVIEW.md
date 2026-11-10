# Sprint 3 overview: The Deck (graphical overhaul)

Plan approved 2026-11-10. termusic works (search, queue, playback, crossfade, history, settings) but looks like a generic TUI: a logo, one full-screen list at a time, a one-line status bar, and a small cassette drawing. The goal is a distinctive, easy to use player that fully commits to a Sony Walkman / cassette look, with animation, optional pixel-art thumbnails on image-capable terminals (Ghostty, kitty), and a Swift menu bar helper that controls the running app.

## Decisions

### Initial decisions (2026-11-10)

| Topic | Decision |
|---|---|
| Layout | **The Deck**: one Walkman frame. Left: cassette window, tape counter + progress, transport keys. Right: the tracklist as the tape's J-card. |
| Navigation | **Tabs on the J-card**: Results, Queue, Playlists, History (`Tab`/`Shift-Tab`, `1`-`4`); `/` focuses a search field at the top of the pane; Settings and Help open over the right pane only. |
| Palette | **Swappable shells**: named color roles; ship TPS-L2 (default), Sunset tape, Amber hi-fi; chosen in Settings. |
| Thumbnails | **Pixel art, crisp**, behind a setting (default off), shown **on the cassette label** for the playing track. Image protocol where available, half-blocks elsewhere. |
| Animations | All four: winding tape, pressable transport keys, tape swap + rolling counter on track change, **real** level meters (never faked). One setting turns animation off. |
| Menu bar helper | **Remote + settings**: now playing, play/pause, prev, next, volume, loop, shuffle, and the app's settings. No media keys / Now Playing yet. |
| Order | **Look first**: 1 Deck + shells + tabs, 2 animations, 3 thumbnails, 4 control socket + helper. |

### Added by the user when reviewing the plan

| Topic | Decision |
|---|---|
| Cassette hiss | A toggle in Settings that adds tape hiss under the music, plus a slider for how much. |
| Roles | **Opus** is the main coding model, with **Fable** (main session) as designer and advisor. **Sonnet** covers edge cases and tests. **Haiku** documents. A fresh Opus agent reviews each milestone. |
| Documentation | A Haiku agent writes the plan into the repo. Every piece Opus and Fable build is written up, with code snippets, in `docs/SPRINT3-OVERVIEW.md`. |
| Mode | **Run in auto mode**: no stops for approval. Where the plan says "design checkpoint", Fable makes the call, keeps going, and lists the call for the user at the bottom of `docs/SPRINT3-OVERVIEW.md` under "Items for you to review". |

Working agreement: design decisions are joint, but asynchronous in this run. Fable makes each visual choice so the work is not blocked, records it with a render and the alternative it rejected, and the user confirms or reverses it from the review section afterwards. Every such choice is kept cheap to reverse (colors and art are data in `theme.rs`, not logic).

## Roles and process

Roles:
- **Opus**: main coding model. Implements each milestone from Fable's spec, consulting Fable.
- **Fable** (main session): designer and advisor. Designs the visuals (art, colors, layout, motion) as concrete specs and renders, and makes the checkpoint calls.
- **Sonnet**: edge cases and tests.
- **Haiku**: documentation. Writes the plan into the repo and each milestone's section here.
- **Fresh Opus agent**: reviews each milestone.

Per-milestone process:
1. Fable designs the visuals and makes the checkpoint calls.
2. Opus implements from the spec; Sonnet adds tests for every new pure function and for edge cases (tiny terminals, unknown duration, missing art, socket errors).
3. `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
4. A fresh Opus agent reviews; fixes go back until the verdict is "ship".
5. Live check against real mpv.
6. A Haiku agent adds the milestone's section to this file: what was built, short code snippets taken from the real diff, what was verified and how, and what was not.
7. Fable appends the milestone's entries to "Items for you to review".
8. Update `SPRINT.md` and `CLAUDE.md` (architecture section in M1 and M4; thumbnails convention in M3), commit, fast-forward into `main`. No push unless asked.

Auto mode limits: the run does not stop for approval, but it still does not push, does not delete user files, and does not invent results. A spike that fails (meters, hiss, image crate) ends that item with a note in the review section instead of a fake.

## Milestones

Out of scope: media keys and macOS Now Playing, art on every list row, queue reordering, mouse support, more than three shells.

### Step 0: plan into the repo

Status: done

- A Haiku agent writes the plan into this file (goal, decision tables, milestones, roles), with a pointer from `SPRINT.md`, since `CLAUDE.md` says planning lives there and the plan file is outside the repo.
- This file matches the `.gitignore` pattern used by its predecessors. Check it is tracked (force-add if needed, as SPRINT2 was).
- The file ends with an empty "Items for you to review" section.

Verification: the file is in the repo and tracked.

### 1. Deck layout, shells, tabs

Status: done, merged into main

Branch `overhaul-1-deck` off `main`. Three commits, in order: (1) shell color roles with no visual change; (2) tabs and the Queue list; (3) the Deck layout.

What will be built:
- Theme: `struct Shell` with named color roles, and three constants `TPS_L2`, `SUNSET`, `AMBER`. Settings gains `shell` (default `tps-l2`; unknown name falls back to default) and a row to cycle it.
- State: keep the `Screen` enum and overlay stack. Add `Screen::Queue` (the queue is shown as a list for the first time). `Search` becomes a focus flag on the Results tab. Help and Settings stay overlays, drawn over the right pane only. Login and Offline are drawn in the right pane; Offline still blocks network actions and retries on `r`.
- Per-tab selection and offset. The "list follows the playing track" behaviour stays for Results.
- Queue tab lists the current and upcoming tracks; Enter jumps to that track. Reordering is out of scope.
- Keys: existing keys keep working. New: `Tab`/`Shift-Tab`, and `1`-`4` when the list has focus. They go in the Help "Keys" table.
- `src/ui.rs` is split into `src/ui/` (`mod.rs`, `deck.rs`, `jcard.rs`, `overlay.rs`). Deck drawing is pure functions from plain inputs to lines, so it can be unit tested.
- Responsive tiers: 80x24 is the primary size and must show the side-by-side Deck. Narrower: deck above the list. Too small for a deck: list plus the one-line bar. Exact breakpoints follow the cassette size settled at checkpoint 2.

Design checkpoints (listed for review, see the process above):
1. Exact colors for each shell, shown as real renders.
2. The cassette: evolve the current 13-line art or redraw it; label footprint and art grid size (fixed here, not in M3); whether the big TERMUSIC logo survives (startup, empty state) or is dropped for the frame title.
3. Transport key caps and the tab strip look.

Verification:
- Render tests with the existing `render(&mut app, w, h)` helper at 60x16, 80x24, 110x32 and 160x45, for each tab, overlay, shell and tier. "Does not panic at 1x1."
- Existing tests that assert on `a.screen` are migrated to the new shape, not deleted.
- Screenshots with `scripts/capture.py` for each design checkpoint.

### 2. Animations and cassette hiss

Status: planned

Branch `overhaul-2-motion`.

What will be built:
- Clock: the loop polls at about 50 ms while something moves (playing, key press, swap) and animations are on; otherwise it stays at 200 ms. `App` gets a small animation state. No timers in `ui`.
- Winding tape: reel sizes derive from position over duration; hubs rotate per frame; a short spin after seek or skip. Unknown duration (live streams): equal reels, slow spin.
- Pressable keys: a key cap draws "down" for about 150 ms after its command, and stays down while its mode is on (pause, loop, shuffle).
- Tape swap and counter: on track start, a sequence of about 500 ms door/eject/insert sequence; a three-digit counter rolls with position.
- Real level meters, spike first, on its own. Mpv's audio levels are **not yet proven** readable (planned: `astats` filter metadata over IPC). If the spike works: read levels about 10 times a second on a background thread, hand the latest L/R pair to `App`, draw with peak hold. If it does not work: stop and decide with the user. No simulated meters.
- Setting `animations` (default on). Off means static art and the 200 ms loop.
- Cassette hiss (same milestone, shares the mpv work). Settings: `hiss` (default off) and `hiss_level` 0-100 (default 30), as a toggle and a slider. Preferred approach, spiked first: (1) a separate tiny mpv playing generated pink or brown noise in a loop, with its own socket and watchdog; (2) if that does not work, a noise filter mixed into each music mpv. Hiss follows the real pause state, and the meters measure music only. If neither works, the item is listed for review; no bundled audio file is added without asking.

Design checkpoint (decided by Fable in auto mode, listed for review): storyboard of the swap sequence and reel frames; the character of the hiss (pink vs brown) and its default level.

Verification:
- Meters: IPC read-back compared against a silent and a loud source. The spike also checks that values move with the audio, CPU cost, and that crossfade and preload still work with the filter on both mpv instances.
- Hiss: process and IPC checks (runs only while music plays, volume follows the slider, paused with playback, gone after quit). How it sounds needs the user's ears.
- Live pty check: animation setting off stops redraws.

### 3. Pixel-art thumbnails

Status: planned

Branch `overhaul-3-art`.

What will be built:
- Setting `thumbnails`, default **off**. `CLAUDE.md` convention changes from "text-only, no thumbnails" to "thumbnails are optional, off by default, pixel-art only".
- Fetch the YouTube mqdefault thumbnail with the existing HTTP client in a tokio task, result returned as a message. Only the playing track and the preloaded next track; small in-memory cache keyed by id. A failed fetch leaves the plain label.
- `src/art.rs` (pure, unit tested): decode (new `image` dependency, JPEG only), center-crop, shrink to a small grid, reduce to the active shell's colors with ordered dithering.
- Drawing: half-blocks by our own small function (works everywhere). Crisp path: the same grid scaled up with hard edges and sent through `ratatui-image` when the terminal reports kitty graphics support. First task: confirm a `ratatui-image` version that builds against `ratatui 0.30.2`. This is **not yet checked**. If none builds, ship half-blocks and raise the choice with the user.
- Changing shell re-renders the art in the new colors. During the tape swap the art is hidden until the new tape is seated.

Design checkpoint: dithering strength and color mapping, compared side by side on real thumbnails in Ghostty.

Verification: unit tests for the pure functions; checked by eye in Ghostty (crisp path) and in a terminal without image support (half-blocks). This needs the user.

### 4. Control socket and menu bar helper

Status: planned

Branch `overhaul-4-helper`.

What will be built:
- Socket (`src/remote.rs`): a Unix listener at `<config dir>/termusic.sock`, mode 0600, removed on exit. At startup an existing file is tested by connecting. If something answers, another termusic is running; its socket is left alone and this instance runs without remote control, saying so in the status bar. Only a dead file is removed. Newline-delimited JSON.
  - In: commands (`Cmd` gets serde derives), `set` for a setting (volume, shell, thumbnails, animations, hiss, hiss level, loop, shuffle), and `get`.
  - Out: a state snapshot on connect and on change (track, paused, position, duration, volume, loop, shuffle, settings).
  - Incoming messages go through the same path as the keys and the Settings screen. Input is validated: unknown commands and out-of-range values are rejected with an error line, never applied.
- Helper (`helper/`): a SwiftPM executable (no Xcode project). An `NSStatusItem` shows the track title; the menu has transport, volume slider, loop, shuffle and settings. It reconnects in the background and greys out when the socket is absent. It never plays audio and never starts termusic.
- The old menu bar widget mentioned in the sprint notes is **not in the repo**, so the helper is built fresh. Whether to reuse or replace the old widget goes in the review section.
- `README.md` documents build and launch.

Verification:
- Unit tests for the JSON protocol on the Rust side, with a `UnixStream`.
- Manual check of the menu bar item with the app running, stopped, and restarted. This needs the user.

## What was built

### Milestone 1: The Deck

Commits on main, oldest first: 40a06b9 shell color roles; 3319afa tabs, search focus flag, Queue tab, per-tab selection; 78fe731 the Deck layout, Pocket and Compact tiers, TPS-L2 default; 4122fc2 design tweaks; 70f01e2 review fixes; 26c597b search focus rule.

The user sees one Walkman frame. On the left (the deck) is a cassette drawn in text: a paper label with the track title and channel, a window with two reels whose tape packs show the position, a tape counter with a progress line, and six transport keys with their key letters. On the right is the J-card: a tab strip (Results, Queue, Lists, History), a search field, and the list for the active tab. The frame has three color shells (TPS-L2, the default, Sunset tape and Amber hi-fi), chosen in Settings.

#### The screen at 80x24

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

#### How it is built

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

#### Keys

| Key | Where | Does |
|---|---|---|
| `Tab` / `Shift-Tab` | list focus | next / previous tab, wrapping |
| `1` `2` `3` `4` | list focus | jump to that tab |
| `/` | list focus | focus the search field |
| `Esc` | search focus | leave the field, keep the text (no longer quits) |
| `Enter` | Queue tab | play the selected track (Queue jump) |

Enter on any empty list does nothing. Typed letters in the search field are text, not commands.

#### Who did what

- Fable wrote the design spec (cassette 34x12, six key caps, three shells, J-card tabs, three size tiers) and reviewed renders and color screenshots.
- An Opus agent implemented the spec in three commits plus one tweak commit.
- Fable made design tweaks after the first renders: History rows show only the resume time, with the youtu.be link in the deck's status row; the result count moved from the tab strip to the search row ("30 found"); the Compact bar drops its hint, then the volume, before cutting the title; an idle cassette shows two empty reels.
- A Sonnet agent fixed the review findings and added the tests.

#### Review

A fresh Opus agent reviewed the milestone. The first verdict was 7/10, "fix first". After the fixes, the re-review gave 8.5/10, "ship".

Review findings that were fixed:
1. A playlist or search reply arriving while the user typed took focus from the search field, so typed letters ran as commands (q quit the app). Now no reply changes focus except the results of an unchanged query.
2. Enter on an empty Results tab cleared the queue and stopped playback. Enter on any empty list now does nothing.
3. Tab cycling got stuck when a tab's command was disabled.
4. `fit`/`pad` could return text wider than asked for emoji with a variation selector. Control characters in the status or search input are now replaced with spaces when drawn.
5. Selection is clamped when an overlay closes.

One instruction from Fable was wrong and was reversed after the re-review: dropping search focus the moment Enter is pressed. It made keys typed during a running search act as commands.

#### Verified

- `cargo fmt --check` clean, `cargo clippy --all-targets -- -D warnings` clean, `cargo test` 174 passed, 0 failed, 1 ignored (the ignored test is `dump_renders`, which prints the screens). Tests went from 135 to 174.
- The reviewer's mutation checks: 13 of 17 mutations were caught on the first pass; the 4 survivors got tests.
- Live in a pty against real mpv and yt-dlp with a scratch config dir at 5% volume: search, play, typed letters after Enter stay text, Enter on an empty tab does nothing, Queue tab jump plays the chosen track, History tab shows the link in the status row, all three shells cycle from Settings, no mpv left running after quit.
- Color screenshots of all three shells were checked by Fable.

#### Not verified

- Real Google sign-in and playlists.
- The Pocket and Compact tiers in a real terminal (render tests only).
- How glyphs such as the shuffle icon and the pause bars look in the user's own terminal font.
- Right-to-left text in a real terminal.

#### Removed

- The big ASCII TERMUSIC logo.
- The previous 13-line cassette art (`theme::CASSETTE`).
- The `next:` line (replaced by the Queue tab).
- The two-step login hint.

## Items for you to review

Things decided or left open while you were away. Each one is cheap to reverse; say the word.

### Milestone 1: The Deck

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
