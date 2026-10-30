#!/usr/bin/env bash
# Records one real response from each live provider into fixtures/, for the
# providers' tests. Run it on your own machine: the keys come from the
# environment and are never written anywhere.
#
#   PTV_DEVID=...  PTV_KEY=...  AEROAPI_KEY=...  tools/record_fixtures.sh
#
# Either provider can be left out by leaving its variables unset.
#
# What it costs: PTV is free. AeroAPI is billed per query and per page of
# results; this asks for one page each of MEL's scheduled departures and
# scheduled arrivals, two queries in all.
#
# Optional: PTV_STOP (the stop id of Southern Cross, metro trains; found by
# the search response this records), AIRPORT (default YMML, Melbourne).

set -euo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
out="$here/fixtures"
mkdir -p "$out/ptv" "$out/aeroapi"
stamp="$(date -u +%Y-%m-%dT%H%MZ)"

# Pretty-prints JSON if python3 is there, so diffs of fixtures read well.
tidy() {
    if command -v python3 >/dev/null; then
        python3 -m json.tool --indent 2
    else
        cat
    fi
}

# PTV signs each request: an HMAC-SHA1 of the path and query (devid
# included), keyed with the API key, as upper-case hex.
ptv() {
    local path="$1"
    local sep='?'
    [[ "$path" == *\?* ]] && sep='&'
    local signed="${path}${sep}devid=${PTV_DEVID}"
    local sig
    sig="$(printf '%s' "$signed" | openssl dgst -sha1 -hmac "$PTV_KEY" -r | cut -d' ' -f1 | tr 'a-f' 'A-F')"
    curl -sS --fail-with-body "https://timetableapi.ptv.vic.gov.au${signed}&signature=${sig}"
}

if [[ -n "${PTV_DEVID:-}" && -n "${PTV_KEY:-}" ]]; then
    echo "PTV: searching for Southern Cross"
    ptv "/v3/search/Southern%20Cross?route_types=0&route_types=3" | tidy >"$out/ptv/search-southern-cross-$stamp.json"
    stop="${PTV_STOP:-1181}"
    echo "PTV: departures from stop $stop (metro trains)"
    ptv "/v3/departures/route_type/0/stop/$stop?max_results=5&expand=Run&expand=Route&expand=Direction&expand=Stop&expand=Disruption&include_cancelled=true" \
        | tidy >"$out/ptv/departures-metro-$stop-$stamp.json"
    echo "PTV: departures from stop $stop (V/Line), if it is a V/Line stop too"
    ptv "/v3/departures/route_type/3/stop/$stop?max_results=5&expand=Run&expand=Route&expand=Direction&include_cancelled=true" \
        | tidy >"$out/ptv/departures-vline-$stop-$stamp.json" || echo "  (none: V/Line uses its own stop id; see the search response)"
else
    echo "PTV: skipped (set PTV_DEVID and PTV_KEY)"
fi

if [[ -n "${AEROAPI_KEY:-}" ]]; then
    airport="${AIRPORT:-YMML}"
    for kind in scheduled_departures scheduled_arrivals; do
        echo "AeroAPI: $airport $kind, one page"
        curl -sS --fail-with-body -H "x-apikey: $AEROAPI_KEY" \
            "https://aeroapi.flightaware.com/aeroapi/airports/$airport/flights/$kind?max_pages=1" \
            | tidy >"$out/aeroapi/$kind-$airport-$stamp.json"
    done
else
    echo "AeroAPI: skipped (set AEROAPI_KEY)"
fi

echo
echo "Written to $out. Look them over before committing: they should hold"
echo "timetable data only. No request URL or key is saved."
