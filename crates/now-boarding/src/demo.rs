//! The demo feed: a made-up airport and a made-up station, each with a
//! full day of movements, delays, gate and platform changes, cancellations
//! and diversions. The same day comes round every time, worked out from the
//! date, so it needs no key, no network and no saved state.

use std::time::Duration;

use chrono::{DateTime, Datelike, Duration as Span, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use splitflap::rng::Rng;

use crate::model::{BoardQuery, Direction, Mode, Movement, Place, Status};
use crate::provider::{Provider, Result};

pub const AIRPORT: &str = "demo-airport";
pub const STATION: &str = "demo-station";

/// Made-up airlines' codes.
const AIRLINES: &[&str] = &["NB", "GX", "VR", "OZ", "KF", "TQ", "MW", "PJ"];

const CITIES: &[&str] = &[
    "London Heathrow",
    "Zürich",
    "Auckland",
    "Singapore",
    "Wellington",
    "São Paulo Guarulhos",
    "Hobart",
    "Tokyo Haneda",
    "Kuala Lumpur International",
    "Christchurch",
    "Perth",
    "Los Angeles",
    "Nadi",
    "Düsseldorf",
    "Adelaide",
    "Hong Kong",
    "Dubai",
    "Queenstown",
    "Brisbane",
    "San Francisco",
    "Denpasar",
    "Johannesburg O.R. Tambo",
    "Sydney",
    "Seoul Incheon",
    "Doha",
    "Rarotonga",
    "Darwin",
    "Vancouver",
    "Port Moresby",
    "Malmö",
    "Reykjavík",
    "Cairns",
];

/// Made-up railway lines: who runs them, and their stops outward from the
/// station, the last being where trains are going.
const LINES: &[(&str, &[&str])] = &[
    (
        "METRO",
        &[
            "Wharf Road",
            "Cannery Lane",
            "Elm Park",
            "Fairview",
            "Hillcrest",
        ],
    ),
    (
        "METRO",
        &[
            "Market Square",
            "Northgate",
            "Ashby",
            "Kingsmere",
            "Lakeside",
        ],
    ),
    ("METRO", &["Old Mill", "Crossways", "Broadmeadow"]),
    (
        "COASTLINE",
        &[
            "Port Ellen",
            "Saltmarsh",
            "Gull Point",
            "Seacliff Junction",
            "Bayhaven",
        ],
    ),
    (
        "VALE RAIL",
        &[
            "Riverbend",
            "Orchard Hill",
            "Stonebridge",
            "Mount Ashcombe",
            "Glenrowan Springs",
        ],
    ),
    (
        "NIGHT LINE",
        &["Fairview", "Marlow Heath", "Upper Kingsmere"],
    ),
];

/// One movement as planned for the day, before the time of day says how it
/// is going.
struct Plan {
    id: String,
    numbers: Vec<String>,
    operator: String,
    place: String,
    via: Vec<String>,
    scheduled: DateTime<Tz>,
    /// Minutes late.
    delay: i64,
    cancelled: bool,
    diverted: bool,
    stand: String,
    /// A new gate or platform, given out this many minutes before it goes
    /// or comes.
    change: Option<(String, i64)>,
    terminal: Option<String>,
}

pub struct Demo {
    tz: Tz,
}

impl Demo {
    /// The demo, its airport and station in time zone `tz`.
    pub fn new(tz: Tz) -> Self {
        Self { tz }
    }

    pub fn places(tz: &str) -> [Place; 2] {
        [
            Place {
                id: AIRPORT.into(),
                name: "Gate 05 International".into(),
                tz: tz.into(),
            },
            Place {
                id: STATION.into(),
                name: "Harbour Street".into(),
                tz: tz.into(),
            },
        ]
    }

    /// The day's plan, for one board, on `date`.
    fn day(&self, mode: Mode, direction: Direction, date: NaiveDate) -> Vec<Plan> {
        let seed = date.num_days_from_ce() as u64 * 4
            + match (mode, direction) {
                (Mode::Flight, Direction::Departure) => 0,
                (Mode::Flight, Direction::Arrival) => 1,
                (Mode::Train, Direction::Departure) => 2,
                (Mode::Train, Direction::Arrival) => 3,
            };
        let mut rng = Rng::with_seed(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut pick = |n: usize| (rng.between(0.0, n as f64) as usize).min(n - 1);
        let (first, last, gap): (u32, u32, (u32, u32)) = match mode {
            Mode::Flight => (5 * 60 + 30, 23 * 60 + 50, (5, 14)),
            Mode::Train => (4 * 60 + 50, 24 * 60 + 30, (3, 9)),
        };
        let mut out = Vec::new();
        let mut minute = first;
        let mut k = 0;
        while minute < last {
            let Some(scheduled) = self.at(date, minute) else {
                minute += 10;
                continue;
            };
            let roll = |p: &mut dyn FnMut(usize) -> usize| p(1000) as f64 / 1000.0;
            let id = format!(
                "{date}-{}-{}-{k}",
                match mode {
                    Mode::Flight => "f",
                    Mode::Train => "t",
                },
                match direction {
                    Direction::Departure => "d",
                    Direction::Arrival => "a",
                }
            );
            let plan = match mode {
                Mode::Flight => {
                    let airline = AIRLINES[pick(AIRLINES.len())];
                    let mut numbers = vec![format!("{airline} {}", 10 + pick(890))];
                    if roll(&mut pick) < 0.35 {
                        for _ in 0..1 + pick(2) {
                            let other = AIRLINES[pick(AIRLINES.len())];
                            if other != airline {
                                numbers.push(format!("{other} {}", 1000 + pick(8999)));
                            }
                        }
                    }
                    let gate = (1 + pick(24)).to_string();
                    let delayed = roll(&mut pick) < 0.16;
                    let delay = if delayed { 15 + pick(80) as i64 } else { 0 };
                    let cancelled = roll(&mut pick) < 0.04;
                    let diverted = direction == Direction::Arrival && roll(&mut pick) < 0.02;
                    let change = (roll(&mut pick) < 0.08).then(|| {
                        let g = (1 + pick(24)).to_string();
                        (g, 50)
                    });
                    Plan {
                        id,
                        operator: airline.into(),
                        numbers,
                        place: CITIES[pick(CITIES.len())].into(),
                        via: Vec::new(),
                        scheduled,
                        delay,
                        cancelled,
                        diverted,
                        stand: gate,
                        change,
                        terminal: Some(if pick(3) == 0 { "T2" } else { "T1" }.into()),
                    }
                }
                Mode::Train => {
                    let (operator, stops) = LINES[pick(LINES.len())];
                    // Arrivals come the other way, from the line's end.
                    let place = stops[stops.len() - 1];
                    let mut via: Vec<String> = stops[..stops.len() - 1]
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    if direction == Direction::Arrival {
                        via.reverse();
                    }
                    let delayed = roll(&mut pick) < 0.2;
                    let delay = if delayed { 2 + pick(14) as i64 } else { 0 };
                    let cancelled = roll(&mut pick) < 0.03;
                    let platform = (1 + pick(14)).to_string();
                    let change = (roll(&mut pick) < 0.06).then(|| ((1 + pick(14)).to_string(), 12));
                    Plan {
                        id,
                        numbers: Vec::new(),
                        operator: operator.into(),
                        place: place.into(),
                        via,
                        scheduled,
                        delay,
                        cancelled,
                        diverted: false,
                        stand: platform,
                        change,
                        terminal: None,
                    }
                }
            };
            out.push(plan);
            minute += gap.0 + pick((gap.1 - gap.0 + 1) as usize) as u32;
            k += 1;
        }
        out
    }

    /// `minute` minutes after the start of `date`, in the demo's zone; the
    /// next day's early hours for minutes past midnight.
    fn at(&self, date: NaiveDate, minute: u32) -> Option<DateTime<Tz>> {
        let day = date + Span::days((minute / (24 * 60)) as i64);
        let m = minute % (24 * 60);
        let t = day.and_hms_opt(m / 60, m % 60, 0)?;
        self.tz.from_local_datetime(&t).earliest()
    }

    /// How a planned movement stands at `now`.
    fn movement(&self, p: &Plan, mode: Mode, direction: Direction, now: DateTime<Tz>) -> Movement {
        let expected = p.scheduled + Span::minutes(p.delay);
        // Whole minutes to go, rounded up: it hasn't gone until it's gone.
        let secs = (expected - now).num_seconds();
        let to_go = secs.div_euclid(60) + i64::from(secs.rem_euclid(60) > 0);
        let stand = match &p.change {
            Some((new, before)) if to_go < *before => new.clone(),
            _ => p.stand.clone(),
        };
        let late = p.delay > 0;
        let status = if p.cancelled {
            Status::Cancelled
        } else if p.diverted && to_go < 30 {
            Status::Diverted
        } else {
            match (mode, direction) {
                (Mode::Flight, Direction::Departure) => match to_go {
                    t if t <= 0 => Status::Departed,
                    t if t <= 5 => Status::GateClosed,
                    t if t <= 15 => Status::FinalCall,
                    t if t <= 30 => Status::Boarding,
                    t if t <= 45 => Status::GateOpen,
                    _ if late => Status::Delayed,
                    _ => Status::OnTime,
                },
                (Mode::Flight, Direction::Arrival) => match to_go {
                    t if t <= -10 => Status::Baggage,
                    t if t <= 0 => Status::Landed,
                    _ if late => Status::Delayed,
                    t if t <= 20 => Status::Expected,
                    _ => Status::OnTime,
                },
                (Mode::Train, Direction::Departure) => match to_go {
                    t if t <= 0 => Status::Departed,
                    _ if late => Status::Delayed,
                    _ => Status::OnTime,
                },
                (Mode::Train, Direction::Arrival) => match to_go {
                    t if t <= 0 => Status::Arrived,
                    _ if late => Status::Delayed,
                    _ => Status::OnTime,
                },
            }
        };
        Movement {
            id: p.id.clone(),
            mode,
            direction,
            scheduled: p.scheduled,
            expected: (!p.cancelled).then_some(expected),
            numbers: p.numbers.clone(),
            operator: p.operator.clone(),
            place: p.place.clone(),
            via: p.via.clone(),
            stand: (!p.cancelled).then_some(stand),
            terminal: p.terminal.clone(),
            status,
        }
    }
}

impl Provider for Demo {
    fn name(&self) -> &str {
        "Demo"
    }

    fn attribution(&self) -> &str {
        "DEMO FEED: MADE-UP, NOT FOR TRAVEL"
    }

    fn min_interval(&self) -> Duration {
        Duration::ZERO
    }

    fn search(&self, text: &str) -> Result<Vec<Place>> {
        let t = text.to_lowercase();
        Ok(Self::places(self.tz.name())
            .into_iter()
            .filter(|p| p.name.to_lowercase().contains(&t))
            .collect())
    }

    /// Movements from an hour ago to eighteen hours ahead: yesterday's
    /// late ones and tomorrow's early ones included.
    fn board(&self, q: &BoardQuery) -> Result<Vec<Movement>> {
        let now = q.now.with_timezone(&self.tz);
        let today = now.date_naive();
        let (from, to) = (now - Span::hours(1), now + Span::hours(18));
        let mut out = Vec::new();
        for date in [today.pred_opt(), Some(today), today.succ_opt()]
            .into_iter()
            .flatten()
        {
            for p in self.day(q.mode, q.direction, date) {
                let t = p.scheduled + Span::minutes(p.delay);
                if t >= from && t <= to {
                    out.push(self.movement(&p, q.mode, q.direction, now));
                }
            }
        }
        Ok(out)
    }
}

/// The demo's time zone: the computer's own, or UTC.
pub fn local_tz() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|tz| tz.parse().ok())
        .unwrap_or(Tz::UTC)
}

