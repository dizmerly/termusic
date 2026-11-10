# Milestone 1 design spec: The Deck

Designer: Fable (main session). Implementer: Opus. This is the visual and behavioural spec for
milestone 1 of the approved plan (/Users/dizmerly/.claude/plans/cached-toasting-peach.md).
Where this spec and the plan differ, this spec wins. Where the spec is silent, choose the
simplest thing that matches the mock and say what you chose.

## 1. The frame at 80x24 (primary size)

One rounded frame fills the terminal. Inside: the deck pane (fixed 36 columns), a vertical
divider, the J-card pane (the rest: 41 columns at 80 wide).

```
╭─ TERMUSIC ──────────────────────────────────────── ⤮ ↻1 ── VOL ▮▮▮▮▮▮▮▯▯▯ 70% ─╮
│                                    │ 1 RESULTS   2 QUEUE   3 LISTS   4 HISTORY │
│ ╭────────────────────────────────╮ │ ♪ never gonna give▏                      │
│ │ ┌────────────────────────────┐ │ │──────────────────────────────────────────│
│ │ │██████████ A ────────────── │ │ │ 01 Never Gonna Give You U…   Rick Astley │
│ │ │██████████ Never Gonna Give │ │ │▸02 Take On Me                       a-ha │
│ │ │██████████ You Up           │ │ │ 03 Africa                           Toto │
│ │ │██████████ Rick Astley      │ │ │ 04 Sweet Dreams               Eurythmics │
│ │ │██████████ ──────────── C90 │ │ │ 05 Everybody Wants To Rul… Tears For Fe… │
│ │ └────────────────────────────┘ │ │ 06 Don't You (Forget Abou…  Simple Minds │
│ │     ╭────────────────────╮     │ │ 07 Tainted Love                Soft Cell │
│ │     │▒▓▓(◐)▓▓▒    ▓(◓)▓   │     │ │ 08 Blue Monday                 New Order │
│ │     ╰────────────────────╯     │ │                                          │
│ ╰─◦──────────▔▔▔▔▔▔▔▔──────────◦─╯ │                                          │
│                                    │                                          │
│  012  ━━━━━━━━━●──────────  1:12   │                                          │
│                                    │                                          │
│                                    │                                          │
│   ┌───┐┌───┐┌───┐┌───┐┌───┐┌───┐   │                                          │
│   │◀◀ ││ ▶ ││▮▮ ││▶▶ ││ ⤮ ││ ↻ │   │                                          │
│   └─b─┘└─p─┘└─p─┘└─n─┘└─s─┘└─l─┘   │                                          │
│                                    │                                          │
│  queued: Take On Me                │                                          │
╰─ / search · Tab next tab · ? help ───────────────────────────────── TPS-L2 ─╯
```

The mock is a sketch: column counts in the prose below are authoritative, not the drawing.

### Top border
`TERMUSIC` at the left in the accent color, bold. At the right: the shuffle icon and loop icon
when those modes are on (same icons as today), then `VOL` + a 10-cell bar + percent. This
replaces `draw_volume` (the overlay on row 0) and the icons in the old bar.

### Bottom border
Left: key hints for the current context (see section 4). Right: the active shell's display
name. Hints are cut with `fit` so they never collide with the shell name.

### Deck pane (36 columns), top to bottom
- Cassette, 34 columns by 12 rows, centered horizontally.
- Blank row.
- Counter and progress row: a three-digit tape counter (`position_secs % 1000`, zero padded),
  a progress line (`━` played, `●` head, `─` remaining), then `m:ss` elapsed. With no duration
  known: counter and elapsed only, the line all `─`. Nothing playing: `000` and an empty line.
- One row reserved for level meters (milestone 2). Leave it blank now.
- Transport keys: six caps, each 5 columns by 3 rows, no gap, centered (30 columns).
- Status row at the bottom of the pane: `app.status` (and the busy indicator), cut to width.
  This replaces the status text in the old one-line bar.
When the pane is taller than needed, put the extra blank rows between the blocks evenly
(simple: center the whole stack vertically, status row pinned to the bottom).

### Cassette (34 x 12), rows
1. `╭` + 32 `─` + `╮`
2. `│ ┌` + 28 `─` + `┐ │`
3-7. Label, five rows: `│ │` + 28 label columns + `│ │`
8. `│ └` + 28 `─` + `┘ │`
9. `│` + 5 spaces + `╭` + 20 `─` + `╮` + 5 spaces + `│`
10. `│` + 5 spaces + `│` + 20 window columns + `│` + 5 spaces + `│`
11. `│` + 5 spaces + `╰` + 20 `─` + `╯` + 5 spaces + `│`
12. `╰─◦` + 10 `─` + 8 `▔` + 10 `─` + `◦─╯`

**Label (28 x 5)**, drawn as paper: background `shell.label`, text `shell.label_ink`.
- Art area: the leftmost 10 columns by 5 rows are reserved for the thumbnail (milestone 3;
  10 x 5 cells is 10 x 10 half-block pixels). In this milestone there is no art, so the text
  uses the full width with one column of margin on each side. Build the label as a function
  that takes `art: bool` so milestone 3 only has to pass `true` and draw into the 10 x 5 rect.
  Add a test for the `art = true` layout now (text starts at column 11).
