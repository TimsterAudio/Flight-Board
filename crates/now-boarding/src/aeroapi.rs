//! FlightAware AeroAPI (version 4): an airport's scheduled departures and
//! arrivals, with gates, terminals, codeshares and delays.
//!
//! AeroAPI bills per page of results, so every board shares one budget:
//! how much a month the owner will spend, and what a page costs. The poll
//! interval is worked out from it, and once a month's budget is spent the
//! board stops asking until the next month.
//!
//! The field names follow AeroAPI as it was known when this was written,
//! read leniently. Check them against a recorded answer in
//! `fixtures/aeroapi/` (see `tools/record_fixtures.sh`).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Datelike, Duration as Span, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::airlines;
use crate::http::Fetch;
use crate::model::{BoardQuery, Direction, Mode, Movement, Place, Status};
use crate::provider::{Answer, Error, Provider, Result};

pub const BASE: &str = "https://aeroapi.flightaware.com/aeroapi";
pub const ATTRIBUTION: &str = "DATA: FLIGHTAWARE AEROAPI";
/// Late enough to call late.
const LATE: i64 = 15;

/// What the owner will spend on AeroAPI, and what it costs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Budget {
    /// Dollars a month: AeroAPI's Personal tier's free credit by default.
    pub monthly: f64,
    /// Dollars a page of results. Check it on AeroAPI's pricing page.
    pub per_page: f64,
    /// Pages a poll asks for: 15 flights a page.
    pub pages: u32,
    /// Hours a day the board is open, so the budget is spread over them.
    pub hours: f64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            monthly: 5.0,
            per_page: 0.02,
            pages: 1,
            hours: 8.0,
        }
    }
}

impl Budget {
    /// Pages the month's money buys.
    pub fn pages_a_month(&self) -> u32 {
        (self.monthly / self.per_page.max(0.0001)).floor() as u32
    }

    /// How long to wait between polls so `boards` boards, open the hours
    /// set, stay inside the budget over a 31-day month: never less than two
    /// minutes.
    pub fn interval(&self, boards: u32) -> Duration {
        let per_poll = (self.pages.max(1) * boards.max(1)) as f64;
        let polls = (self.pages_a_month() as f64 / per_poll).max(1.0);
        let open = 31.0 * self.hours.clamp(0.5, 24.0) * 3600.0;
        Duration::from_secs_f64((open / polls).max(120.0))
    }
}

/// Pages paid for this month, kept between runs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spent {
    /// `2026-10`, the month counted.
    pub month: String,
    pub pages: u32,
}

impl Spent {
    fn month_of(t: DateTime<Utc>) -> String {
        format!("{:04}-{:02}", t.year(), t.month())
    }

    /// Pages paid for in `now`'s month.
    pub fn this_month(&self, now: DateTime<Utc>) -> u32 {
        if self.month == Self::month_of(now) {
            self.pages
        } else {
            0
        }
    }

    fn add(&mut self, now: DateTime<Utc>, pages: u32) {
        let m = Self::month_of(now);
        if self.month != m {
            self.month = m;
            self.pages = 0;
        }
        self.pages += pages;
    }
}

