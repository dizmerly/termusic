# Milestone 2 design spec: motion, level meters, tape hiss

Designer: Fable (main session). Implementer: Opus. Builds on milestone 1 (the Deck, branch
merged into `main`). Overall plan: /Users/dizmerly/.claude/plans/cached-toasting-peach.md.
Where this spec and the plan differ, this spec wins. Where it is silent, do the simplest
thing that fits and report it.

Three spikes were run by the designer against the installed mpv 0.42 (silent output) and passed:
- `--af=@lvl:lavfi=[astats=metadata=1:reset=1:measure_overall=none:measure_perchannel=RMS_level+Peak_level]`
  then `get_property af-metadata/lvl` returns
  `{"lavfi.astats.1.Peak_level": "-18.06", "lavfi.astats.1.RMS_level": "-21.02"}` (strings, dB;
  `.1` is the left channel, `.2` the right, mono has only `.1`). 0.12 ms per round trip.
- A separate mpv playing `av://lavfi:anoisesrc=color=pink:amplitude=0.5:duration=10` with
  `--loop-file=inf` runs and answers IPC (volume, time-pos).
- Not yet checked: the level filter on a real YouTube stream, and with crossfade/preload.

## 1. Animation clock

- `run_tui` (src/main.rs) polls input with a 50 ms timeout while `app.animating()` is true,
  otherwise 200 ms as today. `animating()` is true when animations are enabled AND (a track is
  playing, or a key press / swap / whirr is still running).
- `App` gets `anim: Anim`. All time-dependent drawing derives from it; `ui` has no clocks.
  `Anim { start: Instant, pressed: Option<(Key, Instant)>, swap: Option<(Instant, old title,
  old channel)>, whirr: Option<Instant> }`. The frame number is `start.elapsed() / 50 ms`.
  Pure drawing functions keep taking plain numbers (frame, phase), so tests need no clock.
- New setting `animations: bool` (default true) with a Settings row (Enter or left/right
  toggles). Off means: reels step once a second as in milestone 1, no momentary key press,
  no swap sequence, no meters, 200 ms loop.

## 2. Reels

- While playing, the hub glyph advances one `REEL_FRAMES` step every 3 frames (150 ms).
  Both hubs turn the same way; the right hub stays one step ahead.
- Whirr: for 400 ms after a seek, next or prev, hubs advance every frame.
- Paused or idle: hubs hold their current glyph (do not snap back to frame 0).
- Pack levels: unchanged from milestone 1.

## 3. Transport keys that press

The key block becomes 4 rows. A cap is 3 rows; it sits in rows 1-3 when up, with a one-row
shadow under it, and in rows 2-4 when down (the shadow is covered).

```
 up        down
┌───┐
│ ▶ │     ┌───┐
└─p─┘     │ ▶ │
 ▀▀▀      └─p─┘
```
- Shadow: ` ▀▀▀ ` in `shell.dim`.
- Latched caps stay down and lit (accent, bold) while their state holds: Play while playing,
  Pause while a loaded track is paused, Shuffle while on, Loop while not off.
- Momentary press: any cap goes down for 150 ms when its command is dispatched (from the
  keyboard now, from the remote socket later, so hook it in `App::dispatch`), lit while down.
  For Play/Pause the press shows on the cap that becomes latched.
- The latched positions are state, not animation: they show with animations off too. Only
  the 150 ms momentary press needs the fast clock.
- Deck pane row budget (22 rows at 80x24): cassette 12, blank 1, counter 1, meters 1, keys 4,
  status 1 = 20. Pocket tier: recompute so the J-card still gets at least 10 rows; raise the
  Pocket height threshold if needed and say so.

## 4. Tape swap

On `Event::TrackStarted` (not on resume from pause), with animations on, a 600 ms sequence:
1. 0-200 ms, eject: the old label (old title and channel, remembered in `Anim`) slides up out
   of the 5 label rows, one row per 40 ms; rows it leaves show the empty slot (background
   `shell.body`). Both reels show pack 0.
