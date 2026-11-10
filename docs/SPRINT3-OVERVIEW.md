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

Status: in progress

- A Haiku agent writes the plan into this file (goal, decision tables, milestones, roles), with a pointer from `SPRINT.md`, since `CLAUDE.md` says planning lives there and the plan file is outside the repo.
- This file matches the `.gitignore` pattern used by its predecessors. Check it is tracked (force-add if needed, as SPRINT2 was).
- The file ends with an empty "Items for you to review" section.

Verification: the file is in the repo and tracked.

### 1. Deck layout, shells, tabs

Status: planned

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

Nothing yet. Each milestone adds a section here with code snippets when it lands.

## Items for you to review

Nothing yet. Filled in as the work proceeds.
