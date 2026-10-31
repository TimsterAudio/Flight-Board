//! PTV's Timetable API (version 3): Victoria's trains, metro and V/Line.
//!
//! Every request is signed: an HMAC-SHA1 of its path and query, `devid`
//! included, keyed with the developer's API key, sent as `signature` in
//! upper-case hex. A board is the departures from each of the station's
//! stops (metro and V/Line have stops of their own at the same station),
//! with the next train's stopping pattern for its calling points, and the
//! line's disruptions for the ticker.
//!
//! The field names follow PTV's API as it was known when this was written;
//! the fields are read leniently, so a missing one leaves a blank rather
//! than an empty board. Check them against a recorded response in
//! `fixtures/ptv/` (see `tools/record_fixtures.sh`).

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha1::Sha1;

use crate::http::Fetch;
use crate::model::{BoardQuery, Direction, Mode, Movement, Place, Status};
use crate::provider::{Answer, Error, Provider, Result};

pub const BASE: &str = "https://timetableapi.ptv.vic.gov.au";
pub const ATTRIBUTION: &str = "DATA: PUBLIC TRANSPORT VICTORIA, CC BY 4.0";
/// How often a board asks: once a minute is plenty for trains, and gentle
/// on a free service.
pub const EVERY: Duration = Duration::from_secs(60);
const TZ: Tz = chrono_tz::Australia::Melbourne;

/// Metro trains and V/Line, PTV's route types 0 and 3.
const METRO: u32 = 0;
const VLINE: u32 = 3;

/// A developer's credentials. Never written anywhere but the system key
/// store.
#[derive(Clone)]
pub struct Keys {
    pub devid: String,
    pub key: String,
}

impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Keys {{ devid: {}, key: <hidden> }}", self.devid)
    }
}

pub struct Ptv {
    keys: Keys,
    base: String,
    fetch: Box<dyn Fetch>,
}

/// `path_and_query` signed with `keys`: the path to ask for.
pub fn sign(path_and_query: &str, keys: &Keys) -> String {
    let sep = if path_and_query.contains('?') {
        '&'
    } else {
        '?'
    };
    let with_id = format!("{path_and_query}{sep}devid={}", keys.devid);
    let mut mac =
        Hmac::<Sha1>::new_from_slice(keys.key.as_bytes()).expect("HMAC takes a key of any length");
    mac.update(with_id.as_bytes());
    let sig: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect();
    format!("{with_id}&signature={sig}")
}

/// A place's stops, from its id: `ptv:0:1181,3:1544` is metro stop 1181
/// and V/Line stop 1544.
pub fn stops(place: &Place) -> Vec<(u32, u32)> {
    place
        .id
        .strip_prefix("ptv:")
        .unwrap_or("")
        .split(',')
        .filter_map(|s| {
            let (t, id) = s.split_once(':')?;
            Some((t.parse().ok()?, id.parse().ok()?))
        })
        .collect()
}

fn place_id(stops: &[(u32, u32)]) -> String {
    let parts: Vec<String> = stops.iter().map(|(t, s)| format!("{t}:{s}")).collect();
    format!("ptv:{}", parts.join(","))
}