pub struct AeroApi {
    key: String,
    base: String,
    budget: Budget,
    spent: Arc<Mutex<Spent>>,
    fetch: Box<dyn Fetch>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Airport {
    code_iata: Option<String>,
    name: Option<String>,
    city: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Flight {
    ident: String,
    ident_iata: Option<String>,
    fa_flight_id: String,
    operator_iata: Option<String>,
    codeshares_iata: Vec<String>,
    cancelled: bool,
    diverted: bool,
    origin: Option<Airport>,
    destination: Option<Airport>,
    scheduled_out: Option<DateTime<Utc>>,
    estimated_out: Option<DateTime<Utc>>,
    actual_out: Option<DateTime<Utc>>,
    scheduled_in: Option<DateTime<Utc>>,
    estimated_in: Option<DateTime<Utc>>,
    actual_in: Option<DateTime<Utc>>,
    actual_on: Option<DateTime<Utc>>,
    gate_origin: Option<String>,
    terminal_origin: Option<String>,
    terminal_destination: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Flights {
    scheduled_departures: Vec<Flight>,
    scheduled_arrivals: Vec<Flight>,
    num_pages: Option<u32>,
}

/// `QF401` as a board writes it: `QF 401`.
fn spaced(ident: &str) -> String {
    let i = ident.trim();
    if i.contains(' ') || i.len() < 3 {
        return i.to_string();
    }
    format!("{} {}", airlines::code_of(i), airlines::number_of(i))
}

/// A place as a board names it: the city, or the airport's name.
fn named(a: &Option<Airport>) -> String {
    a.as_ref()
        .and_then(|a| {
            a.city
                .clone()
                .filter(|c| !c.is_empty())
                .or_else(|| a.name.clone())
                .or_else(|| a.code_iata.clone())
        })
        .unwrap_or_default()
}

fn movement(f: &Flight, direction: Direction, tz: Tz, now: DateTime<Utc>) -> Option<Movement> {
    let (scheduled, expected, done) = match direction {
        Direction::Departure => (
            f.scheduled_out?,
            f.actual_out.or(f.estimated_out),
            f.actual_out.is_some(),
        ),
        Direction::Arrival => (
            f.scheduled_in?,
            f.actual_in.or(f.estimated_in),
            f.actual_in.is_some(),
        ),
    };
    let late = expected.is_some_and(|e| e - scheduled >= Span::minutes(LATE));
    let status = if f.cancelled {
        Status::Cancelled
    } else if f.diverted {
        Status::Diverted
    } else {
        match direction {
            Direction::Departure if done => Status::Departed,
            Direction::Arrival if done => Status::Arrived,
            Direction::Arrival if f.actual_on.is_some() => Status::Landed,
            _ if late => Status::Delayed,
            Direction::Arrival if expected.is_some_and(|e| e - now <= Span::minutes(30)) => {
                Status::Expected
            }
            _ => Status::OnTime,
        }
    };
    let mut numbers = vec![spaced(f.ident_iata.as_deref().unwrap_or(&f.ident))];
    numbers.extend(f.codeshares_iata.iter().map(|c| spaced(c)));
    numbers.dedup();
    let (place, stand, terminal) = match direction {
        Direction::Departure => (
            named(&f.destination),
            f.gate_origin.clone(),
            f.terminal_origin.clone(),
        ),
        Direction::Arrival => (named(&f.origin), None, f.terminal_destination.clone()),
    };
    Some(Movement {
        id: if f.fa_flight_id.is_empty() {
            format!("{}-{}", f.ident, scheduled.timestamp())
        } else {
            f.fa_flight_id.clone()
        },
        mode: Mode::Flight,
        direction,
        scheduled: scheduled.with_timezone(&tz),
        expected: expected.map(|e| e.with_timezone(&tz)),
        operator: f
            .operator_iata
            .clone()
            .unwrap_or_else(|| airlines::code_of(&numbers[0])),
        numbers,
        place,
        via: Vec::new(),
        stand: stand.filter(|s| !s.trim().is_empty()),
        terminal,
        status,
    })
}

impl AeroApi {
    pub fn new(
        key: String,
        budget: Budget,
        spent: Arc<Mutex<Spent>>,
        fetch: Box<dyn Fetch>,
    ) -> Self {
        Self {
            key,
            base: BASE.into(),
            budget,
            spent,
            fetch,
        }
    }

    /// Asks somewhere else than FlightAware, such as a stand-in server.
    // Only debug builds' stand-in servers use this.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub fn with_base(mut self, base: &str) -> Self {
        self.base = base.trim_end_matches('/').to_string();
        self
    }
}

/// The first day of the month after `now`'s, for the footer.
fn next_month(now: DateTime<Utc>) -> String {
    let month = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    format!("1 {}", month[now.month() as usize % 12])
}

impl Provider for AeroApi {
    fn name(&self) -> &str {
        "AeroAPI"
    }

    fn attribution(&self) -> &str {
        ATTRIBUTION
    }

    fn min_interval(&self) -> Duration {
        Duration::from_secs(120)
    }

    /// Airports from the built-in list, without spending a query.
    fn search(&self, text: &str) -> Result<Vec<Place>> {
        Ok(crate::airports::search(text, "UTC"))
    }

    fn board(&self, q: &BoardQuery) -> Result<Answer> {
        let pages = self.budget.pages.max(1);
        {
            let spent = self
                .spent
                .lock()
                .map_err(|_| Error::Failed("budget lost".into()))?;
            if spent.this_month(q.now) + pages > self.budget.pages_a_month() {
                return Err(Error::Failed(format!(
                    "THIS MONTH'S AEROAPI BUDGET IS USED: BACK ON {}",
                    next_month(q.now)
                )));
            }
        }
        let kind = match q.direction {
            Direction::Departure => "scheduled_departures",
            Direction::Arrival => "scheduled_arrivals",
        };
        let fmt = |t: DateTime<Utc>| t.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let url = format!(
            "{}/airports/{}/flights/{kind}?type=Airline&max_pages={pages}&start={}&end={}",
            self.base,
            q.place.id,
            fmt(q.now - Span::minutes(30)),
            fmt(q.now + Span::hours(12)),
        );
        let answer = self.fetch.get(&url, &[("x-apikey", &self.key)]);
        // A request that was answered is paid for, whatever it said.
        let body = answer?;
        let f: Flights = serde_json::from_str(&body)
            .map_err(|e| Error::Failed(format!("AeroAPI's answer didn't read: {e}")))?;
        if let Ok(mut spent) = self.spent.lock() {
            spent.add(q.now, f.num_pages.unwrap_or(1).clamp(1, pages));
        }
        let tz: Tz = q.place.tz.parse().unwrap_or(Tz::UTC);
        let list = match q.direction {
            Direction::Departure => &f.scheduled_departures,
            Direction::Arrival => &f.scheduled_arrivals,
        };
        Ok(Answer {
            movements: list
                .iter()
                .filter_map(|x| movement(x, q.direction, tz, q.now))
                .collect(),
            notices: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;
    use chrono::TimeZone;

    const DEPARTURES: &str = include_str!("../testdata/aeroapi-departures-handmade.json");
    const ARRIVALS: &str = include_str!("../testdata/aeroapi-arrivals-handmade.json");

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 30, 7, 0, 0).unwrap()
    }

    fn query(direction: Direction) -> BoardQuery {
        BoardQuery {
            place: crate::airports::melbourne(),
            mode: Mode::Flight,
            direction,
            now: now(),
        }
    }

    fn api(fake: Arc<Fake>, spent: Arc<Mutex<Spent>>) -> AeroApi {
        AeroApi::new("secret".into(), Budget::default(), spent, Box::new(fake))
    }

    #[test]
    fn departures_read_as_a_board() {
        let fake = Arc::new(Fake::default().with(
            "/aeroapi/airports/YMML/flights/scheduled_departures",
            DEPARTURES,
        ));
        let a = api(fake.clone(), Arc::default())
            .board(&query(Direction::Departure))
            .unwrap();
        let ms = &a.movements;
        assert_eq!(ms.len(), 4);
        assert_eq!(ms[0].numbers, ["QF 401", "AA 7365", "EK 5101"]);
        assert_eq!(ms[0].place, "Sydney");
        assert_eq!(ms[0].stand.as_deref(), Some("5"));
        assert_eq!(ms[0].operator, "QF");
        assert_eq!(ms[0].scheduled.format("%H:%M").to_string(), "18:00");
        assert_eq!(ms[0].status, Status::OnTime);
        assert_eq!(ms[1].status, Status::Delayed);
        assert_eq!(ms[2].status, Status::Cancelled);
        assert_eq!(ms[3].status, Status::Departed);
        // The key goes in its header, never the URL; the request asks for
        // airlines only, one page.
        let url = &fake.asked.lock().unwrap()[0];
        assert!(!url.contains("secret"));
        assert!(url.contains("type=Airline&max_pages=1&start=2026-10-30T06:30:00Z"));
    }

    #[test]
    fn arrivals_read_as_a_board() {
        let fake = Arc::new(Fake::default().with(
            "/aeroapi/airports/YMML/flights/scheduled_arrivals",
            ARRIVALS,
        ));
        let a = api(fake, Arc::default())
            .board(&query(Direction::Arrival))
            .unwrap();
        let ms = &a.movements;
        assert_eq!(ms[0].place, "Auckland");
        assert_eq!(ms[0].status, Status::Landed);
        assert_eq!(ms[1].status, Status::Expected);
        assert_eq!(ms[2].status, Status::Diverted);
        assert_eq!(ms[0].stand, None);
    }

    #[test]
    fn the_budget_is_counted_and_kept() {
        let fake = Arc::new(Fake::default().with("/aeroapi/", DEPARTURES));
        let spent = Arc::new(Mutex::new(Spent::default()));
        let p = api(fake.clone(), spent.clone());
        p.board(&query(Direction::Departure)).unwrap();
        assert_eq!(spent.lock().unwrap().this_month(now()), 1);
        // With the month's pages used up, it asks no more.
        spent.lock().unwrap().pages = Budget::default().pages_a_month();
        let e = p.board(&query(Direction::Departure)).unwrap_err();
        assert_eq!(
            e,
            Error::Failed("THIS MONTH'S AEROAPI BUDGET IS USED: BACK ON 1 NOV".into())
        );
        assert_eq!(fake.asked.lock().unwrap().len(), 1);
        // A new month starts again.
        assert_eq!(spent.lock().unwrap().this_month(now() + Span::days(5)), 0);
    }

    #[test]
    fn the_interval_keeps_inside_the_budget() {
        let b = Budget::default();
        // $5 at 2 cents a page is 250 pages; 8 hours a day for 31 days is
        // 248 hours: about an hour apart for one board, two for two.
        assert_eq!(b.pages_a_month(), 250);
        let one = b.interval(1).as_secs_f64() / 60.0;
        assert!((one - 59.5).abs() < 1.0, "{one}");
        assert!((b.interval(2).as_secs_f64() / 60.0 - 2.0 * one).abs() < 1.0);
        // Never more often than every two minutes.
        let rich = Budget {
            monthly: 1000.0,
            ..b
        };
        assert_eq!(rich.interval(1), Duration::from_secs(120));
    }
}
