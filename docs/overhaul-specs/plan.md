# termusic graphical overhaul: "The Deck"

## Context

termusic works (search, queue, playback, crossfade, history, settings) but looks like a generic TUI: a logo, one full-screen list at a time, a one-line status bar, and a small cassette drawing. The goal is a distinctive, easy to use player that fully commits to a Sony Walkman / cassette look, with animation, optional pixel-art thumbnails on image-capable terminals (Ghostty, kitty), and a Swift menu bar helper that controls the running app.

Decisions made with the user on 2026-11-10:

| Topic | Decision |
|---|---|
| Layout | **The Deck**: one Walkman frame. Left: cassette window, tape counter + progress, transport keys. Right: the tracklist as the tape's J-card. |
| Navigation | **Tabs on the J-card**: Results, Queue, Playlists, History (`Tab`/`Shift-Tab`, `1`-`4`); `/` focuses a search field at the top of the pane; Settings and Help open over the right pane only. |
| Palette | **Swappable shells**: named color roles; ship TPS-L2 (default), Sunset tape, Amber hi-fi; chosen in Settings. |
| Thumbnails | **Pixel art, crisp**, behind a setting (default off), shown **on the cassette label** for the playing track. Image protocol where available, half-blocks elsewhere. |
| Animations | All four: winding tape, pressable transport keys, tape swap + rolling counter on track change, **real** level meters (never faked). One setting turns animation off. |
| Menu bar helper | **Remote + settings**: now playing, play/pause, prev, next, volume, loop, shuffle, and the app's settings. No media keys / Now Playing yet. |
| Order | **Look first**: 1 Deck + shells + tabs, 2 animations, 3 thumbnails, 4 control socket + helper. |

Added by the user when reviewing this plan:

| Topic | Decision |
|---|---|
| Cassette hiss | A toggle in Settings that adds tape hiss under the music, plus a slider for how much. |
| Roles | **Opus** is the main coding model, with **Fable** (main session) as designer and advisor. **Sonnet** covers edge cases and tests. **Haiku** documents. A fresh Opus agent reviews each milestone. |
| Documentation | A Haiku agent writes the plan into the repo. Every piece Opus and Fable build is written up, with code snippets, in `docs/SPRINT3-OVERVIEW.md`. |
| Mode | **Run in auto mode**: no stops for approval. Where the plan says "design checkpoint", Fable makes the call, keeps going, and lists the call for the user at the bottom of `docs/SPRINT3-OVERVIEW.md` under "Items for you to review". |

Working agreement: design decisions are joint, but asynchronous in this run. Fable makes each visual choice so the work is not blocked, records it with a render and the alternative it rejected, and the user confirms or reverses it from the review section afterwards. Every such choice is kept cheap to reverse (colors and art are data in `theme.rs`, not logic).

## Facts established

- Frame loop: `run_tui` in `src/main.rs` draws, then `event::poll(200ms)`. Animation needs a faster tick.
- Rendering is one file, `src/ui.rs` (780 lines), reading `App`; colors are loose constants in `src/theme.rs`.
- `App` has `Screen` (Login, Search, Results, Playlists, Help, Settings, History, Offline) plus an overlay `stack`; the queue is never shown as a list (only a `next:` line).
- No control socket exists. `Cmd` in `src/cmd.rs` is the intended contract; nothing listens. `Cmd` does not cover settings.
- `ratatui-image` exists (kitty, sixel, iterm2, half-blocks); its newest release is `12.0.0-rc.3`. Which version pairs with our `ratatui 0.30.2` is **not yet checked**.
- mpv's audio levels are **not yet proven** readable (planned: `astats` filter metadata over IPC).
- Swift 6.4 is installed.

## Step 0 (on approval)

