//! What every feed is turned into: one movement record, a flight or a
//! train arriving or leaving, whatever the provider calls things.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Mode {
    #[default]
    Flight,
    Train,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Direction {
    #[default]
    Departure,
    Arrival,
}

impl Direction {
    pub fn title(self) -> &'static str {
        match self {
            Direction::Departure => "DEPARTURES",
            Direction::Arrival => "ARRIVALS",
        }
    }
}

/// Where a board's movements come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Feed {
    /// The built-in demo: no key, no network.
    #[default]
    Demo,
    /// Public Transport Victoria's Timetable API, for Victoria's trains.
    Ptv,
    /// FlightAware's AeroAPI, for an airport's flights.
    AeroApi,
}

impl Feed {
    /// A short name, for the cache's file names.
    pub fn key(self) -> &'static str {
        match self {
            Feed::Demo => "demo",
            Feed::Ptv => "ptv",
            Feed::AeroApi => "aeroapi",
        }
    }
}

/// Where a movement is in its day, as a board says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    OnTime,
    Delayed,
    Expected,
    Boarding,
    GateOpen,
    FinalCall,
    GateClosed,
    Departed,
    Landed,
    Arrived,
    Baggage,
    Cancelled,
    Diverted,
    Unknown,
}

impl Status {
    /// The phrase on the remarks flap; blank when there's nothing to say.
    pub fn words(self) -> &'static str {
        match self {
            Status::OnTime => "ON TIME",
            Status::Delayed => "DELAYED",
            Status::Expected => "EXPECTED",
            Status::Boarding => "BOARDING",
            Status::GateOpen => "GATE OPEN",
            Status::FinalCall => "FINAL CALL",
            Status::GateClosed => "GATE CLOSED",
            Status::Departed => "DEPARTED",
            Status::Landed => "LANDED",
            Status::Arrived => "ARRIVED",
            Status::Baggage => "BAGGAGE",
            Status::Cancelled => "CANCELLED",
            Status::Diverted => "DIVERTED",
            Status::Unknown => " ",
        }
    }

    /// Whether it's over: gone, arrived or not coming.
    pub fn finished(self) -> bool {
        matches!(
            self,
            Status::Departed | Status::Arrived | Status::Cancelled | Status::Diverted
        )
    }
}

/// One flight or train, arriving or leaving.
#[derive(Clone, Debug, PartialEq)]
pub struct Movement {
    /// Stable across updates, for following a movement as the board
    /// changes.
    pub id: String,
    pub mode: Mode,
    pub direction: Direction,
    /// In the airport's or station's own time zone.
    pub scheduled: DateTime<Tz>,
    pub expected: Option<DateTime<Tz>>,
    /// Flight numbers, the operating one first then codeshares; one for a
    /// train, or none.
    pub numbers: Vec<String>,
    /// The operator's code or short name.
    pub operator: String,
    /// The destination, or for an arrival where it came from.
    pub place: String,
    /// Calling points.
    pub via: Vec<String>,
    /// Gate or platform.
    pub stand: Option<String>,
    pub terminal: Option<String>,
    pub status: Status,
}

impl Movement {
    /// When it actually goes or comes, as far as anyone knows.
    pub fn time(&self) -> DateTime<Tz> {
        self.expected.unwrap_or(self.scheduled)
    }
}

/// An airport or a station.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Place {
    /// The provider's own id for it.
    pub id: String,
    pub name: String,
    pub tz: String,
}

/// What a board asks its provider for.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardQuery {
    pub place: Place,
    pub mode: Mode,
    pub direction: Direction,
    /// The time now: the real time, or the demo's when it runs fast.
    pub now: DateTime<Utc>,
}
