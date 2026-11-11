# Milestone 4: Remote control and menu bar helper, report

Branch `overhaul-4-helper`, cut from `overhaul-3-art` (four commits on top of `9194e4e`). This report describes what was built, the technologies, the design choices, what the tests cover, what the review found, and what was and was not checked. The design is in `docs/overhaul-specs/m4-helper-spec.md`.

## What was built

- **A control socket** in termusic (`src/remote.rs`). It is a Unix socket at `<config dir>/termusic.sock`, mode 0600. Clients send one JSON object per line; termusic answers with state lines and error lines.
- **The protocol** (the table is in the README, section "Menu bar helper (macOS)"): playback commands (`play_pause`, `next`, `prev`, `seek`, `toggle_shuffle`, `cycle_loop`), settings (`volume`, `loop`, `shuffle`, `shell`, `thumbnails`, `animations`, `hiss`, `hiss_level`), and `get state`.
- **State lines** carry the title, channel, id, loaded and paused flags, position (whole seconds), duration, volume, loop, shuffle, the shell id and list, and the settings. They are sent on connect, after each accepted request, and whenever the state changes.
- **A menu bar helper** (`helper/`, SwiftPM package, executable `TermusicBar`). It shows the title in the menu bar (`♪` when nothing plays), and a menu with title and channel, Play/Pause, Previous, Next, a volume slider, Shuffle, a Loop submenu, and a Settings submenu (Shell, Cover art, Animations, Tape hiss, Hiss level). Quit quits only the helper.

Where the code lives:

- `src/remote.rs`: `parse` (line to `Request`), `bind` (ownership rule), `serve` and `client` (accept loop and per-client tasks), `Reply`, `State` and `ShellInfo` (what clients see), `socket_path`.
- `src/app.rs`: `serve_remote` (binds and starts the server), `tick` (handles `Msg::Remote`, at most `REMOTE_PER_TICK` = 32 per tick), `apply` (maps a `Request` onto the setters and `dispatch`), `cmd_gate` (offline and disabled checks for playback commands), `snapshot` (builds the state, no IO), `publish` (sends it on the watch channel when it changed), `shutdown` (removes only our own socket), and the new setters `set_volume`, `set_shell`, `set_loop`, `set_shuffle`.
- `helper/Sources/TermusicProtocol/Protocol.swift`: decoding of state and error lines, request encoders, socket path, and the menu title. No AppKit import.
- `helper/Sources/TermusicBar/Connection.swift`: the socket client (POSIX sockets, one background queue, reconnect every 2 s).
- `helper/Sources/TermusicBar/main.swift`: the menu (`Bar` class) and the throttled sliders.
- `helper/Sources/TermusicBar/SelfTest.swift`: `--selftest`, the same checks as the Swift test target.

## Technologies

- **Rust, no new crates.** `Cargo.toml` is unchanged from `overhaul-3-art`. The code uses what the project already had:
  - `tokio` (already `full`): `UnixListener` and `UnixStream` for the socket, `watch` for the state snapshot, `mpsc` for replies and the request queue, `Notify` to close a client.
  - `serde` (derive) and `serde_json`: the state object and the error lines are built with `json!` and `Serialize`. Requests are parsed from `serde_json::Value`.
  - `std::fs::File::try_lock` (exclusive `flock` on Unix) for `termusic.lock`. The lock is released when the process exits, including after a crash.
- **Swift, no packages.** `helper/Package.swift` declares swift-tools-version 5.9 and a macOS 13 minimum. The toolchain here is Swift 6.4 (`swift --version`). The helper uses AppKit (`NSStatusBar`, `NSMenu`, `NSSlider`) and POSIX sockets (`socket`, `connect`, `poll`, `read`, `write` through `Darwin`). No third-party package and no Xcode project.

## Design decisions