2. 200-400 ms, insert: the new label slides down into place, one row per 40 ms.
3. 400-600 ms, seat: full label, whirr.
First track after startup (no old label): skip phase 1. A new track starting during a swap
restarts the sequence with the label currently shown as the old one.
Pure function: `swap_phase(elapsed_ms) -> Phase` plus the label-row slicing, both tested.
Milestone 3 will hide thumbnail art during phases 1-2; expose the phase so it can.

## 5. Tape counter

Three digits, `position_secs % 1000`, as now. With animations on, a digit that changed is
drawn `shell.dim` for the first 100 ms after the change (a mechanical "between numbers" blink),
then `shell.text`.

## 6. Level meters (real audio levels only)

Row between the counter and the keys:
```
 L ▮▮▮▮▮▮▮▮▯▯▯▯▯▯   R ▮▮▮▮▮▮▯▯▯▯▯▯▯▯
```
- 14 segments per channel. Scale: RMS level in dB mapped linearly from -42 dB (0 segments) to
  0 dB (14 segments), clamped. The top 3 segments are `shell.meter_hi`, the rest
  `shell.meter_lo`; unlit segments `▯` in `shell.dim`.
- Peak hold: the highest segment reached stays lit alone for 500 ms, then falls one segment
  per 100 ms.
- Mono source: the right meter mirrors the left.
- Paused, nothing loaded, no data yet, or animations off: the row shows both meters unlit
  (animations off: the row is blank). Never show invented movement.
- Source: add the `astats` filter above to every music mpv (current and preload; build it in
  the same place as the other mpv args). Read `af-metadata/lvl` from the CURRENT track's socket
  about 10 times a second on a dedicated background thread owned by `Player` (never on the UI
  thread). The thread shares the current socket path and the latest `(left_db, right_db)` with
  the UI through `Arc<Mutex<..>>` or atomics; it idles when there is no current track, when
  paused, or when animations are off, and stops when `Player` is dropped. After `promote` it
  must follow the new current socket. A failed read clears the levels (meters fall to empty);
  it must not log or set the status.
- Parsing is a pure function from the reply JSON to `Option<(f32, f32)>`: tested with the real
  reply shape above, mono, missing keys, `-inf`, and garbage.
- Verify against a real YouTube track that levels move, that silence reads empty, and that
  preload + crossfade still work with the filter on both mpvs. If the filter breaks playback
  or crossfade in a way you cannot fix simply, remove the meters row, keep everything else,
  and report it. Do not fake it.

## 7. Tape hiss

- Settings: `hiss: bool` (default false), `hiss_level: u8` 0-100 (default 30). Two Settings
  rows: "Tape hiss" (toggle) and "Hiss level" (a slider like the Volume row, steps of 5,
  shown with a bar; dim when hiss is off).
- Sound source: one extra mpv owned by `Player`, started with `--no-video --loop-file=inf`,
  its own IPC socket and its own watchdog (the same kill-if-the-app-dies pattern as music
  tracks), playing generated noise. Start from
  `av://lavfi:anoisesrc=color=pink:amplitude=0.05:duration=60` and, if mpv accepts a filter
  chain in that source, shape it toward tape hiss with a high-pass around 1 kHz. Keep the
  source string and the amplitude as named constants at the top of the file; they will be
  tuned by ear by the user. Check with `--ao=null` that the source plays and loops.
- Volume of the hiss mpv = `user_volume * hiss_level / 100`. It follows volume changes and
  slider changes immediately.
- Lifecycle: the hiss runs only while hiss is on AND a track is loaded AND it is not paused.
  It pauses and resumes with the music (including a pause detected from outside through the
  poll, see `sync_pause`), keeps running across track changes and crossfades without a gap,
  and is killed when playback stops, when hiss is switched off, on quit, and by its watchdog
  if the app is killed. It is never polled for levels and never counted as a track.
- No hiss mpv is ever started in unit tests (follow the existing fake-mpv and socket-stub
  patterns).

## 8. Tests and checks

- Pure functions (reel frame from frame number, key cap up/down rendering and exact widths,
  swap phase and label slicing, counter digit states, meter segments and peak fall, level
  parsing, hiss volume math) each get focused tests.
- `animating()` truth table. Animations off renders exactly the milestone 1 reels.
- Extend `dump_renders` with: playing with meters at a fixed level, a key held down, each
  swap phase.
- Nothing panics at any size, as before.