/// A stop's name as a board says it: SOUTHERN CROSS, not SOUTHERN CROSS
/// RAILWAY STATION.
pub fn station_name(name: &str) -> String {
    // Some stops carry their suburb after a slash: "Flinders Street/Melbourne".
    let mut n = name.split('/').next().unwrap_or(name).trim();
    for tail in [" Railway Station", " Station"] {
        if let Some(s) = n.strip_suffix(tail) {
            n = s;
        }
    }
    n.trim().to_string()
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SearchStop {
    stop_id: u32,
    stop_name: String,
    route_type: u32,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SearchAnswer {
    stops: Vec<SearchStop>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Departure {
    stop_id: u32,
    route_id: u32,
    run_ref: String,
    direction_id: i64,
    disruption_ids: Vec<i64>,
    scheduled_departure_utc: Option<DateTime<Utc>>,
    estimated_departure_utc: Option<DateTime<Utc>>,
    platform_number: Option<String>,
    flags: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Run {
    destination_name: Option<String>,
    status: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Route {
    route_name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct DirectionInfo {
    direction_name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Disruption {
    title: String,
    disruption_status: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Departures {
    departures: Vec<Departure>,
    runs: HashMap<String, Run>,
    routes: HashMap<String, Route>,
    directions: HashMap<String, DirectionInfo>,
    disruptions: HashMap<String, Disruption>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PatternStop {
    stop_name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Pattern {
    departures: Vec<Departure>,
    stops: HashMap<String, PatternStop>,
}

fn parse<T: for<'a> Deserialize<'a>>(body: &str) -> Result<T> {
    serde_json::from_str(body).map_err(|e| Error::Failed(format!("PTV's answer didn't read: {e}")))
}

impl Ptv {
    pub fn new(keys: Keys, fetch: Box<dyn Fetch>) -> Self {
        Self {
            keys,
            base: BASE.into(),
            fetch,
        }
    }

    /// Asks somewhere else than PTV, such as a stand-in server for trying
    /// the app out.
    pub fn with_base(mut self, base: &str) -> Self {
        self.base = base.trim_end_matches('/').to_string();
        self
    }

    fn get(&self, path_and_query: &str) -> Result<String> {
        let url = format!("{}{}", self.base, sign(path_and_query, &self.keys));
        self.fetch.get(&url, &[])
    }

    /// The departures from one stop.
    fn departures(&self, route_type: u32, stop: u32) -> Result<Departures> {
        let path = format!(
            "/v3/departures/route_type/{route_type}/stop/{stop}\
             ?max_results=6&include_cancelled=true\
             &expand=Run&expand=Route&expand=Direction&expand=Disruption"
        );
        parse(&self.get(&path)?)
    }

    /// The stops a run calls at after `stop`, in order, the last being
    /// where it ends.
    fn calling_points(&self, route_type: u32, run_ref: &str, stop: u32) -> Result<Vec<String>> {
        let path =
            format!("/v3/pattern/run/{run_ref}/route_type/{route_type}?expand=Stop&stop_id={stop}");
        let p: Pattern = parse(&self.get(&path)?)?;
        let mut deps = p.departures;
        deps.sort_by_key(|d| d.scheduled_departure_utc);
        let from = deps
            .iter()
            .position(|d| d.stop_id == stop)
            .map_or(0, |i| i + 1);
        Ok(deps[from..]
            .iter()
            .filter_map(|d| p.stops.get(&d.stop_id.to_string()))
            .map(|s| station_name(&s.stop_name))
            .collect())
    }
}

/// One departure as a movement, with what the answer it came in says about
/// its run, route and direction.
fn movement(
    d: &Departure,
    route_type: u32,
    a: &Departures,
    now: DateTime<Utc>,
) -> Option<Movement> {
    let scheduled = d.scheduled_departure_utc?;
    let run = a.runs.get(&d.run_ref);
    let cancelled = run
        .and_then(|r| r.status.as_deref())
        .is_some_and(|s| s.eq_ignore_ascii_case("cancelled"))
        || d.flags.split('_').any(|f| f == "CAN");
    let place = run
        .and_then(|r| r.destination_name.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            a.directions
                .get(&d.direction_id.to_string())
                .map(|x| x.direction_name.clone())
        })
        .or_else(|| {
            a.routes
                .get(&d.route_id.to_string())
                .map(|r| r.route_name.clone())
        })
        .unwrap_or_default();
    let expected = d.estimated_departure_utc;
    let at = expected.unwrap_or(scheduled);
    let late = expected.is_some_and(|e| e - scheduled >= chrono::Duration::minutes(1));
    let status = if cancelled {
        Status::Cancelled
    } else if at <= now {
        Status::Departed
    } else if late {
        Status::Delayed
    } else {
        Status::OnTime
    };
    Some(Movement {
        id: format!("{route_type}:{}:{}", d.run_ref, scheduled.timestamp()),
        mode: Mode::Train,
        direction: Direction::Departure,
        scheduled: scheduled.with_timezone(&TZ),
        expected: expected.map(|e| e.with_timezone(&TZ)),
        numbers: Vec::new(),
        operator: if route_type == VLINE {
            "V/LINE"
        } else {
            "METRO"
        }
        .into(),
        place: station_name(&place),
        via: Vec::new(),
        stand: d.platform_number.clone().filter(|p| !p.trim().is_empty()),
        terminal: None,
        status,
    })
}

impl Provider for Ptv {
    fn name(&self) -> &str {
        "PTV"
    }

    fn attribution(&self) -> &str {
        ATTRIBUTION
    }

    fn min_interval(&self) -> Duration {
        Duration::from_secs(30)
    }

    /// Stations matching `text`, each with its metro and V/Line stops
    /// together.
    fn search(&self, text: &str) -> Result<Vec<Place>> {
        let term: String = text
            .trim()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c.to_string()
                } else {
                    "%20".into()
                }
            })
            .collect();
        if term.is_empty() {
            return Ok(Vec::new());
        }
        let path = format!("/v3/search/{term}?route_types={METRO}&route_types={VLINE}");
        let found: SearchAnswer = parse(&self.get(&path)?)?;
        let mut places: Vec<(String, Vec<(u32, u32)>)> = Vec::new();
        for s in found
            .stops
            .iter()
            .filter(|s| s.route_type == METRO || s.route_type == VLINE)
        {
            let name = station_name(&s.stop_name);
            match places.iter_mut().find(|(n, _)| *n == name) {
                Some((_, stops)) => {
                    if !stops.contains(&(s.route_type, s.stop_id)) {
                        stops.push((s.route_type, s.stop_id));
                    }
                }
                None => places.push((name, vec![(s.route_type, s.stop_id)])),
            }
        }
        Ok(places
            .into_iter()
            .map(|(name, stops)| Place {
                id: place_id(&stops),
                name,
                tz: TZ.name().into(),
            })
            .collect())
    }

    fn board(&self, q: &BoardQuery) -> Result<Answer> {
        if q.direction == Direction::Arrival {
            return Err(Error::Failed("PTV gives departures only".into()));
        }
        let stops = stops(&q.place);
        if stops.is_empty() {
            return Err(Error::Failed("choose a station in Settings".into()));
        }
        let mut out = Answer::default();
        let mut first_failure = None;
        let mut answered = false;
        for &(route_type, stop) in &stops {
            match self.departures(route_type, stop) {
                Ok(a) => {
                    answered = true;
                    for d in &a.departures {
                        if let Some(m) = movement(d, route_type, &a, q.now) {
                            out.movements.push(m);
                        }
                    }
                    let mut ids: Vec<i64> = a
                        .departures
                        .iter()
                        .flat_map(|d| d.disruption_ids.clone())
                        .collect();
                    ids.sort_unstable();
                    ids.dedup();
                    for id in ids {
                        if let Some(d) = a.disruptions.get(&id.to_string()) {
                            let current = d
                                .disruption_status
                                .as_deref()
                                .is_none_or(|s| s.eq_ignore_ascii_case("current"));
                            if current && !d.title.is_empty() && !out.notices.contains(&d.title) {
                                out.notices.push(d.title.clone());
                            }
                        }
                    }
                }
                Err(e) => {
                    first_failure.get_or_insert(e);
                }
            }
        }
        if !answered {
            return Err(first_failure.unwrap_or(Error::Failed("no answer from PTV".into())));
        }
        out.movements.sort_by_key(|m| m.scheduled);
        // The next train's calling points, for the ticker: one more request.
        if let Some(i) = out.movements.iter().position(|m| !m.status.finished()) {
            let m = &out.movements[i];
            let mut parts = m.id.splitn(3, ':');
            let route_type: u32 = parts.next().and_then(|t| t.parse().ok()).unwrap_or(METRO);
            let run_ref = parts.next().unwrap_or("").to_string();
            let stop = stops
                .iter()
                .find(|(t, _)| *t == route_type)
                .map_or(0, |(_, s)| *s);
            if !run_ref.is_empty() {
                if let Ok(mut via) = self.calling_points(route_type, &run_ref, stop) {
                    // The last stop is where it's going, already on the board.
                    via.pop();
                    out.movements[i].via = via;
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;
    use chrono::TimeZone;

    const DEPARTURES: &str = include_str!("../testdata/ptv-departures-handmade.json");
    const PATTERN: &str = include_str!("../testdata/ptv-pattern-handmade.json");
    const SEARCH: &str = include_str!("../testdata/ptv-search-handmade.json");

    fn keys() -> Keys {
        Keys {
            devid: "1234567".into(),
            key: "9c132d31-6a30-4cac-8d8b-8a1970834799".into(),
        }
    }

    fn southern_cross() -> Place {
        Place {
            id: "ptv:0:1181,3:1544".into(),
            name: "Southern Cross".into(),
            tz: "Australia/Melbourne".into(),
        }
    }

    fn query(now: DateTime<Utc>) -> BoardQuery {
        BoardQuery {
            place: southern_cross(),
            mode: Mode::Train,
            direction: Direction::Departure,
            now,
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 30, 7, 0, 0).unwrap()
    }

    #[test]
    fn requests_are_signed_with_the_devid_first() {
        let k = keys();
        let signed = sign("/v3/routes", &k);
        assert!(signed.starts_with("/v3/routes?devid=1234567&signature="));
        let sig = signed.rsplit('=').next().unwrap();
        assert_eq!(sig.len(), 40);
        assert!(sig
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()));
        // A query already there takes devid after an ampersand.
        assert!(sign("/v3/search/x?route_types=0", &k)
            .contains("route_types=0&devid=1234567&signature="));
    }

    #[test]
    fn the_signature_is_hmac_sha1_of_the_path_and_query() {
        let k = Keys {
            devid: "1".into(),
            key: "key".into(),
        };
        assert_eq!(
            sign("/v3/routes", &k),
            format!(
                "/v3/routes?devid=1&signature={}",
                expected_hmac("key", "/v3/routes?devid=1")
            )
        );
    }

    /// HMAC-SHA1 by its definition, from SHA-1 alone, to check the crate's.
    fn expected_hmac(key: &str, msg: &str) -> String {
        use sha1::Digest;
        let mut k = key.as_bytes().to_vec();
        k.resize(64, 0);
        let ipad: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
        let opad: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
        let inner = Sha1::new().chain_update(&ipad).chain_update(msg).finalize();
        let outer = Sha1::new()
            .chain_update(&opad)
            .chain_update(inner)
            .finalize();
        outer.iter().map(|b| format!("{b:02X}")).collect()
    }

    #[test]
    fn a_station_is_its_stops() {
        assert_eq!(stops(&southern_cross()), [(0, 1181), (3, 1544)]);
        assert_eq!(
            station_name("Southern Cross Railway Station"),
            "Southern Cross"
        );
        assert_eq!(
            station_name("Flinders Street Station/Melbourne City"),
            "Flinders Street"
        );
    }

    #[test]
    fn search_puts_a_stations_stops_together() {
        let ptv = Ptv::new(
            keys(),
            Box::new(Fake::default().with("/v3/search/", SEARCH)),
        );
        let found = ptv.search("Southern Cross").unwrap();
        assert_eq!(found[0].name, "Southern Cross");
        assert_eq!(stops(&found[0]), [(0, 1181), (3, 1544)]);
        // Bus stops are left out.
        assert!(found.iter().all(|p| !stops(p).is_empty()));
    }

    #[test]
    fn a_board_reads_departures_from_every_stop() {
        let fake = Fake::default()
            .with("/v3/departures/route_type/0/stop/1181", DEPARTURES)
            .with(
                "/v3/departures/route_type/3/stop/1544",
                r#"{"departures":[]}"#,
            )
            .with("/v3/pattern/", PATTERN);
        let ptv = Ptv::new(keys(), Box::new(fake));
        let a = ptv.board(&query(now())).unwrap();
        let ms = &a.movements;
        assert_eq!(ms.len(), 4);
        // In time order, in Melbourne's time.
        assert_eq!(ms[0].scheduled.format("%H:%M").to_string(), "18:05");
        assert_eq!(ms[0].place, "Frankston");
        assert_eq!(ms[0].stand.as_deref(), Some("12"));
        assert_eq!(ms[0].operator, "METRO");
        // Three minutes late is late.
        assert_eq!(ms[1].status, Status::Delayed);
        assert_eq!(ms[1].place, "Werribee");
        // A cancelled run says so.
        assert_eq!(ms[2].status, Status::Cancelled);
        // The first train calls at its pattern's stops after this one.
        assert_eq!(ms[0].via, ["Flinders Street", "Richmond", "Caulfield"]);
        // The disruption goes to the ticker.
        assert_eq!(
            a.notices,
            ["Buses replace trains between Caulfield and Frankston after 21:00"]
        );
    }

    #[test]
    fn one_stop_failing_still_gives_a_board() {
        let fake = Fake::default()
            .with("/v3/departures/route_type/0/stop/1181", DEPARTURES)
            .failing(
                "/v3/departures/route_type/3/",
                Error::Failed("the provider answered 500".into()),
            );
        let ptv = Ptv::new(keys(), Box::new(fake));
        assert_eq!(ptv.board(&query(now())).unwrap().movements.len(), 4);
    }

    #[test]
    fn busy_and_refused_pass_through() {
        let fake = Fake::default().failing("/v3/", Error::Busy(Duration::from_secs(90)));
        let ptv = Ptv::new(keys(), Box::new(fake));
        assert_eq!(
            ptv.board(&query(now())).unwrap_err(),
            Error::Busy(Duration::from_secs(90))
        );
    }

    #[test]
    fn requests_carry_the_devid_and_a_signature_never_the_key() {
        let fake =
            std::sync::Arc::new(Fake::default().with("/v3/departures/", r#"{"departures":[]}"#));
        let ptv = Ptv::new(keys(), Box::new(fake.clone()));
        ptv.board(&query(now())).unwrap();
        let asked = fake.asked.lock().unwrap().clone();
        assert_eq!(
            asked.len(),
            2,
            "one request for each of the station's stops"
        );
        assert!(asked[0].starts_with(
            "https://timetableapi.ptv.vic.gov.au/v3/departures/route_type/0/stop/1181?"
        ));
        assert!(asked[0].contains("devid=1234567&signature="));
        assert!(asked.iter().all(|u| !u.contains(&keys().key)));
    }
}
