# Fixtures

One real response per provider, recorded by `tools/record_fixtures.sh` on the
owner's machine with the owner's keys, for the providers' tests. Responses only:
no request URLs, keys or signatures are saved.

- `ptv/`: PTV Timetable API v3, Southern Cross (search, metro and V/Line
  departures).
- `aeroapi/`: FlightAware AeroAPI v4, Melbourne Airport (YMML) scheduled
  departures and arrivals, one page each.

The demo feed doesn't use these; it makes its own day of movements.
