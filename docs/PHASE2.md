# Phase 2: the board app with the demo feed

`cargo run --release -p now-boarding`

## What's in it

- **Model and providers** (`model.rs`, `provider.rs`): the brief's `Movement`,
  `Status` and `Provider`. The demo is the first provider; PTV and AeroAPI come
  next, behind the same trait.
- **Demo feed** (`demo.rs`): Gate 05 International (flights) and Harbour Street
  (trains). The day is worked out from the date, so it's the same every time
  and needs no key and no network. A flights day has about 110 departures and
  as many arrivals: delays, cancellations, gate changes, diversions and
  codeshares. A trains day has about 200 trains each way, with calling points,
  delays, cancellations and platform changes.
- **Poller** (`poller.rs`): each board has its own thread that asks its
  provider and passes the answer back, backing off on failures and on
  "busy". The window never waits on it.
- **Normaliser** (`normalise.rs`): scheduled order; drops movements once they've
  gone (flights 10 minutes after departure, arrivals 25 minutes after landing,
  trains after a minute or two). It also fills the cells, picks the status tabs
  (yellow for boarding and final call, amber for delayed, red for cancelled or
  diverted) and spots gate and platform changes.
- **Board view** (`view.rs`): the header (title and time in big cards, the
  place, an optional second-language title), the grid, a ticker of cards (a
  train's calling points, or notices), and a footer (updated time, attribution,
  page).
- **Paging**: a board shows *rows* rows (default 15). With more movements, it
  turns the page on a timer, every 10 seconds to every 10 minutes (default 30
  seconds), through at most *pages* pages (default 3). The footer says
  PAGE 1 OF 3.
- **Changes**: a gate or platform change lights a lamp round that cell, which
  blinks three times, and rings the chime. A flight starting to board rings it
  too, at most once every 4 seconds.
- **Stale data**: after two failed updates the footer says LAST UPDATED; after
  30 minutes without one the remarks fold to SEE SCREENS. The demo never fails,
  so this waits for the live feeds to be seen working.
- **Crowd clatter**: up to 10 flaps a frame get a click each. More are heard as
  a crowd: 8 clicks spread across the moment and the board, each as loud as
  the flaps it stands for.
- **Layouts**: one board, departures above arrivals in one window, or a window
  each.
- **Settings**: flights or trains, layout, rows, page turn and pages, column
  widths (each a preset you can change), second title, lamps, dimmer, bulbs,
  flip speed, clatter, chime, the demo's speed (real time, 10×, 60×) and start.
  It also has the About note: not for travel decisions.

The command line takes the same choices for trying things out: `--speed 60
--start 06:00 --layout stacked --trains --rows 15 --page 20 --mood evening
--settings`.

## Frame rate and paging

Fewer rows mean fewer cards: 15 rows is 495 against 24 rows' 792. The gallery
bench under Xvfb (4 shared cores, 1180 × 1080, a departure every 2 seconds):

| Rows | No lamps | Warm lamps |
| --- | --- | --- |
| 15 | 90 fps | 66 fps |
| 24 | 68 fps | 54 fps |

A page turn flips the whole board once, much like the top flight leaving.

## Done when

The brief's test, run at 60×: `demo::tests::a_day_at_the_board` plays a whole
day through the normaliser a minute at a time. It checks that flights leave
from the top only once they've gone, that delays and cancellations show, and
that gates change. The screenshots show the same at 60× and 10× in the app.
