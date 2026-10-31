//! From movements to what the board shows: the order, what's gone, the
//! columns and their words, and what has changed since the last update.

use chrono::{DateTime, Duration as Span, Utc};
use egui::Color32;
use serde::{Deserialize, Serialize};
use splitflap::flap::REMARKS;
use splitflap::{Column, Drum, Row};

use crate::model::{Direction, Mode, Movement, Status};

pub const YELLOW: Color32 = Color32::from_rgb(244, 196, 48);
pub const AMBER: Color32 = Color32::from_rgb(240, 132, 32);
pub const RED: Color32 = Color32::from_rgb(222, 56, 44);

/// What a column shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Field {
    /// The scheduled time.
    Time,
    /// The flight number, cycling through codeshares.
    Flight,
    /// The airline's code on its own, for its colour card: the flight
    /// number's first part, cycling with it.
    Airline,
    /// The flight number without the airline's code.
    Number,
    /// Destination, or where an arrival comes from.
    Place,
    /// Gate or platform.
    Stand,
    /// The word flap: ON TIME, BOARDING…
    Remarks,
    /// The expected time: as a time for flights, or ON TIME, a time or what
    /// happened, for trains.
    Expected,
    Operator,
}

/// A column of a board: what it shows, its caption and how many cards.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub field: Field,
    pub title: &'static str,
    pub cards: usize,
}

impl Spec {
    fn new(field: Field, title: &'static str, cards: usize) -> Self {
        Self {
            field,
            title,
            cards,
        }
    }

    /// The board's column for it.
    pub fn column(&self, mode: Mode) -> Column {
        match (self.field, mode) {
            (Field::Time, _) | (Field::Expected, Mode::Flight) => {
                Column::time(self.title, self.cards)
            }
            (Field::Remarks, _) => Column::words(self.title, self.cards, REMARKS),
            _ => Column::new(self.title, self.cards, Drum::Alpha),
        }
    }
}

/// The brief's column presets for each kind of board.
pub fn preset(mode: Mode, direction: Direction) -> Vec<Spec> {
    use Field::*;
    match (mode, direction) {
        (Mode::Flight, Direction::Departure) => vec![
            Spec::new(Time, "TIME", 5),
            Spec::new(Airline, "FLIGHT", 2),
            Spec::new(Number, "", 5),
            Spec::new(Place, "DESTINATION", 16),
            Spec::new(Stand, "GATE", 4),
            Spec::new(Remarks, "REMARKS", 12),
        ],
        (Mode::Flight, Direction::Arrival) => vec![
            Spec::new(Time, "TIME", 5),
            Spec::new(Airline, "FLIGHT", 2),
            Spec::new(Number, "", 5),
            Spec::new(Place, "FROM", 16),
            Spec::new(Expected, "EXPECTED", 5),
            Spec::new(Remarks, "REMARKS", 12),
        ],
        (Mode::Train, Direction::Departure) => vec![
            Spec::new(Time, "TIME", 5),
            Spec::new(Place, "DESTINATION", 18),
            Spec::new(Stand, "PLAT", 3),
            Spec::new(Expected, "EXPECTED", 9),
            Spec::new(Operator, "OPERATOR", 10),
        ],
        (Mode::Train, Direction::Arrival) => vec![
            Spec::new(Time, "TIME", 5),
            Spec::new(Place, "FROM", 18),
            Spec::new(Stand, "PLAT", 3),
            Spec::new(Expected, "EXPECTED", 9),
        ],
    }
}

/// How long a movement stays on the board after it has gone or come:
/// flights a while, for the last of the gate and the baggage; trains
/// hardly at all.
fn linger(m: &Movement) -> Span {
    Span::minutes(match (m.mode, m.direction) {
        (Mode::Flight, Direction::Departure) => 10,
        (Mode::Flight, Direction::Arrival) => 25,
        (Mode::Train, Direction::Departure) => 1,
        (Mode::Train, Direction::Arrival) => 2,
    })
}