/// `t` in UTC, for queries.
pub fn utc(t: DateTime<Tz>) -> DateTime<Utc> {
    t.with_timezone(&Utc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(mode: Mode, direction: Direction, now: DateTime<Utc>) -> BoardQuery {
        BoardQuery {
            place: Demo::places("UTC")[0].clone(),
            mode,
            direction,
            now,
        }
    }

    fn noon() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 30, 12, 0, 0).unwrap()
    }

    #[test]
    fn the_same_day_comes_round_every_time() {
        let d = Demo::new(Tz::UTC);
        let q = query(Mode::Flight, Direction::Departure, noon());
        assert_eq!(d.board(&q).unwrap(), d.board(&q).unwrap());
        let other = query(Mode::Flight, Direction::Departure, noon() + Span::days(1));
        assert_ne!(
            d.board(&q).unwrap()[0].numbers,
            d.board(&other).unwrap()[0].numbers
        );
    }

    #[test]
    fn a_day_has_everything_a_board_shows() {
        let d = Demo::new(Tz::UTC);
        let day = d.day(
            Mode::Flight,
            Direction::Departure,
            NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(),
        );
        assert!(day.len() > 80, "{}", day.len());
        assert!(day.iter().any(|p| p.delay > 0));
        assert!(day.iter().any(|p| p.cancelled));
        assert!(day.iter().any(|p| p.change.is_some()));
        assert!(day.iter().any(|p| p.numbers.len() > 1));
        let trains = d.day(
            Mode::Train,
            Direction::Departure,
            NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(),
        );
        assert!(trains.len() > 150, "{}", trains.len());
        assert!(trains.iter().any(|p| p.via.len() >= 3));
        assert!(trains.iter().any(|p| p.change.is_some()));
    }

    #[test]
    fn a_flight_goes_through_its_day() {
        let d = Demo::new(Tz::UTC);
        let date = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        let plan = d
            .day(Mode::Flight, Direction::Departure, date)
            .into_iter()
            .find(|p| !p.cancelled && p.delay == 0)
            .unwrap();
        let at = |mins: i64| {
            d.movement(
                &plan,
                Mode::Flight,
                Direction::Departure,
                plan.scheduled - Span::minutes(mins),
            )
            .status
        };
        assert_eq!(at(60), Status::OnTime);
        assert_eq!(at(40), Status::GateOpen);
        assert_eq!(at(20), Status::Boarding);
        assert_eq!(at(10), Status::FinalCall);
        assert_eq!(at(3), Status::GateClosed);
        assert_eq!(at(-1), Status::Departed);
    }

    #[test]
    fn a_gate_changes_before_boarding() {
        let d = Demo::new(Tz::UTC);
        let date = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        let plan = d
            .day(Mode::Flight, Direction::Departure, date)
            .into_iter()
            .find(|p| !p.cancelled && p.change.as_ref().is_some_and(|(g, _)| *g != p.stand))
            .unwrap();
        let t = plan.scheduled + Span::minutes(plan.delay);
        let before = d.movement(
            &plan,
            Mode::Flight,
            Direction::Departure,
            t - Span::minutes(70),
        );
        let after = d.movement(
            &plan,
            Mode::Flight,
            Direction::Departure,
            t - Span::minutes(40),
        );
        assert_ne!(before.stand, after.stand);
    }

    /// A whole day at the board, a minute at a time, as the app sees it:
    /// flights leave from the top, delays and cancellations show, and gates
    /// change.
    #[test]
    fn a_day_at_the_board() {
        use crate::normalise::{moved, normalise, preset, rows};
        let d = Demo::new(Tz::UTC);
        let specs = preset(Mode::Flight, Direction::Departure);
        let start = Utc.with_ymd_and_hms(2026, 10, 30, 5, 0, 0).unwrap();
        let mut before: Vec<Movement> = Vec::new();
        let (mut left, mut changes, mut delayed, mut cancelled) = (0, 0, 0, 0);
        for minute in 0..(19 * 60) {
            let now = start + Span::minutes(minute);
            let ms = normalise(
                d.board(&query(Mode::Flight, Direction::Departure, now))
                    .unwrap(),
                now,
            );
            if let Some(top) = before.first() {
                if !ms.iter().any(|m| m.id == top.id) {
                    left += 1;
                    assert!(
                        top.status.finished(),
                        "{} left while {:?}",
                        top.id,
                        top.status
                    );
                }
            }
            changes += moved(&before, &ms).len();
            let page = rows(&ms[..ms.len().min(15)], &specs, 0, false);
            delayed += page.iter().filter(|r| r.cells[4] == "DELAYED").count();
            cancelled += page.iter().filter(|r| r.cells[4] == "CANCELLED").count();
            // Every row's cells fit the drums.
            for r in &page {
                assert_eq!(r.cells.len(), specs.len());
            }
            before = ms;
        }
        assert!(left > 40, "{left}");
        assert!(changes > 0);
        assert!(delayed > 0 && cancelled > 0);
    }

    /// Prints today's gate and platform changes, to find one to watch:
    /// cargo test -p now-boarding changes_today -- --ignored --nocapture
    #[test]
    #[ignore]
    fn changes_today() {
        let d = Demo::new(local_tz());
        let today = Utc::now().with_timezone(&local_tz()).date_naive();
        for (mode, direction) in [
            (Mode::Flight, Direction::Departure),
            (Mode::Train, Direction::Departure),
        ] {
            for p in d.day(mode, direction, today) {
                if let Some((stand, before)) = &p.change {
                    let at = p.scheduled + Span::minutes(p.delay - before);
                    println!(
                        "{mode:?} {} to {}: {} -> {stand} at {}",
                        p.scheduled.format("%H:%M"),
                        p.place,
                        p.stand,
                        at.format("%H:%M")
                    );
                }
            }
        }
    }

    #[test]
    fn the_board_covers_the_hours_ahead() {
        let d = Demo::new(Tz::UTC);
        let now = Utc.with_ymd_and_hms(2026, 10, 30, 23, 30, 0).unwrap();
        let b = d
            .board(&query(Mode::Train, Direction::Departure, now))
            .unwrap();
        // Past midnight, tomorrow's first trains are there.
        assert!(b.iter().any(|m| m.scheduled.date_naive().day() == 31));
        assert!(b.iter().all(|m| m.time() >= now - Span::hours(1)));
    }
}