- Row 1: `A` then a rule (`─`) to the right edge.
- Rows 2-3: the track title, word-wrapped over two rows, cut with `…` if longer.
- Row 4: the channel, dimmer ink.
- Row 5: a rule ending in `C90` at the right.
- Nothing playing: rows 2-3 read `NO TAPE` / `press / to search`, row 4 empty.

**Window (20 columns)**: two reel cells of 9 columns with a 2-column gap. A reel cell is the
hub `(X)` (3 columns) centered, with tape pack on both sides, up to 3 cells per side. Pack level
is 0..=6: `level / 2` full cells `▓` nearest the hub, plus one `▒` on the outside when the level
is odd, rest spaces. Left level = `round(6 * (1 - progress))`, right = `round(6 * progress)`,
progress = `position / duration` (unknown duration: 3 and 3; nothing playing: 6 and 0).
`X` is a frame from `theme::REEL_FRAMES`; keep today's rule (frame = whole seconds of
position while playing, frame 0 when paused or idle). Milestone 2 makes it smooth.
Window background is the darkest tone (`shell.body`), pack in `shell.dim`, hubs in `shell.accent`.
Cassette body lines are `shell.frame`.

Make this a pure function, e.g. `deck::cassette(&Shell, &CassetteView) -> Vec<Line<'static>>`
where `CassetteView { title, channel, progress: Option<f64>, spin: usize, playing: bool,
art: bool }`. Same for the keys and the counter row. No `App` in their signatures.

### Transport keys
Six caps in this order, each:
```
┌───┐
│ ▶ │
└─p─┘
```
| Cap | Glyph | Key letter on the bottom edge | Lit (accent, bold) when |
|---|---|---|---|
| Previous | `◀◀` | `b` | never (milestone 2 flashes it) |
| Play | `▶` | `p` | a track is playing |
| Pause | `▮▮` | `p` | a track is loaded and paused |
| Next | `▶▶` | `n` | never |
| Shuffle | `⤮` | `s` | shuffle is on |
| Loop | `↻` (`↻1` on loop one) | `l` | loop is not off |
Unlit caps are `shell.frame` with `shell.text` glyphs. Take the key letters from the
`commands::*::KEY` constants, not literals, so the caps stay in sync with the commands.
Glyph cells: every cap interior must be exactly 3 display columns (use `unicode-width`).

### J-card pane
- Row 1, tab strip: `1 RESULTS`, `2 QUEUE`, `3 LISTS`, `4 HISTORY`, evenly spread. The active
  tab is reversed (background `shell.accent`, text `shell.body`, bold); the others `shell.dim`.
  Results shows its count when it has results: `1 RESULTS 30`.
- Row 2, search field, shown on every tab: when focused `♪ ` + the input + a cursor `▏` in
  the accent color; when not focused and empty, `♪ / to search` dim; when not focused with
  text, the last query in `shell.text`.
- Row 3: a rule.
- Rows 4+: the list for the active tab. Row format: marker column (`▸` for the playing
  track in the accent color, else space), a two-digit index, the title (cut with `…`), and
  the right column right-aligned in dim (channel; on History the existing `youtu.be/<id> mm:ss`).
  Selected row: background `shell.select_bg`, bold. Reuse `draw_list`, `fit`, `history_row`.
- Empty states, centered and dim, one or two lines: Results: `Type / and search for a song`;
  Queue: `Nothing queued. Enter on a result plays it.`; Lists when signed out: the existing
  sign-in message and keys; History: `Nothing played yet`.
- Help and Settings overlays draw over the J-card pane only (a `theme::panel` filling the
  pane), keeping their current content and scrolling. Login and Offline messages also draw
  in the J-card pane. The deck stays visible and live behind all of them.

## 2. Responsive tiers

| Tier | When | What |
|---|---|---|
| Deck | width >= 76 and height >= 22 | Section 1. Deck pane stays 36 columns; the J-card takes all extra width. |
| Pocket | not Deck, width >= 38 and height >= 30 | Cassette, counter row and keys stacked at the top (centered), J-card below with at least 10 rows. Same frame borders. |
| Compact | everything else | The J-card fills the screen (no frame), with the existing one-line `draw_bar` at the bottom. |

Nothing may panic at any size, including 1x1 and 0 height. Keep and extend the existing
"tiny sizes do not panic" tests. 80x24 must render the Deck tier: assert it in a test.

## 3. Shells (colors)

`theme::Shell` holds the roles below as `ratatui::style::Color::Rgb`. The whole frame area is
filled with `body` as background, in every tier.

