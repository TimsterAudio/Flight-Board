# Now Boarding

A live airport and railway departures and arrivals board, drawn with
split-flap cards: *Flight Board* or *Train Board*. Windows and Linux; Mac
later. The design brief is [docs/BRIEF.md](docs/BRIEF.md).

Work in progress: Phases 0 to 2 of the brief's build plan; see
[TODO.md](TODO.md).

    cargo run --release -p now-boarding

runs the board on its demo feed: no key, no network. Right-click or
double-click it for Settings.

## What's here

- `crates/now-boarding`: the app. See [docs/PHASE2.md](docs/PHASE2.md).
- `crates/splitflap`: the split-flap look shared with
  [Split-Flap Clock](https://github.com/TimsterAudio/SplitFlap_Clock). It holds
  the cards and their drums (letters, times, and word flaps for remarks), text
  folded to what a drum can show, the `Board` grid that ripples to new rows,
  the clatter and chime, and the lamps. Pure look and sound: no network.
- `docs/DATA.md`: the providers' costs and limits, to be checked against
  their own pages (Phase 0).
- `tools/record_fixtures.sh`: records one real response per provider into
  `fixtures/`, run on your own machine with your own keys.
- `docs/clock-on-splitflap.patch`: moves the clock onto the crate.

## The gallery

A made-up airport's departures on a full board:

    cargo run --release -p splitflap --example gallery -- --rows 24 --mood warm

`--every S` sets the seconds between departures, `--mood off|soft|warm|evening`
sets the lamps, `--sound` plays the clatter, and `--bench S` prints the frame
rate after S seconds. `cpu_bench` times egui's share of a frame without a window.

## Checks

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings
    cargo test

On Linux, building needs ALSA's headers (`libasound2-dev`).

The board's typeface is Barlow Condensed SemiBold by The Barlow Project Authors, under the
SIL Open Font License (`crates/splitflap/assets/OFL.txt`).
