# Roadmap

The brief's build plan, with what's been added along the way.

## Phases

- [x] **Phase 0**: cost table (`docs/DATA.md`) and fixture script. The table is
  still to be checked against the providers' own pages, and the fixtures to be
  recorded on the owner's machine.
- [x] **Phase 1**: the `splitflap` crate, the gallery, the clock on the crate.
- [x] **Phase 2**: the board app with the demo feed: settings, normaliser,
  paging, row shifting, crowd clatter, the three layouts.
- [x] **Phase 3**: PTV for Southern Cross: key store, station search, polling,
  cache on disk, stale handling, attribution. Still to do with the owner's
  key: record the PTV fixture, and watch the board beside PTV's own for an
  hour.
- [ ] **Phase 4**: FlightAware AeroAPI for MEL: polling budget, codeshare
  cycling, and **airline colour cards**: the airline's two-letter code on a
  card in the airline's colour, instead of a logo (see below).
- [ ] **Phase 5**: release 1.0: user guide, release workflow for Windows and
  Linux, beer money.

## Later

- [ ] **Airline logo cards.** A card on the FLIGHT column that shows the
  airline's mark instead of its code, as some boards did. The 1.0 rules say no
  airline logos: they're trademarks, and the app can't ship them. Ways in:
  logos the user supplies (a folder of images by IATA code), or a licensed set.
  Until then, airline colour cards stand in: the IATA code (QF, NZ, SQ) in
  plain card letters on a card in the airline's main colour. The colours come
  from a small built-in table of common airlines, which Settings can change or
  add to; airlines not in it get the board's own card.
- [ ] Full-screen wall mode.
- [ ] More train providers: Darwin (Great Britain), Transport for NSW, Deutsche
  Bahn, NS, opentransportdata.swiss, generic GTFS-Realtime.
- [ ] AeroDataBox as a second flight provider, if AeroAPI's free credit runs
  out too fast.
- [ ] Mac build.
