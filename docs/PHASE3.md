# Phase 3: PTV, for Southern Cross

Train boards can now come from Public Transport Victoria's Timetable API,
with your own free developer ID and key.

## Using it

1. Ask PTV for a Timetable API key. How to ask is on ptv.vic.gov.au, under
   Data and reporting, Datasets, PTV Timetable API. They send a developer ID
   and a key.
2. In Now Boarding's Settings, choose **Trains**, then **PTV: Victoria's
   trains**.
3. Paste the developer ID and key, press **Save**, then **Test**.
4. Search for **Southern Cross** and choose it.

The board asks PTV once a minute and shows departures. PTV gives departures
only, so with PTV the board is a departures board whatever the layout.

## What's in it

- **`ptv.rs`**: signed requests (HMAC-SHA1 of the path and query, `devid`
  included, as `signature` in upper-case hex). Station search puts a
  station's metro and V/Line stops together, and the board asks for the
  departures from each. Each poll adds one more request: the next train's
  stopping pattern, for the ticker's "calling at". Current disruptions go to
  the ticker too. One stop failing still gives a board from the other.
- **`http.rs`**: the app's only way onto the network: ureq with rustls, a
  20-second timeout, and nothing sent but the request. 429 becomes "busy"
  (honouring Retry-After), and the poller backs off; 401 or 403 says to check
  the key.
- **`keys.rs`**: the developer ID and key live in the system key store:
  GNOME Keyring or KWallet on Linux, Credential Manager on Windows. They're
  never in the settings file, the cache, the repo or a URL (only the
  signature goes in a URL).
- **`cache.rs`**: the last good board is kept in the system's cache folder
  (`~/.cache/now-boarding` on Linux). A restart shows it at once, and with no
  network it stays up with its time in the footer. After two failed updates
  the footer says LAST UPDATED and why; after 30 minutes the board shows SEE
  SCREENS.
- **Settings**: the feed, the key (Save, Test, Forget), station search, and
  the credit to PTV under Creative Commons Attribution 4.0. The footer
  carries the credit on the board.
- **No key yet**: the board says PTV NEEDS YOUR KEY: SEE SETTINGS, and asks
  nothing of the network.

## Not yet checked against the real thing

The field names PTV's answers are read by come from its API as it was known.
They haven't been seen in a real answer yet: the provider sites are closed to
this build session. The fields are read leniently, so a different name leaves
a blank rather than an empty board. The tests use hand-written answers in
`crates/now-boarding/testdata/`.

Next, with your key:

1. Run `tools/record_fixtures.sh` and commit `fixtures/ptv/`: the tests then
   read a real answer, and any field that differs gets fixed.
2. Watch the board beside PTV's own departures for Southern Cross for an
   hour: the brief's test for this phase.

The screenshots for this phase come from a stand-in server playing PTV with
made-up departures. In a debug build only, it's set by
`NOW_BOARDING_PTV_BASE`, with `NOW_BOARDING_PTV_DEVID` and
`NOW_BOARDING_PTV_KEY` in place of a key store. A release build ignores all
three.

## Building on Linux

The key store needs D-Bus's headers to build: `sudo apt install
libdbus-1-dev pkg-config`, with `libasound2-dev` as before.
