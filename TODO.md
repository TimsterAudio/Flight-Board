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
- [x] **Phase 4**: FlightAware AeroAPI for MEL: polling budget, codeshare
  cycling, airline colour cards, and logos the user adds for personal use.
  Still to do with the owner's key: record the AeroAPI fixture, check the
  price per page, and watch the board beside MEL's own.
- [ ] **Phase 5**: release 1.0: user guide, release workflow for Windows and
  Linux, beer money.

## Later

- [x] **Airline logo cards**, for personal use: logos the user adds to the
  logos folder (`QF.png`) show on the airline's card. The app ships and
  fetches none: they're trademarks. Airlines without one get colour cards.
- [ ] A licensed logo set, if one is ever wanted for a public release.
- [ ] Full-screen wall mode.
- [ ] More train providers: Darwin (Great Britain), Transport for NSW, Deutsche
  Bahn, NS, opentransportdata.swiss, generic GTFS-Realtime.
- [ ] AeroDataBox as a second flight provider, if AeroAPI's free credit runs
  out too fast.
- [ ] Mac build.