1. **Built on milestone 3.** The branch is cut from `overhaul-3-art` because the protocol includes `set thumbnails`, which is milestone 3's setting. The spec header says the work builds on milestones 1 and 2 and that they are merged into `main`; that header is out of date. Milestone 2 is not merged into `main` either (see the m3 report, design decision 9), so this branch carries milestones 2 and 3 with it.
2. **One code path.** Every request becomes a `Msg::Remote` and is applied on the UI thread by `App::apply`, which calls the same setters and `dispatch` as the keys. The setters that did not exist before are `set_volume`, `set_shell`, `set_loop` and `set_shuffle`; `set_animations`, `set_thumbnails`, `set_hiss` and `set_hiss_level` were already there. Playback commands go through `cmd_gate` first: while offline they get `offline`, and when their key is disabled in `commands/` they get `disabled`. The keyboard gets the same results in practice: the Offline screen takes no playback keys, and `handle_list_key` skips disabled keys. Settings requests are not gated by offline, because the spec asks that settings work offline; the keyboard cannot change settings offline. A remote `shuffle` or `loop` change plays the same key-press animation as the key (`press`, through `apply` and `dispatch`).
3. **Ownership.** `termusic.lock` is taken with an exclusive lock before anything else. Then the socket path is checked: a socket that answers belongs to another termusic and is left alone, a socket that does not answer is a crash leftover and is removed, and a path that is not a socket (a regular file, for example) is left alone with a status line. The new socket is bound under a temporary name (`termusic.sock.<pid>`), chmod 0600, and renamed into place, so the socket never exists with looser permissions. The config dir is created with mode 0700 if it is missing. On shutdown, the socket is removed only if its (dev, ino) still matches the socket this process bound, so a socket that another bind put in place survives.
4. **Flow control.** A client does not have to wait for replies: the reader keeps reading and never blocks on the writer. At most 4 clients are served at once (`MAX_CLIENTS`); extra connections are closed. A client's reply queue holds 16 lines (`OUT_QUEUE`); a full queue closes that client. A write that takes longer than 2 s closes the writer (`WRITE_TIMEOUT`). A line over 4096 bytes closes the client (`MAX_LINE`). A slot is freed only when both the reader and the writer task of the client have ended (the `Guard` counter). The app handles at most 32 remote messages per tick (`REMOTE_PER_TICK`), so one flooding client cannot stall a frame.
5. **State lines.** The snapshot is sent only when it differs from the last one (`send_if_modified`). Position is floored to whole seconds, so while playing there is about one state line a second. Each accepted request gets exactly one reply: a state line for a valid request (`get state` and every setter), or an error line. A state change can add an unsolicited state line, so a client may see two state lines for one request. Malformed JSON gets an error line and the connection stays open.
6. **The helper renders only the last state line.** It keeps no state of its own and never guesses: if the connection is down, the title reads `termusic is not running`, the menu bar item is dimmed, and all controls except Quit are disabled. It reconnects every 2 s. The volume and hiss-level sliders send at most 10 values a second while dragging (`throttled`, 0.1 s), and the release always sends the final value. A slider is not overwritten from the state while the mouse is dragging it. The Shell submenu is rebuilt only when the `shells` list changes, because replacing items under an open submenu breaks it. Shell names are shown as the app sends them (`SUNSET TAPE`, `AMBER HI-FI`); the helper does not change their case.

## Test coverage

Verified in this session: `cargo test` reports 258 passed and 2 ignored (milestone 3 was 235 passed). `remote.rs` has 23 tests, which is the difference.

### `src/remote.rs`

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

### Swift

Two sets of checks, with the same content:

- `helper/Tests/TermusicProtocolTests/ProtocolTests.swift`: 10 Swift Testing tests (`decodesRealLine`, `decodesValuesAndUnknownField`, `decodesErrorAndRejectsJunk`, `requestBytes`, `socketPaths`, `titles`, `nullsDecode`, `appErrorsDecode`, `missingFieldIsNil`, `nullPositionDecodes`).
- `TermusicBar --selftest` (`helper/Sources/TermusicBar/SelfTest.swift`): 35 checks, the same cases plus the request bytes for every setter. Verified in this session: the existing release binary prints `ALL PASS` and exits 0. I did not rebuild it; its timestamp is newer than the sources.

`swift test` does not run under the command line tools here. Verified in this session, it fails while building the test target with `external macro implementation type 'TestingMacros.TestDeclarationMacro' could not be found for macro 'Test'; plugin for module 'TestingMacros' not found`. The test target should run under Xcode.

### Not covered by tests

- Rust: the per-tick cap (`REMOTE_PER_TICK`). The burst test sends 30 requests, below the cap of 32, so no test reaches it. The accept-error retry in `serve`.
- Swift: `Connection` (socket I/O, reconnect, the 10 per second slider throttle) and the menu (`Bar`). They are not covered by any check; the encoders and decoders are.
- Live behaviour of the menu (layout, wording, the dimmed title, slider feel).

## Review

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

## Verified live

Checked by the implementing session (Opus) after the review fixes, in a pty with a real termusic, real `mpv` and `yt-dlp`, and a script that drove the socket; the helper check ran the real `TermusicBar` binary against the real app.

- The socket was created with mode 0600 and removed after quit.
- A state line arrived on connect. State followed playback (title, loaded, position advancing).
- `play_pause` paused and resumed mpv. `set volume` reached mpv.
- `set shell` changed the screen. `set hiss` started the hiss mpv.
- Garbage input got error lines, and the connection stayed open.
- `next` changed the track.
- No mpv was left running after quit.
- The helper, run against the real app: it connected (one more Unix peer on the app), stayed alive when termusic quit, reconnected when termusic started again, and left nothing running.

## Not verified

- How the menu looks: layout, wording, the dimmed title. Only the author can judge this.
- Slider feel and the 10 per second throttle.
- Clicking each menu item. The encoders are checked by `--selftest` and the Rust parser tests, not by clicking.
- A second OS user trying to connect. The socket is 0600 and the directory is 0700; this was not tried with another user.
- Swift Testing under Xcode: `swift test` was not run successfully here.

## Items for the author to review

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
