# Milestone 3 design spec: pixel-art cover on the cassette label

Designer: Fable (main session). Implementer: Opus. Builds on milestones 1 and 2 (merged into
`main`). Overall plan: /Users/dizmerly/.claude/plans/cached-toasting-peach.md. Where this spec
and the plan differ, this spec wins. Where it is silent, do the simplest thing and report it.

Checked by the designer: `ratatui-image` 11.1.0 (latest stable; 12.x is release-candidate only)
declares `ratatui ^0.30.1` and `image ^0.25.6`. Not yet checked: that it builds here, and its
exact API. Read its source in `~/.cargo/registry/src/*/ratatui-image-11.1.0/` (after adding the
dependency) rather than relying on memory.

## 1. What the user sees

With the new setting on, the playing track's YouTube thumbnail appears on the left of the
cassette label as chunky pixel art in the colors of the active shell, like a printed sticker:

```
│ ┌────────────────────────────┐ │
│ │▓▒░▒▓▓▒░░▒ A ────────────── │ │
│ │▒░░▒▓▓▓▒░▒ Never Gonna Give │ │
│ │░░▒▓▓██▓▒░ You Up           │ │
│ │▒▒▓███▓▒░░ Rick Astley      │ │
│ │▓▓██▓▓▒░░▒ ──────────── C90 │ │
│ └────────────────────────────┘ │
```
(The shading above only suggests an image; the real thing is colored pixels.)

- Setting `thumbnails: bool`, default **false**. Settings row "Cover art" (toggle with Enter
  or left/right, like the other toggles), applied immediately and saved through the existing
  save path. Changing it goes through one `App` method so the milestone 4 remote can call it.
- When on: the label always uses the `art = true` layout from milestone 1 (10 x 5 cell slot at
  the left, text from column 11). Until the image is ready, and if it cannot be fetched, the
  slot shows a placeholder: a checker of `shell.label` and a slightly darker paper tone made by
  mixing `label` with `label_ink` (about 15%), drawn with half-blocks. No status-bar message for a failed
  fetch.
- When off: exactly the milestone 1/2 label, and no network request is ever made for images.
- Nothing loaded (NO TAPE): no slot, plain label.
- During the tape swap (milestone 2), the art is hidden in the eject and insert phases (the
  slot slides as plain paper with the label) and appears when the tape is seated.
- Compact tier has no cassette, so no art. Deck and Pocket show it.
- Changing shell re-renders the art in the new shell's colors without refetching.

## 2. Fetching

- URL: `https://i.ytimg.com/vi/<id>/mqdefault.jpg` (320x180, no API key, no account). The id
  comes from `Track.id`; validate it is 11 characters of `[A-Za-z0-9_-]` before building the
  URL, otherwise skip.
- Fetch in a tokio task with the existing `reqwest` client pattern and a short timeout (5 s);
  the result returns to `App` as a `Msg` carrying the track id. Never block the UI thread.
  Respect the existing offline handling: no fetch while offline, and a failed fetch must not
  switch the app to the Offline screen.
- When to fetch: when a track starts (`Event::TrackStarted`) with the setting on; when the
  setting is switched on while a track is loaded; and for the preloaded next track when it is
  preloaded (so the art is ready at the swap). Never for list rows.
- Cache: decoded source pixels (see section 3) in memory keyed by track id, at most 8 entries,
  oldest dropped. A reply for a track that is no longer current or next is still cached.
  Ignore bodies larger than 1 MB and anything that fails to decode.

## 3. Pixelizing (`src/art.rs`, pure, unit tested)

- New dependency `image = { version = "0.25", default-features = false, features = ["jpeg"] }`.
- `decode(bytes) -> Option<Source>`: decode the JPEG, center-crop to a square, resize to
  40 x 40 RGB with a box/triangle filter. That small source is what the cache keeps.
- `pixelize(&Source, n, palette: &[Color]) -> Grid`: resize to `n x n` (n = 10 for half-blocks,
  n = 20 for the image protocol), then map every pixel to the nearest palette color with 4x4
  ordered (Bayer) dithering. Distance in plain RGB is fine. Deterministic: same input, same
  output.
- Palette: the shell's colors `body, select_bg, dim, frame, text, accent, label, label_ink,
  meter_lo, meter_hi` (duplicates removed). This is what makes the cover look printed in the
  deck's own inks.
- Dither strength is a named constant (start at a spread of 32 on a 0-255 scale).
- Tests: a solid color image maps to the nearest palette color everywhere; a horizontal
  gradient between two palette colors produces both colors and nothing else, with the darker
  share on the darker side; non-square input is center-cropped (a test image with distinct
  left/center/right bands); output dimensions; garbage bytes give `None`; a 1x1 image works.
  Build test JPEGs in the test with the `image` crate's encoder (enable the encoder only under
  `dev-dependencies` if the feature split allows it; otherwise decode-from-raw-RGB test helper
  plus one tiny JPEG fixture as a byte array in the test module).

## 4. Drawing

Two paths, same 10 x 5 cell rectangle inside the label.

**Half-blocks (always available, the default path):** grid 10 x 10. Each cell is `▀` with the
foreground set to the upper pixel and the background to the lower pixel. Our own code in
`ui/deck.rs`; no dependency. This is also what tests render.

**Image protocol (crisp path):** when the terminal supports a real image protocol.
- New dependency `ratatui-image = { version = "11.1", default-features = false, features = ["crossterm"] }`
  (no chafa; adjust features only as far as needed to build).
- At startup, after the terminal is in raw mode and before the event loop, ask the terminal
  once what it supports (the crate's `Picker` query). If the query fails, times out, or reports
  only half-blocks, use our half-block path. Decide once; store the picker in `App` (or next to
  the terminal in `main.rs`) and pass what `ui` needs through `App`, keeping "ui only reads".
  The startup query must not delay startup noticeably or leave stray bytes in the input: test
  in a terminal without support.
- Build the image from the 20 x 20 grid, scaled up with nearest-neighbour to the pixel size of
  the 10 x 5 cell rectangle (cell pixel size from the picker), so the pixels stay hard-edged.
  Create the protocol object once per (track id, shell, cell size) and cache it; do not rebuild
  it every frame.
- Environment override `TERMUSIC_ART=halfblocks|off` forces the half-block path or disables
  drawing art regardless of the setting (for terminals that misreport). Document it in README.
- When the art is hidden (swap phases, overlays do not cover the deck so that is not a case)
  or the label moves, make sure no stale protocol image stays on screen: verify what the crate
  does when a widget stops being rendered, and clear explicitly if needed.

## 5. Docs

- `CLAUDE.md`: replace the convention "Songs are text-only (no thumbnails); decoration is
  limited to unicode icons/logo." with: thumbnails are optional and off by default, shown only
  as pixel art on the cassette label for the playing track; lists stay text-only. Also update
  the Project line that says "text-only". Add `art.rs` to Architecture.
- README: the setting, the env override, which terminals get the crisp path.

## 6. Verification

- Unit tests as in section 3; render tests: label with art at 80x24 (cells in the slot are `▀`
  with palette colors only), placeholder when not loaded, plain label when the setting is off,
  art hidden in swap phases, shell change recolors.
- No test uses the network. The fetch function takes the URL/bytes path apart enough that the
  `Msg` handling is tested with canned bytes.
- Live: with the setting on and a real track, confirm over a pty that the half-block art
  appears (the slot cells become `▀` with varying colors) and that exactly one request per
  track is made (count in a debug log or by code reading; say which). The crisp path can only
  be judged by the user in Ghostty: report whether the picker query ran and what it returned
  in the terminal you tested in.
