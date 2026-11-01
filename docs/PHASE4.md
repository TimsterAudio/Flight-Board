# Phase 4: flights from FlightAware AeroAPI, for MEL

Flight boards can now come from AeroAPI, with your own key, on a budget
that keeps a month's polling inside what you set.

## Using it

1. Sign up for FlightAware AeroAPI and choose the Personal tier (personal
   use only). Make an API key in its portal.
2. In Settings choose **Flights**, then **FlightAware AeroAPI**. Paste the
   key and press **Save**.
3. Check the price of a page of results on AeroAPI's pricing page and set it
   under **your budget**. The default, $0.02 a page, is a placeholder, not a
   checked price.
4. **Test** asks for one page of departures, so it costs one page.

The airport is Melbourne (YMML) to begin with; Settings finds others by IATA
or ICAO code or by name, from a built-in list, or takes any ICAO code.

## The budget

AeroAPI bills by the page of results (15 flights). The budget is:

- how much a month you'll spend (default $5, the Personal tier's credit),
- what a page costs,
- pages each update, at most (default 1),
- the hours a day the board is open (default 8).

From those, the app works out how often to ask. One board at $5, 2 cents a
page and 8 hours a day comes to about once an hour; departures and arrivals
together, every two hours. It counts every page it pays for, keeps the count
between runs, and once the month's pages are used it stops asking. The board
then says THIS MONTH'S AEROAPI BUDGET IS USED: BACK ON 1 NOV. Settings shows
the pages used this month, and the footer shows when the next update is.

## The board

- **FLIGHT** is now two columns: a 2-card **airline card** and the number.
  Codeshares take turns every 8 seconds, the card changing with them.
- **Airline colour cards**: the airline's code on a card in its main colour,
  from a built-in table of about 40 airlines (and the demo's made-up ones).
  Settings can switch them off or set any airline's colour.
- **Logos, for personal use**: put PNG pictures in the logos folder, named by
  the airline's code (`QF.png`). That airline's card then shows the picture
  once it has landed, on a light card with the split across it. The app ships
  no logos and fetches none. Settings shows the folder, opens it, and looks
  again after you add some.
- Departures show ON TIME, DELAYED (15 minutes or more), CANCELLED and
  DEPARTED. Arrivals show ON TIME, EXPECTED (within 30 minutes), DELAYED,
  LANDED, ARRIVED, CANCELLED and DIVERTED. AeroAPI tracks flights rather
  than gates, so BOARDING and FINAL CALL aren't made up from the time.

## Not yet checked against the real thing

As with PTV: AeroAPI's field names come from its API as known, read
leniently, and tested against hand-written answers in
`crates/now-boarding/testdata/`. Running `tools/record_fixtures.sh` with your
key records a real one; then the tests read that, and any field that differs
gets fixed. The brief's test for this phase: the board beside MEL's own
website, and a month's polling inside the free allowance.

The screenshots come from a stand-in server with made-up flights. In a debug
build only, `NOW_BOARDING_AEROAPI_BASE` and `NOW_BOARDING_AEROAPI_KEY` set
it up; a release build ignores them.

## Terminals, and any airport's own time

- **Terminals**: each board can show all terminals or one (Settings,
  Terminals). The terminals offered are those named in the flights the board
  has. A flight with no terminal given shows only on All. FlightAware sends
  the whole airport, 15 flights a page, so a one-terminal board asks a page
  at a time until it has enough of its terminal's flights to fill it, up to
  the pages each update set in the budget (at a busy airport such as
  Heathrow, 4 or 5), or until there are no more. The budget spaces the
  updates out as if every page were used. Changing a board's terminal takes effect
  at once, without asking FlightAware again.
- **Two boards of the same kind**: with two boards (stacked, or a window
  each), the second can be arrivals or a second departures board: T1
  departures beside T2 departures, say.
- **Any airport's own time**: FlightAware gives each airport's time zone with
  its flights. The board uses it, and keeps it, so an airport given only by
  its ICAO code shows its own local time and its name, not the computer's.

## Paying less: one download for boards that share it

- **Each update fills the board for a while.** A board asks for enough
  flights to fill every page it turns through, and one page more. Between
  updates it carries on by itself: flights that have left drop off and the
  rest move up, so updates can be far apart without the board running dry.
- **Boards of the same airport and direction share one download** (say T1
  and T2 departures). It asks a page at a time until every board has
  enough of its own terminal's flights, then gives the answer to them all.
  The budget counts downloads, not boards, so two terminal boards update as
  often as one.
