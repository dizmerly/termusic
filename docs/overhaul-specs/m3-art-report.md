# Milestone 3: Cover art, report

Branch `overhaul-3-art`, cut from `overhaul-2-motion`. This report describes what was built, the libraries used, the design choices, what the tests cover, what the review found, and what was and was not checked.

## What was built

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

## Technologies

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

## Design decisions

1. **Pixelize into the active shell's own inks (10 colors), with 4x4 Bayer dithering, spread 32.** The palette is `art::shell_palette`: the shell's body, select, dim, frame, text, accent, label, label-ink, meter-low and meter-high colors, with duplicates removed. The spread was chosen from a contact sheet of 4 real thumbnails, 3 shells, and spreads 0, 32 and 64 (`contact_sheet`, an ignored test that writes a PNG). Spread 64 was too noisy at 10x10.
2. **Tonal stretch**, added by the designer after the contact sheet. Dark covers collapsed into the darkest ink. `stretch` maps the image's luma range onto the palette's luma range with one affine map on all channels, so hue is kept. A flat image or a flat palette is left alone.
3. **Half-blocks everywhere; the crisp image only where a protocol is reported.** Half-blocks work in every terminal and need no query. The crisp image is only worth the risk where the terminal really supports an image protocol.
4. **The terminal query runs only when the environment names an image-capable terminal.** `art::should_query` checks `TERM_PROGRAM` (ghostty, iterm.app, wezterm, kitty), `TERM` (contains kitty or ghostty) and `KITTY_WINDOW_ID`. The reason: `ratatui-image`'s query thread outlives its timeout, and in a terminal that never answered it swallowed typed input (found in a live pty check). `TERMUSIC_ART` is read case-insensitively with surrounding whitespace removed (`normalize`): `detect` forces the query, `halfblocks` forces half-blocks, `off` turns art off, and `halfblocks` and `off` never query. The query has a 1 s timeout. A query that hits the timeout counts as failed, even if the library returned a guessed picker (`.filter(|_| start.elapsed() < timeout)`).
5. **One-time status notice.** When Cover art is switched on in a terminal without an image protocol, the status line says so once per session (`art_noted`). Off never shows the notice on the crisp path. This was requested as the error handling for that case.
6. **Covers: one request in flight per track, silent on failure.** A track's cover is requested on track start, on the preloaded track, on the toggle, and on reconnect. A failed or evicted id is removed from `art_tried`, so the next natural trigger asks again. Failures never write a status message and never switch to the Offline screen. The cache holds 8 covers. While streaming, the body is capped at 1 MiB (`push_capped`), so a chunked response without a content length cannot grow without limit.
7. **Art hidden during the tape swap's eject and insert phases, shown when seated.** The label slides as paper while the tape moves, so the crisp image (which the terminal draws over the cells) is only drawn in the seated phase. In `deck.rs`: `if let (Some(p), None | Some(Phase::Seat), Some(r)) = (crisp, v.phase, slot_rect(stack))`.
8. **Transport key caps widened to 6 and a one-column gap before the mode keys.** The prev, pause and next caps are 6 columns wide so their two-column glyphs center. A single column of space after next separates the transport bank from shuffle and loop, and makes the key row exactly the 34-column cassette width. The gap was decided by the Fable designer. Code: `key_caps` in `src/ui/deck.rs`, which pushes one space after the `Key::Next` cap.
9. **Branch `overhaul-3-art` was cut from `overhaul-2-motion`.** Milestone 2 is not merged into `main`, so this branch carries milestone 2's changes too.

## Test coverage

All tests pass. The full suite is 235 passed, 2 ignored. The ignored tests are `contact_sheet` in `src/art.rs` (writes a PNG for design review; reads its inputs from `TERMUSIC_THUMBS`) and `dump_renders` in `src/ui/mod.rs` (not part of this milestone's table).

### `src/art.rs`

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

### `src/app.rs`

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

### `src/ui/deck.rs`

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

### `src/ui/mod.rs`

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

### Not covered by tests

- The live network fetch of the thumbnail (`art::fetch` against i.ytimg.com).
- The real terminal query and its 1 s timeout branch.
- The crisp image in a real terminal (kitty, sixel, iTerm2, Ghostty, WezTerm).
- Sixel and iTerm2 image overflow after a font-size change. This is a known limit, marked `shortcut:` in `art::protocol`.
- That exactly one request is made per track. The tests and code reading show this; real requests were not counted.

## Review

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

## Verified live

Checked in a pty with a real `mpv` and `yt-dlp`, using a terminal that does not answer image queries:

- Startup took under 1 s.
- A typed search reached the app.
- A real track played.
- The label slot showed the cover as 10x5 half-block cells in 8 shell colors.
- No `mpv` was left running after quit.
- Milestone 2's `astats` level meters read correctly from a real YouTube stream. This is the first real-stream check of milestone 2.

## Not verified

- The crisp path in Ghostty, kitty or iTerm2. Only the author can look at this.
- That exactly one request is made per track. This is shown by tests and code reading, not by counting real requests.
- How it looks by eye in the author's terminal.

## Items for the author to review

- The dithering look per shell: `DITHER_SPREAD` in `src/art.rs` (32), and the shell palettes in `shell_palette`.
- The tonal stretch in `stretch` (`src/art.rs`).
- The key row spacing: the 6-column transport caps and the one-column gap in `key_caps` (`src/ui/deck.rs`).
- The README "Cover art" section. It already describes the setting, the detection hints, `TERMUSIC_ART`, and the one-time notice. It also says that WezTerm goes through the iTerm2 protocol because the `image` crate blacklists its kitty and sixel paths. That claim was not re-checked in this report.
