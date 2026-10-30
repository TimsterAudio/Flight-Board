# Data sources: costs and limits

Phase 0 of the [brief](BRIEF.md). This is the table to be checked against
each provider's own pages before money or keys are involved.

**Status, 2026-10-30: not yet verified.** This session's network policy blocks
every provider site (flightaware.com, ptv.vic.gov.au,
timetableapi.ptv.vic.gov.au, data.vic.gov.au, aerodatabox.com, api.market,
rapidapi.com), so no figure below has been read from its source. The
"Brief says" figures come from searches on 2026-10-30; "From memory" ones come
from what the build session knew beforehand. Each still needs checking on the
page in the last column. Fill in "Checked" once it has been.

## PTV Timetable API (Victoria): trains

| What | Brief says / from memory | Checked | Where to check |
| --- | --- | --- | --- |
| Cost | Free | | ptv.vic.gov.au: Footer, Data and reporting, Datasets, PTV Timetable API |
| Getting a key | Email PTV for a developer ID and key (from memory: subject "PTV Timetable API – request for key") | | Same page |
| Licence and attribution | Creative Commons Attribution 4.0 (from memory); credit "Public Transport Victoria" | | Same page; the data.vic.gov.au listing |
| Rate limits | None published (from memory); poll gently anyway | | Same page |
| Signing | HMAC-SHA1 of path and query with `devid`, keyed by the API key, upper-case hex in `signature` | | timetableapi.ptv.vic.gov.au/swagger/ui/index |
| Departures | `/v3/departures/route_type/{0 metro, 3 V/Line}/stop/{id}`: scheduled and estimated UTC times, platform, run, route, direction, disruptions | | Swagger page |
| Arrivals | No arrivals endpoint (from memory); arrivals must be built from runs' stopping patterns | | Swagger page |
| Southern Cross stop id | 1181 for metro trains (from memory); V/Line has its own id | | `/v3/search/Southern%20Cross` (the fixture script records it) |

## FlightAware AeroAPI v4: flights

| What | Brief says / from memory | Checked | Where to check |
| --- | --- | --- | --- |
| Personal tier | No monthly fee; up to $5 of queries free a month; personal and academic use only | | flightaware.com/commercial/aeroapi, Compare plans |
| Billing | Per query, per result set of 15 records | | Same page; the portal's pricing table |
| Cost per airport board query | Not known | | Portal pricing table: `/airports/{id}/flights/scheduled_departures` |
| Rate limit | 10 queries a minute on Personal (a user's report) | | Compare plans |
| Pages | Up to 10 pages per query on Personal (a user's report); `max_pages` sets it | | Documentation |
| Auth | `x-apikey` header; base `https://aeroapi.flightaware.com/aeroapi` | | Documentation |
| Fields a board needs | `ident_iata`, `codeshares_iata`, `operator_iata`, `scheduled_out`, `estimated_out`, `gate_origin`, `terminal_origin`, `status`, `cancelled`, `destination.city` (from memory) | | Documentation, and the fixture |
| MEL departures per query | Not known: how many pages a day's departures take | | The fixture: count records, and look at `links.next` |

## AeroDataBox: the fallback for flights

| What | Brief says | Checked | Where to check |
| --- | --- | --- | --- |
| FIDS endpoint tier | A paid tier | | aerodatabox.com pricing; api.market listing |
| Free allowance | Unclear; one blog says 700 requests a month | | Same |
| Time window per call | Not known (12 hours, from memory) | | doc.aerodatabox.com |

## The polling budget, once checked

The app works out how often to poll from:

- the cost of one board query (pages × price per page),
- the monthly allowance (default $5 for AeroAPI Personal),
- the hours a day the board is open.

For example, if one MEL departures query cost 2 pages at $0.01 a page, $5 a
month would buy 250 queries: about one every 3 hours around the clock, or one
every 45 minutes over 6 hours a day. **These numbers are an example of the
sum, not prices.** The real price per page goes in the table above.

## Fixtures

`tools/record_fixtures.sh` records one response per provider into
`fixtures/`, run on the owner's machine with the keys in environment variables.
It makes two AeroAPI queries of one page each.
