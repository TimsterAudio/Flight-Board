//! One board on the screen: its header (the title, the place and the time),
//! the grid of movements a page at a time, a ticker of cards, and a footer.
//! It takes movements from its poller and works out what the cards say.

use std::time::Duration;

use chrono::{DateTime, Duration as Span, Utc};
use chrono_tz::Tz;
use egui::{vec2, Align2, Color32, Context, FontFamily, FontId, Painter, Pos2, Vec2};
use splitflap::board::{Flip, CARD, FONT, SPACING};
use splitflap::lighting::LightMap;
use splitflap::rng::Rng;
use splitflap::{Board, Column, Row, Style};

use crate::clock::Clock;
use crate::demo::Demo;
use crate::model::{BoardQuery, Direction, Mode, Movement, Place, Status};
use crate::normalise::{self, Field, Spec};
use crate::poller::Poller;
use crate::provider::Provider;

/// The title's and the time's big cards.
const BIG: Vec2 = vec2(44.0, 64.0);
/// Gaps down the board, in board units.
const UNDER_TITLE: f32 = 10.0;
const UNDER_HEADER: f32 = 18.0;
const ABOVE_TICKER: f32 = 14.0;
const FOOTER: f32 = 22.0;
/// How long each piece of the ticker shows, in seconds.
const TICKER_EVERY: f64 = 6.0;
/// How long a codeshare shows on a flight's cell, in seconds.
const CODESHARE_EVERY: f64 = 8.0;
/// How long a gate or platform change's lamp blinks: three times.
pub const BLINK: f64 = 1.8;
/// How old the last good update can be, on the board's clock, before the
/// remarks give up and say SEE SCREENS.
const STALE: i64 = 30;
/// Failed updates in a row before the footer says when it last had one.
const FAILURES_SHOWN: u32 = 2;

/// What a board shows and how.
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    pub mode: Mode,
    pub direction: Direction,
    pub place: Place,
    pub rows: usize,
    /// Seconds each page shows, when there are more movements than rows.
    pub page_secs: f64,
    /// The most pages it turns through.
    pub max_pages: usize,
    pub specs: Vec<Spec>,
    pub second_title: String,
}

/// Something the board wants announced with the chime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Announce {
    /// A gate or platform has changed.
    StandChange,
    /// A flight has started boarding.
    Boarding,
}

pub struct View {
    pub setup: Setup,
    tz: Tz,
    title: Board,
    clock: Board,
    name: Board,
    second: Option<Board>,
    grid: Board,
    ticker: Board,
    poller: Poller,
    attribution: String,
    movements: Vec<Movement>,
    /// The board's time of the last good update, and failures since.
    updated: Option<DateTime<Utc>>,
    failures: u32,
    error: Option<String>,
    page: usize,
    pages: usize,
    page_since: f64,
    /// Movements whose gate or platform just changed, and when.
    blinks: Vec<(String, f64)>,
    ticker_piece: usize,
    ticker_since: f64,
}

/// How many letter cards fit across `width` board units.
fn cards_across(width: f32) -> usize {
    ((width + CARD.y * SPACING) / (CARD.x + CARD.y * SPACING) + 1e-3).floor() as usize
}

impl View {
    pub fn new(setup: Setup, clock: Clock, ctx: &Context, time: f64) -> Self {
        let tz: Tz = setup.place.tz.parse().unwrap_or(Tz::UTC);
        let provider: Box<dyn Provider> = Box::new(Demo::new(tz));
        let attribution = provider.attribution().to_string();
        let query = BoardQuery {
            place: setup.place.clone(),
            mode: setup.mode,
            direction: setup.direction,
            now: clock.now(),
        };
        // The demo asks once a board minute, and at least every few
        // seconds of real time.
        let every = Duration::from_secs_f64((60.0 / clock.speed().max(0.01)).clamp(1.0, 5.0));
        let wake = ctx.clone();
        let poller = Poller::start(provider, query, clock, every, move || {
            wake.request_repaint()
        });
        let columns: Vec<Column> = setup.specs.iter().map(|s| s.column(setup.mode)).collect();
        let grid = Board::new(columns, setup.rows.max(1), time, Rng::seeded()).with_tabs(true);
        let ticker_cards = cards_across(grid.size().x);
        let second = (!setup.second_title.trim().is_empty())
            .then(|| Board::new(vec![Column::text("", 24)], 1, time, Rng::seeded()));
        Self {
            tz,
            title: Board::new(vec![Column::text("", 10)], 1, time, Rng::seeded()).with_card(BIG),
            clock: Board::new(vec![Column::time("", 5)], 1, time, Rng::seeded()).with_card(BIG),
            name: Board::new(vec![Column::text("", 24)], 1, time, Rng::seeded()),
            second,
            grid,
            ticker: Board::new(vec![Column::text("", ticker_cards)], 1, time, Rng::seeded()),
            poller,
            attribution,
            movements: Vec::new(),
            updated: None,
            failures: 0,
            error: None,
            page: 0,
            pages: 1,
            page_since: time,
            blinks: Vec::new(),
            ticker_piece: 0,
            ticker_since: time,
            setup,
        }
    }