/// The movements a board shows at `now`: in scheduled order, those long
/// gone dropped.
pub fn normalise(mut ms: Vec<Movement>, now: DateTime<Utc>) -> Vec<Movement> {
    ms.retain(|m| m.time().with_timezone(&Utc) + linger(m) > now);
    ms.sort_by(|a, b| a.scheduled.cmp(&b.scheduled).then_with(|| a.id.cmp(&b.id)));
    ms
}

/// What the board says in `field` for `m`; `cycle` picks which codeshare
/// the flight cell shows.
pub fn cell(m: &Movement, field: Field, cycle: usize) -> String {
    let hm = |t: chrono::DateTime<chrono_tz::Tz>| t.format("%H:%M").to_string();
    match field {
        Field::Time => hm(m.scheduled),
        Field::Flight => match m.numbers.len() {
            0 => String::new(),
            n => m.numbers[cycle % n].clone(),
        },
        Field::Airline => match m.numbers.len() {
            0 => m.operator.clone(),
            n => crate::airlines::code_of(&m.numbers[cycle % n]),
        },
        Field::Number => match m.numbers.len() {
            0 => String::new(),
            n => crate::airlines::number_of(&m.numbers[cycle % n]),
        },
        Field::Place => m.place.clone(),
        Field::Stand => m.stand.clone().unwrap_or_default(),
        Field::Remarks => m.status.words().to_string(),
        Field::Expected => match (m.mode, m.status) {
            (Mode::Flight, Status::Cancelled | Status::Diverted) => String::new(),
            (Mode::Flight, _) => m.expected.map(hm).unwrap_or_default(),
            (Mode::Train, Status::Cancelled) => "CANCELLED".into(),
            (Mode::Train, Status::Departed) => "DEPARTED".into(),
            (Mode::Train, Status::Arrived) => "ARRIVED".into(),
            (Mode::Train, _) => match m.expected {
                Some(e) if e > m.scheduled => hm(e),
                _ => "ON TIME".into(),
            },
        },
        Field::Operator => m.operator.clone(),
    }
}

/// The lit tab beside a row that matters: yellow to board, amber when
/// late, red when it isn't coming.
pub fn tab(m: &Movement) -> Option<Color32> {
    match m.status {
        Status::Boarding | Status::FinalCall => Some(YELLOW),
        Status::Delayed => Some(AMBER),
        Status::Cancelled | Status::Diverted => Some(RED),
        _ => None,
    }
}

/// How airlines' cards look: their colours, from Settings and the
/// built-in table, and any logos the user has added.
pub struct Operators<'a> {
    /// Colour cards at all.
    pub colours: bool,
    /// Colours set in Settings, by airline code.
    pub own: &'a [(String, [u8; 3])],
    /// The logo for an airline code, and its size, if the user has one.
    pub logo: &'a mut dyn FnMut(&str) -> Option<(egui::TextureId, egui::Vec2)>,
}

/// The light card a logo sits on, and the letters on it while it flips.
const LOGO_CARD: Color32 = Color32::from_rgb(242, 240, 234);
const LOGO_INK: Color32 = Color32::from_rgb(30, 30, 28);

/// The board's rows for `ms`. With `stale`, the remarks say SEE SCREENS:
/// the board no longer knows. With `ops`, the airline column gets its
/// colour card or logo.
pub fn rows(
    ms: &[Movement],
    specs: &[Spec],
    cycle: usize,
    stale: bool,
    mut ops: Option<&mut Operators>,
) -> Vec<Row> {
    let airline = specs.iter().position(|s| s.field == Field::Airline);
    ms.iter()
        .map(|m| {
            let cells: Vec<String> = specs
                .iter()
                .map(|s| match s.field {
                    Field::Remarks if stale => "SEE SCREENS".into(),
                    f => cell(m, f, cycle),
                })
                .collect();
            let mut row = Row::new(&cells);
            if let Some(t) = tab(m).filter(|_| !stale) {
                row = row.tab(t);
            }
            if let (Some(col), Some(ops)) = (airline, ops.as_deref_mut()) {
                let code = cell(m, Field::Airline, cycle);
                if let Some((tex, size)) = (ops.logo)(&code) {
                    row = row.tile(col, LOGO_CARD, LOGO_INK).picture(col, tex, size);
                } else if ops.colours {
                    if let Some(c) = crate::airlines::colour(&code, ops.own) {
                        let card = Color32::from_rgb(c[0], c[1], c[2]);
                        row = row.tile(col, card, crate::airlines::ink(c));
                    }
                }
            }
            row
        })
        .collect()
}