- A Haiku agent is given this plan and writes it into the repo as the opening of `docs/SPRINT3-OVERVIEW.md` (goal, decisions tables, milestones, roles), with a pointer from `SPRINT.md`, since `CLAUDE.md` says planning lives there and this plan file is outside the repo. `docs/SPRINT3-OVERVIEW.md` is listed in `.gitignore` patterns like its predecessors, so check it is tracked (force-add if needed, as SPRINT2 was).
- The file ends with an empty **"Items for you to review"** section that is filled in as the work proceeds.

## Milestone 1: Deck layout, shells, tabs

Branch `overhaul-1-deck` off `main`. Three separate commits, in this order, so each is reviewable and the existing render tests guard the first:
1. Shell color roles, with no visual change (current colors become the "Sunset tape" shell until the checkpoint).
2. Tabs and the Queue list.
3. The Deck layout.

**Theme (`src/theme.rs`)**
- Replace loose color constants with `struct Shell { name, body, frame, text, dim, accent, label, label_ink, meter_lo, meter_hi, select_bg }` and three constants: `TPS_L2`, `SUNSET`, `AMBER`. Style helpers (`title`, `selected`, `dim`, `panel`) take `&Shell`.
- `Settings` gains `shell: String` (default `"tps-l2"`, unknown name falls back to default) and a Settings row to cycle it, following the existing `Row` enum pattern in `src/app.rs`.

**State (`src/app.rs`)**
- Least churn: keep the `Screen` enum and the overlay `stack`. Add `Screen::Queue`; the four tabs are the subset Results, Queue, Playlists, History. `Screen::Search` turns into a focus flag on the Results tab (the search field at the top of the pane). Help and Settings stay overlays, now drawn over the right pane only. Login and Offline are drawn in the right pane (Offline still blocks network actions and retries on `r`).
- Existing tests that assert on `a.screen` are migrated to the new shape, not deleted.
- Per-tab selection/offset (today one shared `selected`/`offset` with `saved_sel`); the "list follows the playing track" behaviour stays for Results.
- Queue tab lists `queue.current()` + `queue.upcoming()`; Enter jumps to that track. Reordering is out of scope.
- Keys: existing ones keep working (`/`, `P`/F2, `H`/F4, `S`/F3, `?`/F1 map to the tab or overlay). New: `Tab`/`Shift-Tab`, `1`-`4` (only when the list has focus). These are not `commands/` files (those carry `KEY: char` and Tab is not a char); they go in the Help "Keys" table in `help_lines`, next to Esc and F1-F4 today. A new `commands/queue_tab.rs`-style file is added only if a letter key is introduced.

**Rendering: split `src/ui.rs` into `src/ui/`**
- `mod.rs` (frame, responsive choice), `deck.rs` (cassette window, counter/progress, transport keys), `jcard.rs` (tab strip, search field, lists), `overlay.rs` (help, settings), reusing `fit`, `bar`, `mmss`, `draw_list`, `help_lines`, `history_row`.
- All deck drawing is pure functions from plain inputs (shell, progress, frame number, flags) to `Vec<Line>`, like today's `cassette_lines`, so they can be unit tested without a terminal.
- Responsive tiers, checked by render tests. **80x24 is the primary size and must show the side-by-side Deck** (the approved mock is about 60 columns wide). Narrower than the Deck's minimum: deck above the list ("pocket"). Too small for a deck at all: list plus today's one-line bar (`draw_bar` kept for this). Exact breakpoints fall out of the cassette size settled at checkpoint 2.

**Design checkpoints with the user (before the code is finalized)**
1. Exact colors for each shell, shown as real renders.
2. The cassette, decided together:
   - Evolve the user's current 13-line `theme::CASSETTE` art or redraw it for the deck.
   - Label footprint and art grid size. This must be settled here, not in M3: a label of W columns by H rows gives only W x 2H half-block pixels, so the cassette has to be drawn around a label big enough to read as cover art in the fallback path.
   - Whether the big TERMUSIC logo survives somewhere (startup, empty state) or is dropped in favour of the frame title.
3. Transport key caps and the tab strip look.