    /// Takes in what the poller has brought, and says what to announce.
    pub fn take_updates(&mut self, time: f64) -> Vec<Announce> {
        let mut out = Vec::new();
        while let Ok(u) = self.poller.updates.try_recv() {
            match u.result {
                Ok(ms) => {
                    let ms = normalise::normalise(ms, u.at);
                    if self.updated.is_some() {
                        for id in normalise::moved(&self.movements, &ms) {
                            self.blinks.push((id, time));
                            out.push(Announce::StandChange);
                        }
                        let boarding = ms.iter().any(|m| {
                            m.status == Status::Boarding
                                && self
                                    .movements
                                    .iter()
                                    .any(|b| b.id == m.id && b.status != Status::Boarding)
                        });
                        if boarding {
                            out.push(Announce::Boarding);
                        }
                    }
                    self.movements = ms;
                    self.updated = Some(u.at);
                    self.failures = 0;
                    self.error = None;
                }
                Err(e) => {
                    self.failures += 1;
                    self.error = Some(e.to_string());
                }
            }
        }
        self.blinks.retain(|(_, since)| time - since < BLINK);
        out
    }

    /// Whether the board has gone too long without a good update.
    fn stale(&self, now: DateTime<Utc>) -> bool {
        self.updated
            .is_some_and(|at| now - at > Span::minutes(STALE))
    }

    /// Sets the cards for `now` on the board's clock, `time` on the
    /// window's.
    pub fn show(&mut self, time: f64, now: DateTime<Utc>) {
        let local = now.with_timezone(&self.tz);
        self.title
            .set_rows(&[Row::new(&[self.setup.direction.title()])], time);
        self.clock
            .set_rows(&[Row::new(&[local.format("%H:%M").to_string()])], time);
        self.name.set_rows(
            &[Row::new(&[self.setup.place.name.as_str()]).ink(WHITE)],
            time,
        );
        if let Some(b) = &mut self.second {
            b.set_rows(
                &[Row::new(&[self.setup.second_title.as_str()]).ink(WHITE)],
                time,
            );
        }

        // Movements gone since the last update leave now, not at the next.
        let ms = normalise::normalise(self.movements.clone(), now);
        // Only the next few pages, as a real board shows.
        let shown = &ms[..ms.len().min(self.setup.rows * self.setup.max_pages.max(1))];
        let pages = normalise::pages(shown, self.setup.rows);
        self.pages = pages.len();
        if self.page >= pages.len() {
            self.page = 0;
        }
        if pages.len() > 1 && time - self.page_since >= self.setup.page_secs {
            self.page = (self.page + 1) % pages.len();
            self.page_since = time;
        }
        let page = &pages[self.page];
        let cycle = (time / CODESHARE_EVERY) as usize;
        let stale = self.stale(now);
        let mut rows = normalise::rows(page, &self.setup.specs, cycle, stale);
        // A changed gate or platform blinks its lamp.
        let stand = self
            .setup
            .specs
            .iter()
            .position(|s| s.field == Field::Stand);
        for (row, m) in rows.iter_mut().zip(page) {
            if let (Some(col), Some((_, since))) =
                (stand, self.blinks.iter().find(|(id, _)| *id == m.id))
            {
                let on = (((time - since) / (BLINK / 6.0)) as usize).is_multiple_of(2);
                if on {
                    *row = row.clone().mark(col, WHITE);
                }
            }
        }
        self.grid.set_rows(&rows, time);

        // The ticker: the first train's calling points, or the notices.
        let width = self.ticker.columns()[0].cards;
        let text = self.ticker_text(&ms);
        let pieces = normalise::ticker_pieces(&text, width);
        if time - self.ticker_since >= TICKER_EVERY {
            self.ticker_piece += 1;
            self.ticker_since = time;
        }
        let piece = pieces
            .get(self.ticker_piece % pieces.len().max(1))
            .cloned()
            .unwrap_or_default();
        self.ticker.set_rows(&[Row::new(&[piece]).ink(WHITE)], time);
    }