| Role | TPS-L2 (default) | Sunset tape | Amber hi-fi |
|---|---|---|---|
| display name | `TPS-L2` | `SUNSET TAPE` | `AMBER HI-FI` |
| settings id | `tps-l2` | `sunset` | `amber` |
| body (background) | #16284F | #1A1026 | #241812 |
| frame (borders, cassette shell) | #B9C2D3 | #FF69B4 | #C9A27A |
| text | #E8ECF4 | #FFE6F4 | #F3E3C3 |
| dim | #7F8FB3 | #9A7FBD | #94795C |
| accent | #FF7A1A | #FFAA64 | #FFB000 |
| label (paper) | #F2E8CF | #FFE078 | #EADBB8 |
| label_ink | #26221C | #3A2A5C | #3B2A1A |
| meter_lo | #7FD1B9 | #64DCFF | #5CFFB0 |
| meter_hi | #FF4D3D | #FF4D8D | #FF6A3D |
| select_bg | #2A4687 | #3A2A5C | #4A3423 |

- `Settings.shell: String` (serde default `"tps-l2"`; an unknown id falls back to the default
  shell, it is not an error and the file is not rewritten because of it).
- A new Settings row "Shell" (the existing `Row` enum pattern): left/right cycles through the
  three, applies immediately, and is saved through the existing background save path.
- Every color in `ui` comes from the active shell. Remove the old loose color constants once
  nothing uses them.

## 4. Navigation and keys

State: keep `Screen` and the overlay `stack`. Add `Screen::Queue`. The four tabs are
Results, Queue, Playlists, History. `Screen::Search` goes away: searching is a focus flag
(`app.searching` or similar) while the active tab stays what it was; pressing Enter runs the
search and, when results arrive, switches to the Results tab with focus on the list.
Migrate the existing tests that use `Screen::Search`; do not delete their coverage.

| Key | Where | Does |
|---|---|---|
| `/` | list focus, any tab or overlay | focus the search field (closes overlays) |
| `Esc` | search focus | leave the field, keep the text. If there is nothing behind it (no results, empty queue, startup), Esc still just leaves the field; it no longer quits |
| `Esc` | overlay | close it (existing behaviour) |
| `Tab` / `Shift-Tab` | list focus | next / previous tab, wrapping |
| `Tab` | search focus | leave the field (existing behaviour) |
| `1` `2` `3` `4` | list focus | jump to that tab |
| `P`/`F2`, `H`/`F4` | as today | open the Lists / History tab (now tabs, not overlays) |
| `S`/`F3`, `?`/`F1` | as today | Settings / Help overlays |
| `q` | list focus or overlay | quit (as today). In search focus `q` is a letter; `Ctrl-C` always quits |
| `Enter` | Queue tab | jump to the selected track: it becomes current, tracks before it move to the played side, the rest stay upcoming. Add the method to `Queue` with tests; do not rebuild the queue from scratch |
| everything else | | unchanged (`j`/`k`, PgUp/PgDn, `g`/`G`, playback commands) |

- App starts with focus in the search field on the Results tab (as today, where startup is
  the Search screen).
- Each tab remembers its own selection and scroll offset. The Results list still follows the
  playing track as it does today. The Queue tab lists the current track first (marked as
  playing) then the upcoming tracks.
- The Playlists tab keeps the login rule: signed out shows the sign-in prompt in the pane
  and Enter starts login (today's Login screen behaviour, now inside the pane).
- Bottom-border hints by context: search focus `Enter search · Esc back · F1 help`;
  list focus `/ search · Tab next tab · Enter play · ? help`; overlays `Esc back`.
- Help: add `Tab`, `Shift-Tab` and `1`-`4` to the Keys table in `help_lines` (they are not
  `commands/` files: those carry `KEY: char`). Keep README's key table in sync.

## 5. Removed

- The big ASCII `LOGO` and its gradient: no tier has room for it next to the Deck. Remove it
  and its test. (Listed for the user's review; git history keeps it.)
- The old `theme::CASSETTE` art, `cassette_above`, `draw_next` (the Queue tab replaces the
  `next:` line) and `draw_volume`. Migrate or replace their tests; do not leave dead code.
  (Also listed for the user's review.)

## 6. Code shape

- `src/ui.rs` becomes `src/ui/mod.rs` (frame, tier choice), `deck.rs`, `jcard.rs`,
  `overlay.rs`. `ui` still only reads `App` (the existing exception for page size stays).
- Three commits on branch `overhaul-1-deck`, each passing fmt, clippy and tests:
  1. `Shell` color roles, old look unchanged (current colors mapped onto the Sunset shell
     as closely as the roles allow), the `shell` setting and Settings row.
  2. Tabs, focus flag, `Screen::Queue`, per-tab selection, the Queue jump.
  3. The Deck, Pocket and Compact rendering, the three final shells with TPS-L2 as default,
     removals from section 5.
- Add one ignored test `dump_renders` that prints the text render of: the Deck at 80x24 and
  120x36 (idle; playing at 30% with shuffle and loop-one on; Queue tab; Help overlay; Settings
  overlay; search focused), Pocket at 60x34, Compact at 60x16. The designer runs it with
  `cargo test dump_renders -- --ignored --nocapture` to check the look.