## Milestone 2: Animations

Branch `overhaul-2-motion`.

- **Clock**: `run_tui` polls at ~50 ms while something is moving (playing, key press, swap) and animations are on; otherwise stays at 200 ms. `App` gets a small `Anim { frame, pressed: Option<(Cmd, Instant)>, swap: Option<Instant> }`. No timers in `ui`; it reads `Anim`.
- **Winding tape**: reel pack sizes derive from `position / duration` (left shrinks, right grows); hubs rotate per frame; short fast spin after seek/skip. Unknown duration (live streams): equal reels, slow spin.
- **Pressable keys**: a key cap draws "down" for ~150 ms after its `Cmd` is dispatched, and stays down while its mode is on (pause, loop, shuffle).
- **Tape swap + counter**: on `Event::TrackStarted`, a ~500 ms door/eject/insert sequence; a three-digit counter rolls with position.
- **Real level meters** (spike first, on its own):
  - Spike: start mpv with an `astats` audio filter that publishes metadata, read it over IPC, confirm values move with the audio, measure CPU, and confirm crossfade and preload still work with the filter on both mpv instances.
  - If it works: read levels ~10 times a second on a background thread (never on the UI thread; today's poll has a 30 ms worst case), hand the latest L/R pair to `App` over the existing `Msg` channel, draw with peak hold.
  - If it does not work: stop and decide with the user. No simulated meters.
- Setting `animations: bool` (default on); off means static art and the 200 ms loop.

**Cassette hiss** (same milestone, it shares the mpv work with the meters)
- Settings: `hiss: bool` (default off) and `hiss_level: u8` 0-100 (default 30), as two rows on the Settings screen: a toggle and a slider using the same left/right handling as the Volume row.
- Approach to spike first, in this order of preference:
  1. A separate tiny mpv playing generated pink/brown noise in a loop, with its own IPC socket and watchdog (reusing `Player::spawn`'s pattern). Its volume is `hiss_level` scaled by the user volume; it pauses and resumes with playback and stops when nothing is playing. This keeps hiss constant through a crossfade, where two music mpvs overlap.
  2. If a generated-noise source does not work in mpv here, a noise filter mixed into each music mpv's audio chain, with the level set over IPC.
- The hiss follows the real pause state from the poll (the bug fixed today), so it never plays over silence.
- The level meters measure the music only, not the hiss.
- If neither approach works, stop that item and list it for review; no bundled audio file is added without asking.

**Design checkpoint (decided by Fable in auto mode, listed for review)**: storyboard of the swap sequence and the reel frames; the character of the hiss (pink vs brown noise, default level).

## Milestone 3: Pixel-art thumbnails

Branch `overhaul-3-art`.

- Setting `thumbnails: bool`, default **off**. Update `CLAUDE.md`: the "text-only, no thumbnails" convention becomes "thumbnails are optional, off by default, pixel-art only".
- Fetch `https://i.ytimg.com/vi/<id>/mqdefault.jpg` with the existing `reqwest` client in a tokio task; result returns as a `Msg`. Only the playing track and the preloaded next track; small in-memory cache keyed by id. A failed fetch leaves the plain label; no status-bar noise.
- `src/art.rs`, pure and unit tested: decode (new dependency `image`, JPEG only), center-crop, shrink to a small grid, reduce to the active shell's colors with ordered dithering. Output: a grid of colors.
- Draw: half-blocks by our own small function (works everywhere, no dependency). Crisp path: the same grid scaled up with hard edges and sent through `ratatui-image` when the terminal reports kitty graphics support. First task of the milestone: confirm a `ratatui-image` version that builds against `ratatui 0.30.2`; if none does, ship half-blocks and raise the choice with the user.
- Changing shell re-renders the art in the new colors.
- During the tape swap animation the art is hidden until the new tape is seated (protocol images are not cells and cannot slide with the drawing).

**Design checkpoint**: dithering strength and color mapping, compared side by side on real thumbnails in Ghostty (grid size and label footprint were fixed in M1).

## Milestone 4: Control socket and menu bar helper

Branch `overhaul-4-helper`.

- The sprint notes mention a menu bar widget already on the user's machine whose skip button fails; its source is not in the repo. In auto mode the helper is built fresh in `helper/`, and "reuse or replace the old widget" goes in the review section.
- **Socket (`src/remote.rs`)**: tokio `UnixListener` at `<config dir>/termusic.sock`, mode 0600, removed on exit. At startup an existing file is tested by trying to connect: if something answers, another termusic is running and its socket is left alone (this instance runs without remote control and says so in the status bar); only a dead file is removed. Newline-delimited JSON.
  - In: `Cmd` (add serde derives), `set` for a setting (volume, shell, thumbnails, animations, hiss, hiss level, loop, shuffle), `get`.
  - Out: a state snapshot on connect and on change (track, paused, position, duration, volume, loop, shuffle, settings).
  - Incoming messages become a `Msg` on the existing channel and go through `App::dispatch` / the same settings code the Settings screen uses, so there is one code path.
  - Input is validated: unknown commands and out-of-range values are rejected with an error line, never applied.
- **Helper (`helper/`)**: a SwiftPM executable (no Xcode project), `NSStatusItem` showing the track title; menu with transport, volume slider, loop, shuffle, settings. Reconnects in the background and greys out when the socket is absent. It never plays audio and never starts termusic.
- Document build and launch in `README.md`.

## Process for every milestone

1. Fable designs the visuals (art, colors, layout, motion) as concrete specs and renders, and makes the checkpoint calls.
2. An Opus agent implements the milestone from that spec, consulting Fable; a Sonnet agent then adds tests for every new pure function and for edge cases (tiny terminals, unknown duration, missing art, socket errors).
3. `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
4. A fresh Opus agent reviews; fixes go back until the verdict is "ship".
5. Live check against real mpv.
6. A Haiku agent adds the milestone's section to `docs/SPRINT3-OVERVIEW.md`: what was built, with short code snippets taken from the real diff (it is given the diff and the implementer's report, and told to quote code only from them), what was verified and how, and what was not.
7. Fable appends the milestone's entries to "Items for you to review": each design call made without the user (with the rejected alternative), anything not verified, anything that needs the user's eyes or ears (Ghostty art, hiss by ear, menu bar item), and any spike that failed.
8. Update `SPRINT.md` and `CLAUDE.md` (architecture section changes in M1 and M4; the thumbnails convention in M3), commit, fast-forward into `main`. No push unless asked.

Auto mode limits: the run does not stop for approval, but it still does not push, does not delete user files, and does not invent results. A spike that fails (meters, hiss, image crate) ends that item with a note in the review section instead of a fake.

## Verification

- **Unit / render tests**: the existing `render(&mut app, w, h)` test helper with ratatui's test backend, at 60x16, 80x24, 110x32, 160x45, for each tab, overlay, shell and tier; "does not panic at 1x1".
- **Screenshots**: `scripts/capture.py` (existing pty capture) for each design checkpoint.
- **Live pty check**: extend the scratch driver used for the pause/loop fixes (real mpv at low volume): tabs switch, queue tab matches playback, animation setting off stops redraws, no mpv left after quit.
- **Thumbnails**: by eye in Ghostty (crisp path) and in a terminal without image support (half-blocks). This needs the user.
- **Meters**: IPC read-back compared against a silent and a loud source.
- **Hiss**: process and IPC checks (running only while music plays, volume follows the slider, paused with playback, gone after quit). How it sounds needs the user's ears.
- **Helper**: unit tests for the JSON protocol on the Rust side with a `UnixStream`; manual check of the menu bar item with the app running, stopped, and restarted.

## Out of scope

Media keys and macOS Now Playing, art on every list row, queue reordering, mouse support, more than three shells.