    fn ticker_text(&self, ms: &[Movement]) -> String {
        let next = ms.iter().find(|m| !m.status.finished());
        match (self.setup.mode, next) {
            (Mode::Train, Some(m)) if !m.via.is_empty() => {
                let (verb, end) = match self.setup.direction {
                    Direction::Departure => ("CALLING AT", m.place.as_str()),
                    Direction::Arrival => ("FROM", m.place.as_str()),
                };
                let mut stops = m.via.join(", ");
                stops.push_str(&format!(" AND {end}"));
                format!(
                    "THE {} {verb} {stops}.  {}",
                    normalise::cell(m, Field::Time, 0),
                    self.attribution
                )
            }
            _ => format!(
                "WELCOME TO {}.  {}.",
                self.setup.place.name, self.attribution
            ),
        }
    }

    /// The footer's line: when it was last updated, who the data is from,
    /// and the page.
    fn footer(&self) -> String {
        let mut parts = Vec::new();
        if let Some(at) = self.updated {
            let t = at.with_timezone(&self.tz).format("%H:%M");
            parts.push(if self.failures >= FAILURES_SHOWN {
                format!("LAST UPDATED {t}")
            } else {
                format!("UPDATED {t}")
            });
        } else if let Some(e) = &self.error {
            parts.push(e.to_uppercase());
        } else {
            parts.push("WAITING FOR THE FIRST UPDATE".into());
        }
        parts.push(self.attribution.clone());
        if self.pages > 1 {
            parts.push(format!("PAGE {} OF {}", self.page + 1, self.pages));
        }
        parts.join("  ·  ")
    }

    /// Every board's flaps moved on to `time`.
    pub fn tick(&mut self, time: f64, speed: f64) -> Vec<Flip> {
        let mut out = Vec::new();
        for b in self.boards_mut() {
            out.extend(b.tick(time, speed));
        }
        out
    }

    pub fn busy(&self, time: f64) -> bool {
        self.title.busy(time)
            || self.clock.busy(time)
            || self.name.busy(time)
            || self.second.as_ref().is_some_and(|b| b.busy(time))
            || self.grid.busy(time)
            || self.ticker.busy(time)
            || !self.blinks.is_empty()
    }

    fn boards_mut(&mut self) -> Vec<&mut Board> {
        let mut v = vec![&mut self.title, &mut self.clock, &mut self.name];
        if let Some(b) = &mut self.second {
            v.push(b);
        }
        v.push(&mut self.grid);
        v.push(&mut self.ticker);
        v
    }

    /// The board's size in board units.
    pub fn size(&self) -> Vec2 {
        let w = self
            .grid
            .size()
            .x
            .max(self.title.size().x + self.clock.size().x + 20.0);
        let mut h = self.title.size().y + UNDER_TITLE + self.name.size().y;
        if let Some(b) = &self.second {
            h += 6.0 + b.size().y;
        }
        h += UNDER_HEADER + self.grid.size().y + ABOVE_TICKER + self.ticker.size().y + FOOTER;
        vec2(w, h)
    }

    /// Draws the board with its top left at `origin`, `scale` pixels to the
    /// board unit.
    pub fn draw(
        &self,
        ctx: &Context,
        painter: &Painter,
        origin: Pos2,
        scale: f32,
        style: &Style,
        light: Option<&LightMap>,
    ) {
        let at = |x: f32, y: f32| origin + vec2(x, y) * scale;
        let w = self.size().x;
        let mut y = 0.0;
        self.title
            .draw_lit(ctx, painter, at(0.0, y), scale, style, light);
        self.clock.draw_lit(
            ctx,
            painter,
            at(w - self.clock.size().x, y),
            scale,
            style,
            light,
        );
        y += self.title.size().y + UNDER_TITLE;
        self.name
            .draw_lit(ctx, painter, at(0.0, y), scale, style, light);
        y += self.name.size().y;
        if let Some(b) = &self.second {
            y += 6.0;
            b.draw_lit(ctx, painter, at(0.0, y), scale, style, light);
            y += b.size().y;
        }
        y += UNDER_HEADER;
        self.grid
            .draw_lit(ctx, painter, at(0.0, y), scale, style, light);
        y += self.grid.size().y + ABOVE_TICKER;
        self.ticker
            .draw_lit(ctx, painter, at(0.0, y), scale, style, light);
        y += self.ticker.size().y;
        let p = at(0.0, y + FOOTER * 0.6);
        let colour = light.map_or(style.caption, |l| l.light(p, style.caption));
        painter.text(
            p,
            Align2::LEFT_CENTER,
            self.footer(),
            FontId::new(11.0 * scale, FontFamily::Name(FONT.into())),
            colour,
        );
    }
}

pub const WHITE: Color32 = Color32::from_rgb(236, 234, 226);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticker_fills_the_board_width() {
        let n = cards_across(44.0 * CARD.x + 43.0 * CARD.y * SPACING);
        assert_eq!(n, 44);
    }
}
