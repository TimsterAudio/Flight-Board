# Now Boarding — Design Brief

Oct 30, 2026 · @Tim

## Summary

Build **Now Boarding**: a desktop app (Windows and Linux, Mac later) that shows a live departures or arrivals board for a chosen airport or railway station, drawn with the split-flap cards from [Split-Flap Clock](https://github.com/TimsterAudio/SplitFlap_Clock) 1.2.0. Yes, it can do both planes and trains, but they are not equally easy: live train boards are free in most places (UK, Australian states, Germany, Netherlands, Switzerland), while live flight boards with gates and remarks almost always need a paid API and the user's own key.

The look is mostly solved; the new work is the data. That means network code (which the clock deliberately has none of), a provider layer that turns each feed into one common movement record, and a board that re-sorts rows the way a real one does. Every provider sits behind a built-in demo feed, so the board can be built, tested and shown with no key and no network.

## What already exists

Split-Flap Clock 1.2.0 (Rust, egui 0.36 / eframe, about 12,900 lines, Windows, Linux and Mac) already has most of the board's look. It does not use TAW Panel; it has its own look. What carries over:

| Part | Where in the clock | What the board gets from it | Gap for a board |
| --- | --- | --- | --- |
| Flap card | `src/flap.rs` (631 lines) | Drum per card that only steps forward; fast 0.07 s flips on the way, 0.30 s landing flip; picture flaps (the sky card) | Drum is 46 symbols, capitals only: `␣A–Z0–9.-+:/'&,()`. No lowercase, no accents, no word flaps |
| Board layout | `src/app.rs` (`Line`, `Cell`, big and small card sizes) | Rows of cards, region headings, colours per row, refresh that flips from the first changed row down | Fixed columns, sorting and row shifting are new |
| Clatter and chime | `src/sound.rs` (cpal, 64 voices, two-note airport chime) | The sound of a board updating; announcement chime | 64 voices is too few for a full board flipping at once |
| Lamps and moods | `src/lighting.rs` | Warm lamps, dimmer, blowing bulbs, the janitor; moods Classic, Terminal 1975, Last flight out, Quiet library | None: reuse as is |
| Places | `src/places.rs`, `src/cities.rs` (7,000+ cities), chrono-tz | City and time-zone lookup | Airports (IATA/ICAO) and stations are new lists |
| Packaging | Release workflow, Mac app script, HTML/PDF user guide | One-file downloads for all three systems | Needs a TLS stack and a key store |

The clock promises "nothing downloaded while it runs". The board must break that promise, so it is a separate app, not a mode of the clock.

## Product scope

One app, two modes (flights and trains), two directions (departures and arrivals), one board per window. Version 1 covers:

- **A board** for one airport or station, departures or arrivals, chosen in Settings. Several windows can show several boards.
- **Demo feed** built in: a made-up airport ("Gate 05") and station with a full day of movements, delays, gate changes and cancellations, so the board runs with no key and no network.
- **One live train provider and one live flight provider** in 1.0: PTV (Victoria) for trains and FlightAware AeroAPI for flights, with the provider layer open for more.
- **The clock's look and show**: lamps, moods, clatter, the chime for an announcement ("Now boarding", "Platform change").
- **Layouts**: one board; departures and arrivals stacked top and bottom in one window; or each in its own window. Full-screen wall mode comes after 1.0.

Not in version 1: journey planning, alerts on your own flight, maps, ticketing, or anything used to make real travel decisions. The board says so in About: data can be late or wrong; check with the airline or operator.

## Data sources and availability

Trains are the easy half: official live boards with platforms are free in many countries. Flights are the hard half: the data a board shows (gate, terminal, "Boarding", "Gate closed") lives in airline and airport systems, and it reaches developers mainly through paid APIs. Figures below came from searches on 2026-10-30 and are **not verified against the providers' own pricing pages**; Phase 0 checks each one.

**Trains**

| Provider | Covers | Cost and access | Platforms, delays, calling points | Notes |
| --- | --- | --- | --- | --- |
| [Darwin Live Departure Board](https://departures-board.github.io/Departures-Board-API-Keys-Guide.pdf) via Rail Data Marketplace | Great Britain, every National Rail station | Free account, free subscription, own key per product; approval same day to 3 days | Yes, yes, yes (Service Details product) | The closest thing to a real board. Older OpenLDBWS SOAP feed is being replaced; use RDM. Attribution to National Rail likely required |
| Transport for NSW Open Data | Sydney Trains, NSW TrainLink | Free key (from memory) | GTFS-Realtime trip updates; platforms in stop data | Needs GTFS static + realtime joined into a board |
| PTV Timetable API | Victoria (Melbourne trains, V/Line) | Free developer ID and key, signed requests (from memory) | Departures endpoint with platform and estimated time | Board-shaped already |
| Deutsche Bahn Timetables, NS API, opentransportdata.swiss | Germany, Netherlands, Switzerland | Free keys (from memory) | Yes | Good for later providers |
| Generic GTFS-Realtime | Many cities worldwide | Usually free | Delays yes; platforms often missing | One adapter covers many networks, but boards look thinner |

**Flights**

| Provider | Cost and access | What a board gets | Notes |
| --- | --- | --- | --- |
| [FlightAware AeroAPI](https://flightaware.com/commercial/aeroapi/) | Per query; up to $5 free a month on the Personal tier, which is for personal and academic use only | Scheduled and estimated times, gates, terminals, status | Billed per 15 records; a [user reports](https://discussions.flightaware.com/t/query-response-limit/80831) 10 queries a minute and a cap of 10 pages per query on Personal. A busy airport's departures can take several pages, so a free month runs out fast if polled often |
| AeroDataBox (via [API.market](https://api.market/blog/aerodatabox/blog-updates/buying-best-flight-api-complete-guide-api-market) or RapidAPI) | Airport FIDS endpoints are a [paid tier](https://pub.dev/packages/aerodatabox); free allowance unclear (one blog says 700 requests a month) | Departures and arrivals by time window, terminal, gate, status, codeshares | Board-shaped and cheap; the fallback if AeroAPI's free credit runs out too fast |
| [OpenSky Network](https://openskynetwork.github.io/opensky-api/python.html) | Free with an account, non-commercial | Departures and arrivals per airport from ADS-B, after the fact | No schedules, gates, status or destinations shown on boards; airports are estimates. Not a live board; useful only for a "recently departed" view |
| Aviationstack, FlightLabs, Flightradar24 API | Small free tiers or monthly plans | Varies | Check in Phase 0 |
| Airport websites | Free to look at | Exactly the airport's own board | Scraping breaks without warning and usually breaks the site's terms. Not in 1.0 |

**What this means for the build**

- **Bring your own key.** A public binary can't carry a shared key: it would leak, and every user's polling would bill one account. Users paste their own key in Settings; it is kept in the system key store.
- **Poll on a budget.** The app works out the interval from the provider's limits and the user's monthly allowance (for example, $5 of AeroAPI a month for one airport means polling about every 10 to 20 minutes, not every minute), and shows the next update time.
- **Trains first.** One free, board-shaped train feed (PTV, Victoria) proves the whole pipeline before money is spent on flight data.

## Board design

The board is a header, a fixed grid of rows, and a footer, all in flap cards. It should read like a 1970s Solari board first and a data screen second.

**Layout**

- **Header**: board title in big cards (`DEPARTURES`, `ARRIVALS`), the airport or station name, and the local time using the clock's big digits. An optional second-language title on a small line, as real boards did.
- **Rows**: 8 to 24, chosen in Settings; default 12. Blank rows are blank cards, never empty space.
- **Footer**: last updated time, provider attribution, and a one-line ticker for calling points or notices.

**Columns** (widths in cards; each set is a preset the user can adjust)

| Board | Columns |
| --- | --- |
| Flight departures | TIME 5 · FLIGHT 7 · DESTINATION 16 · GATE 4 · REMARKS 12 |
| Flight arrivals | TIME 5 · FLIGHT 7 · FROM 16 · EXPECTED 5 · REMARKS 12 |
| Train departures | TIME 5 · DESTINATION 18 · PLAT 3 · EXPECTED 9 · OPERATOR 10 |
| Train arrivals | TIME 5 · FROM 18 · PLAT 3 · EXPECTED 9 |

**Cards and drums**

- **Digit drum** for times and gates (0–9, space, colon): short drum, quick to land.
- **Letter drum**: the clock's 46 symbols. Text is folded to it: accents dropped (ZÜRICH to ZURICH, SÃO PAULO to SAO PAULO), lowercase upper-cased, anything else to a space.
- **Word flaps** for remarks: one wide card per cell with a drum of printed phrases, as Solari boards had: ON TIME, DELAYED, BOARDING, GATE OPEN, FINAL CALL, GATE CLOSED, DEPARTED, CANCELLED, LANDED, BAGGAGE, DIVERTED, EXPECTED, PLATFORM CHANGE. Faster, cleaner and more authentic than spelling them.
- **Operator flaps**: a coloured tile with the IATA or operator code (QF, BA, GWR), never a real airline logo.
- **Long names**: an abbreviation table first (INTERNATIONAL to INTL, AIRPORT to APT), then cut at the column width. Codeshares cycle on the FLIGHT cell every 8 s, as at real airports.

**How it moves**

- When the first row departs, every row below moves up one, and each card flips only as far as its new symbol: the clock's "from the first changed row down" refresh.
- Changes land row by row from the top with a small stagger (about 40 ms), so the board ripples rather than snapping.
- Status colours, used sparingly: yellow for Final call and Boarding, red for Cancelled, amber for Delayed, as a lit tab beside the remark rather than coloured text. A gate or platform change blinks its cell's lamp three times with the chime.
- Stale data: after two failed updates the footer shows `LAST UPDATED 14:05`, and after 30 minutes the remarks fold to `SEE SCREENS`.

## Architecture

Three layers: a shared flap crate for the look, the board app for everything else, and providers behind one trait. The network lives only in the app, on its own thread, so the UI never waits on it.

&#91;embedded content: architecture · providers, board app, splitflap crate\]

Each board's poller thread fetches from one provider; the grid only ever receives the cells that change, and the clock draws with the same crate.

**Crates and repos**

- **`splitflap` crate** (new, lifted out of the clock, a workspace member in the Flight-Board repo): `flap.rs` card and drums, the clatter synth from `sound.rs`, lamps and moods from `lighting.rs`, plus a new `Board` grid widget that takes rows of text and animates the difference. Pure look and sound, no network. The clock moves onto it in a patch release, so both apps share one look. Apps pin a commit, as with TAW Panel.
- **`Flight-Board` repo** (exists, empty): Settings, providers, the poller, the key store, the demo feed, the layouts, releases.

**Data model** (sketch; the new session settles names)

```rust
struct Movement {
    id: String,                  // stable across updates, for diffing rows
    mode: Mode,                  // Flight | Train
    direction: Direction,        // Departure | Arrival
    scheduled: DateTime<Tz>,     // in the airport's or station's own zone
    expected: Option<DateTime<Tz>>,
    numbers: Vec<String>,        // QF1, BA7311 … codeshares; one for trains
    operator: String,            // code for the operator tile
    place: String,               // destination or origin
    via: Vec<String>,            // calling points
    stand: Option<String>,       // gate or platform
    terminal: Option<String>,
    status: Status,
}

enum Status { OnTime, Delayed, Expected, Boarding, GateOpen, FinalCall, GateClosed,
              Departed, Landed, Arrived, Baggage, Cancelled, Diverted, Unknown }

trait Provider: Send {
    fn name(&self) -> &str;
    fn attribution(&self) -> &str;
    fn min_interval(&self) -> Duration;   // from the provider's limits
    fn search(&self, text: &str) -> Result<Vec<Place>>;   // airports or stations
    fn board(&self, q: &BoardQuery) -> Result<Vec<Movement>>;
}
```

**Update loop**

1. A poller thread per board calls `Provider::board` at the budgeted interval, with back-off on errors and `429`.
2. Results go through one normaliser: sort by time, drop movements more than 10 minutes gone, fold text to the drum, map status to a word flap.
3. The board diffs new rows against shown rows by `id` and hands the grid only the cells that change.
4. The last good board is cached on disk, so a restart shows something at once and offline shows the last known board.

**Libraries**: `ureq` with `rustls` for HTTPS (blocking, no async runtime, builds for Windows from Linux), `serde_json`, `keyring` for keys, `chrono-tz` as in the clock. SOAP is avoided by using Darwin through Rail Data Marketplace's REST.

## Challenges and risks

The biggest risk is flight data cost, not code. Ranked by how likely each is to sink or reshape the project:

| Challenge | Why it matters | How we handle it |
| --- | --- | --- |
| Flight data costs money | Gates and remarks need a paid API; free tiers cover a few hundred calls a month | Bring-your-own key, a polling budget, trains first, demo feed always there |
| Provider terms | Personal tiers forbid business use; scraping breaks airport terms; attribution is often required | Personal use only, stated in About; no scraping; attribution in the footer |
| Every feed is different | Status words, time zones, codeshares and platform fields differ per provider | One `Movement` model and normaliser; recorded sample responses as test fixtures per provider |
| Performance | A 12 × 45 board is about 540 cards, ten times the clock; egui repaints the whole window while anything flips | Draw only changed cards' shapes, cache static ones, cap the frame rate when idle; measure in Phase 1 with a 24-row board |
| Sound | The clatter has 64 voices; a full board flipping at once would need hundreds | A crowd model: past a threshold, play a blended clatter scaled to the number of flips instead of one voice per card |
| Character set | Names with accents, long names, lowercase, non-Latin scripts | Fold to the drum, an abbreviation table, a column cut; non-Latin names use the provider's Latin field |
| Network code in a family that has none | TAW Panel and the clock are both "nothing downloaded" | Network only in the board app, one HTTPS client, no telemetry, nothing sent but the provider request |
| Keys in a public app | Users must create accounts and keys, some with a wait of up to 3 days | A Settings page per provider with step-by-step instructions and a test button |
| Data can be wrong | Feeds lag; a board is trusted on sight | Show last updated time; stale fold to SEE SCREENS; About says not for travel decisions |
| Mac signing | The clock's Mac app is unsigned; a network app may trip more warnings | Same approach as the clock for 1.0; signing is a later choice |

## Build plan

Six phases, each ending in something the owner can run and look at. Phases 0 to 2 need no keys and no money.

1. **Phase 0: check the data (a day).** Confirm price, limits and terms for PTV and FlightAware AeroAPI (AeroDataBox as the fallback) from their own pages, and how many MEL departures one AeroAPI query returns. Record one real sample response per provider as a fixture (the owner supplies keys). *Done when:* a one-page table of verified costs and a fixtures folder.
2. **Phase 1: the `splitflap` crate.** Lift the card, drums, clatter and lamps out of the clock; add digit and word drums, the text fold and the `Board` grid. Gallery example showing a 24-row board. *Done when:* the clock builds on the crate and looks the same in screenshots; the gallery holds 60 fps on a 24-row board under Xvfb.
3. **Phase 2: the board app with the demo feed.** Settings (board, rows, columns, mood), the normaliser, diffing and row shifting, the crowd clatter, the single, stacked and two-window layouts. *Done when:* a simulated day runs at 60× speed with departures leaving, delays, a gate change and a cancellation, all animating correctly.
4. **Phase 3: first live train provider.** PTV, for a Melbourne station, with the key store, station search, polling, cache, stale handling and attribution. *Done when:* the board matches the operator's own website for a station over an hour.
5. **Phase 4: first live flight provider.** FlightAware AeroAPI on the free Personal credit, for MEL, with the polling budget, codeshare cycling and operator tiles. *Done when:* one airport's board matches the airport's website and a month's polling stays inside the free allowance.
6. **Phase 5: release 1.0.** User guide in the clock's style, release workflow for Windows and Linux (Mac later), beer money. *Done when:* both downloads run on clean machines.

Each phase follows the family's checks (`cargo fmt --check`, clippy with `-D warnings` for Linux and Windows, tests) and is looked at under Xvfb before it is pushed.

## Decisions

The owner settled these on 2026-10-30; the sections above follow them.

| Question | Decision |
| --- | --- |
| First real boards | PTV for trains (Victoria, a Melbourne station) and FlightAware AeroAPI for flights (Melbourne Airport, MEL, assumed) |
| Paying for data | The first build runs on free tiers only: PTV is free, AeroAPI's Personal tier gives about $5 of free queries a month. Released users bring and pay for their own keys |
| Where it runs | Desktop window on Windows and Linux; Mac later. No wall screen or Raspberry Pi in 1.0 |
| Repo | [TimsterAudio/Flight-Board](https://github.com/TimsterAudio/Flight-Board) (exists, empty). The `splitflap` crate is a workspace member there |
| Name | Now Boarding, with Flight Board or Train Board as each board's subtitle. May change later; the repo keeps the name Flight-Board for now |
| Layout | Options: one board, or departures and arrivals stacked top and bottom in one window, or each in its own window |

## Kickoff prompt for the new Claude session

Paste this into a new session on the empty `TimsterAudio/Flight-Board` repository, with `TimsterAudio/SplitFlap_Clock` added for reading. Export this doc as Markdown to `docs/BRIEF.md` in the new repo first, so the session can read it.

```markdown
Build Now Boarding (subtitled Flight Board or Train Board), a live airport and railway departures/arrivals board
drawn with split-flap cards. The full brief is docs/BRIEF.md: read it all
first, then the clock's src/flap.rs, src/sound.rs, src/lighting.rs and the
board parts of src/app.rs in TimsterAudio/SplitFlap_Clock.

Work phase by phase as the brief's Build plan says, starting with Phase 0
and Phase 1. Stop at the end of each phase, show me screenshots taken under
Xvfb, and wait for my go before the next one.

Rules:
- Rust, egui/eframe 0.36 like the clock; Windows and Linux (Mac later).
- First real boards: PTV (a Melbourne station) and FlightAware
  AeroAPI (MEL), on free tiers only.
- Network code only in the board app, on its own thread; ureq + rustls.
- No API key in the code or the repo. Keys come from Settings and live in
  the system key store.
- The demo feed must always work with no key and no network.
- No scraping of airport or operator websites; no airline logos.
- Before each push: cargo fmt --check, cargo clippy --all-targets
  -- -D warnings (Linux and x86_64-pc-windows-gnu), cargo test.
- Ask me the brief's open questions that block the phase you're on.
```
