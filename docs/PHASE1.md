# Phase 1: the `splitflap` crate

What was lifted from Split-Flap Clock 1.2.0, what's new, and how fast it draws.

## Lifted unchanged in behaviour

`flap.rs` (cards and drums), `sound.rs` (clatter, chime, bulb pop), `lighting.rs`
(lamps and moods), `rng.rs`. Their tests came with them.

Two changes reach the clock, and both draw identical pixels:

- `Lighting::draw` paints its shade and warm glow as one layer instead of two.
  With premultiplied colours, darkening by an alpha and then adding a colour is
  the same as one colour at that alpha.
- A symbol too wide for its card is printed smaller. No clock card is that wide.

## New

- **Drums**: `Drum::Time` (blank, 0 to 9, colon) for times and gate numbers.
  `Drum::Words` holds whole phrases, one a flap. `flap::REMARKS` lists them in
  the order a flight goes through them: ON TIME, EXPECTED, DELAYED, GATE OPEN,
  BOARDING, FINAL CALL, GATE CLOSED, DEPARTED, LANDED, ARRIVED, BAGGAGE,
  DIVERTED, CANCELLED, PLATFORM CHANGE, SEE SCREENS.
- **Text fold** (`fold.rs`): accents dropped, case raised, anything else
  becomes a space. Long names are shortened from an abbreviation table
  (INTERNATIONAL to INTL, AIRPORT to APT, STATION to STN…), then cut.
- **`Board`** (`board.rs`): columns of letter cards, time cards or one wide word
  flap, under captions. `set_rows` finds the first row that differs and sets
  every row from there down flipping, each 40 ms after the one above. Each card
  flips only as far as its new symbol. Rows can have their own ink, a lit tab
  beside the remark, and coloured tiles (for operator codes later).
- **One mesh a board** (`mesh.rs`): the clock draws each card with the
  painter, clipping each half of each letter. Every clip is a draw call, about
  3,100 a frame on a 24-row board. Mesa's software renderer, which Xvfb uses,
  can't keep up with that. The board tessellates each card's parts once and
  keeps them, cuts letters by their quads instead of clipping, and sends the
  whole board as one mesh. Lamps light its vertices rather than painting a
  layer over the window.

## Frame rate under Xvfb

Measured with `gallery --rows 24 --every 2 --bench 10` on this build machine:
4 shared cores, Mesa llvmpipe, a 1180 × 1080 window, 792 cards. With a
departure every 2 seconds, cards are flipping in about 88% of frames, a much
busier board than any real one.

| Lamps | Mean | Median frame | 99th percentile |
| --- | --- | --- | --- |
| Off | 80 fps | 12.1 ms | 18.3 ms (55 fps) |
| Warm | 55 fps | 17.5 ms | 26.0 ms (38 fps) |
| Evening | 57 fps | 17.2 ms | 24.4 ms (41 fps) |

Before the mesh work, the same board ran at 11 fps (17 fps without lamps).
egui's own work is now under 2 ms a frame (`cpu_bench`). The rest is Mesa
rasterising on the CPU, so a machine with a GPU has room to spare. With lamps
on, the board doesn't quite hold 60 fps on this machine's software renderer.
The lamps' remaining cost is spread thin: the glow over the lamps, the lit
vertex colours and the shadows each cost a millisecond or so.

## The clock on the crate

`docs/clock-on-splitflap.patch` removes the clock's `flap.rs`, `sound.rs`,
`lighting.rs` and `rng.rs` and takes them from the crate, pinned to a commit
of this repository. Built that way, with lamps on and three places with sky
cards, its screenshot under Xvfb is pixel-identical to 1.2.0's: ImageMagick
`compare -metric AE` counts 0 pixels different. Its tests (30), fmt and clippy
for Linux and Windows all pass.