/// The movements whose gate or platform has changed between `before` and
/// `after`, by id.
pub fn moved(before: &[Movement], after: &[Movement]) -> Vec<String> {
    after
        .iter()
        .filter(|a| {
            before.iter().any(|b| {
                b.id == a.id && b.stand.is_some() && a.stand.is_some() && b.stand != a.stand
            })
        })
        .map(|a| a.id.clone())
        .collect()
}

/// Splits `rows` into pages of `per` rows. Always at least one page, which
/// may be empty.
pub fn pages<T: Clone>(rows: &[T], per: usize) -> Vec<Vec<T>> {
    let per = per.max(1);
    let mut out: Vec<Vec<T>> = rows.chunks(per).map(|c| c.to_vec()).collect();
    if out.is_empty() {
        out.push(Vec::new());
    }
    out
}

/// `text` in pieces of at most `width` characters, broken between words
/// where it can be: for a ticker that shows one piece at a time.
pub fn ticker_pieces(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in splitflap::fold::fold(text).split(' ') {
        let mut word = word.to_string();
        while word.chars().count() > width {
            let head: String = word.chars().take(width).collect();
            word = word.chars().skip(width).collect();
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            out.push(head);
        }
        let len = line.chars().count();
        if len > 0 && len + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::Tz;

    fn at(h: u32, m: u32) -> DateTime<Tz> {
        Tz::UTC.with_ymd_and_hms(2026, 10, 30, h, m, 0).unwrap()
    }

    fn flight(id: &str, h: u32, m: u32, status: Status) -> Movement {
        Movement {
            id: id.into(),
            mode: Mode::Flight,
            direction: Direction::Departure,
            scheduled: at(h, m),
            expected: Some(at(h, m)),
            numbers: vec!["NB 1".into(), "GX 2001".into()],
            operator: "NB".into(),
            place: "Zürich".into(),
            via: Vec::new(),
            stand: Some("5".into()),
            terminal: None,
            status,
        }
    }

    #[test]
    fn the_board_runs_in_scheduled_order_and_drops_the_long_gone() {
        let now = at(12, 0).with_timezone(&Utc);
        let ms = vec![
            flight("c", 12, 30, Status::OnTime),
            flight("a", 11, 45, Status::Departed),
            flight("b", 11, 55, Status::Departed),
            flight("d", 12, 5, Status::Boarding),
        ];
        let ids: Vec<String> = normalise(ms, now).into_iter().map(|m| m.id).collect();
        // Gone 15 minutes: dropped; gone 5: still there.
        assert_eq!(ids, ["b", "d", "c"]);
    }

    #[test]
    fn a_delayed_flight_keeps_its_place_and_stays_until_it_goes() {
        let now = at(12, 0).with_timezone(&Utc);
        let mut late = flight("late", 11, 0, Status::Delayed);
        late.expected = Some(at(12, 40));
        let ms = normalise(vec![flight("x", 11, 30, Status::OnTime), late], now);
        assert_eq!(ms.len(), 1, "the 11:30 has gone");
        assert_eq!(ms[0].id, "late");
    }

    #[test]
    fn cells_say_what_a_board_says() {
        let m = flight("a", 9, 5, Status::FinalCall);
        let specs = preset(Mode::Flight, Direction::Departure);
        let rows = rows(std::slice::from_ref(&m), &specs, 0, false, None);
        assert_eq!(
            rows[0].cells,
            ["09:05", "NB", "1", "Zürich", "5", "FINAL CALL"]
        );
        assert_eq!(rows[0].tab, Some(YELLOW));
        // Codeshares take turns.
        assert_eq!(cell(&m, Field::Flight, 1), "GX 2001");
        assert_eq!(cell(&m, Field::Flight, 2), "NB 1");
        // Stale, the remarks give up.
        let stale = super::rows(std::slice::from_ref(&m), &specs, 0, true, None);
        assert_eq!(stale[0].cells[5], "SEE SCREENS");
        assert_eq!(stale[0].tab, None);
    }

    #[test]
    fn airlines_get_their_cards() {
        let m = flight("a", 9, 5, Status::OnTime);
        let specs = preset(Mode::Flight, Direction::Departure);
        let mut no_logos = |_: &str| None;
        let mut ops = Operators {
            colours: true,
            own: &[],
            logo: &mut no_logos,
        };
        let r = rows(std::slice::from_ref(&m), &specs, 0, false, Some(&mut ops));
        // NB is a demo airline: its colour card, on the airline column.
        assert!(r[0].tiles.get(1).copied().flatten().is_some());
        assert!(r[0].picture.is_none());
        // Codeshares cycle the card with the number.
        assert_eq!(cell(&m, Field::Airline, 1), "GX");
        assert_eq!(cell(&m, Field::Number, 1), "2001");
        // A logo wins over the colour.
        let tex = egui::TextureId::User(7);
        let mut logos = |code: &str| (code == "NB").then_some((tex, egui::vec2(100.0, 50.0)));
        let mut ops = Operators {
            colours: true,
            own: &[],
            logo: &mut logos,
        };
        let r = rows(std::slice::from_ref(&m), &specs, 0, false, Some(&mut ops));
        assert_eq!(r[0].picture.map(|p| (p.0, p.1)), Some((1, tex)));
        // Colour cards off, and no logo: the board's own card.
        let mut none = |_: &str| None;
        let mut ops = Operators {
            colours: false,
            own: &[],
            logo: &mut none,
        };
        let r = rows(std::slice::from_ref(&m), &specs, 0, false, Some(&mut ops));
        assert!(r[0].tiles.iter().all(Option::is_none));
    }

    #[test]
    fn trains_say_their_expected_time_in_words() {
        let mut t = flight("t", 9, 5, Status::OnTime);
        t.mode = Mode::Train;
        assert_eq!(cell(&t, Field::Expected, 0), "ON TIME");
        t.expected = Some(at(9, 12));
        t.status = Status::Delayed;
        assert_eq!(cell(&t, Field::Expected, 0), "09:12");
        t.status = Status::Cancelled;
        assert_eq!(cell(&t, Field::Expected, 0), "CANCELLED");
    }

    #[test]
    fn a_new_gate_is_noticed() {
        let a = flight("a", 9, 5, Status::OnTime);
        let mut b = a.clone();
        b.stand = Some("12".into());
        assert_eq!(
            moved(std::slice::from_ref(&a), std::slice::from_ref(&b)),
            ["a"]
        );
        assert!(moved(std::slice::from_ref(&a), std::slice::from_ref(&a)).is_empty());
        // A gate first given out isn't a change.
        let mut none = a.clone();
        none.stand = None;
        assert!(moved(&[none], &[b]).is_empty());
    }

    #[test]
    fn rows_page_by_the_page() {
        let rows: Vec<u32> = (0..32).collect();
        let p = pages(&rows, 15);
        assert_eq!(p.len(), 3);
        assert_eq!(p[2], [30, 31]);
        assert_eq!(pages::<u32>(&[], 15), [Vec::<u32>::new()]);
    }

    #[test]
    fn the_ticker_breaks_between_words() {
        let p = ticker_pieces(
            "Calling at Wharf Road, Cannery Lane, Elm Park and Hillcrest",
            24,
        );
        assert!(p.iter().all(|s| s.chars().count() <= 24), "{p:?}");
        assert_eq!(p[0], "CALLING AT WHARF ROAD,");
        assert_eq!(
            p.join(" "),
            "CALLING AT WHARF ROAD, CANNERY LANE, ELM PARK AND HILLCREST"
        );
    }
}
