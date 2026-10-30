//! A board: a fixed grid of cards in columns, under small captions, that
//! takes rows of text and flips to them.
//!
//! Each column is a run of letter cards, a run of time cards, or one wide
//! word flap. When rows change, the board works out the first row that
//! differs and sets every row from there down on its way, each a moment
//! after the one above, so a change ripples down the board. Each card flips
//! only as far as its new symbol, so when the top row leaves and the rest
//! move up, cards that keep their letter stay still.

use std::cell::RefCell;

use egui::{
    pos2, vec2, Align2, Color32, Context, FontFamily, FontId, Mesh, Painter, Pos2, Rect, Vec2,
};

use crate::flap::{Drum, Fall, Flap, Look};
use crate::fold::fit;
use crate::lighting::LightMap;
use crate::mesh::Cache;
use crate::rng::Rng;

/// The font family the cards are printed in; see [`crate::install_fonts`].
pub const FONT: &str = "flap";

/// A letter card, in board units.
pub const CARD: Vec2 = vec2(22.0, 32.0);
/// The space between cards, as a share of their height.
pub const SPACING: f32 = 0.07;
/// The space between columns and between rows, as a share of the cards'
/// height, and above the rows for the captions.
const COLUMN_GAP: f32 = 14.0 / 32.0;
const ROW_GAP: f32 = 9.0 / 32.0;
const CAPTIONS: f32 = 22.0;
/// The lit tab at the end of a row, beside the remark, and its gap.
const TAB: f32 = 7.0;
const TAB_GAP: f32 = 7.0;
/// How long each row waits after the one above when changes ripple down.
pub const STAGGER: f64 = 0.04;

/// One column of the board.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    /// The caption over it, such as `DESTINATION`; empty for none.
    pub title: String,
    /// How many letter cards wide it is.
    pub cards: usize,
    /// Its cards' drum: [`Drum::Alpha`] and [`Drum::Time`] give a card per
    /// letter; [`Drum::Words`] gives one wide card.
    pub drum: Drum,
}

impl Column {
    pub fn text(title: &str, cards: usize) -> Self {
        Self::new(title, cards, Drum::Alpha)
    }

    pub fn time(title: &str, cards: usize) -> Self {
        Self::new(title, cards, Drum::Time)
    }

    pub fn words(title: &str, cards: usize, words: &'static [&'static str]) -> Self {
        Self::new(title, cards, Drum::Words(words))
    }

    pub fn new(title: &str, cards: usize, drum: Drum) -> Self {
        Self {
            title: title.to_string(),
            cards: cards.max(1),
            drum,
        }
    }

    /// Whether the column is one wide card rather than a card a letter.
    fn wide(&self) -> bool {
        matches!(self.drum, Drum::Words(_))
    }

    /// The symbols its cards are to show for `text`.
    pub fn symbols(&self, text: &str) -> Vec<String> {
        if self.wide() {
            let word = fit(text, usize::MAX);
            return vec![if word.is_empty() { " ".into() } else { word }];
        }
        let text = match self.drum {
            Drum::Alpha => fit(text, self.cards),
            _ => text.chars().take(self.cards).collect(),
        };
        let mut out: Vec<String> = text.chars().map(|c| c.to_string()).collect();
        out.resize(self.cards, " ".into());
        out
    }

    /// Its width in board units, for cards `card` in size.
    fn width(&self, card: Vec2) -> f32 {
        let n = self.cards as f32;
        n * card.x + (n - 1.0) * card.y * SPACING
    }
}

/// A row to show: a text for each column, and how it is lit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub cells: Vec<String>,
    /// The letters' colour, or the board's when `None`.
    pub ink: Option<Color32>,
    /// A cell's cards in a colour of their own, by column, such as an
    /// operator's tile: the card's colour and its letters'.
    pub tiles: Vec<Option<(Color32, Color32)>>,
    /// A lit tab at the end of the row, as beside a remark that matters.
    pub tab: Option<Color32>,
}

impl Row {
    pub fn new<S: AsRef<str>>(cells: &[S]) -> Self {
        Self {
            cells: cells.iter().map(|s| s.as_ref().to_string()).collect(),
            ..Self::default()
        }
    }

    pub fn ink(mut self, ink: Color32) -> Self {
        self.ink = Some(ink);
        self
    }

    pub fn tab(mut self, tab: Color32) -> Self {
        self.tab = Some(tab);
        self
    }

    /// Gives column `col` a tile of `card` colour with `ink` letters.
    pub fn tile(mut self, col: usize, card: Color32, ink: Color32) -> Self {
        if self.tiles.len() <= col {
            self.tiles.resize(col + 1, None);
        }
        self.tiles[col] = Some((card, ink));
        self
    }

    fn cell(&self, col: usize) -> &str {
        self.cells.get(col).map_or("", String::as_str)
    }
}

/// A flap that has fallen somewhere on the board, for the sound.
#[derive(Clone, Copy, Debug)]
pub struct Flip {
    /// Where the card is, from -1 (left of the board) to 1 (right).
    pub pan: f32,
    /// The card's height in board units.
    pub size: f32,
    pub fall: Fall,
}

/// How the board is drawn, around and on the cards.
#[derive(Clone, Copy)]
pub struct Style {
    pub look: Look,
    pub caption: Color32,
    /// How dark the cards' shadows are, 0 for none; see
    /// [`crate::lighting::Mood::shadow`].
    pub shadow: f32,
}

struct Line {
    row: Row,
    /// Each column's cards.
    flaps: Vec<Vec<Flap>>,
}

pub struct Board {
    columns: Vec<Column>,
    lines: Vec<Line>,
    card: Vec2,
    rng: Rng,
    cache: RefCell<Cache>,
}

impl Board {
    /// A board of `rows` blank rows of small cards, which flip to their
    /// first rows a moment apart, as a board does when it is switched on.
    pub fn new(columns: Vec<Column>, rows: usize, now: f64, rng: Rng) -> Self {
        let mut b = Self {
            columns,
            lines: Vec::new(),
            card: CARD,
            rng,
            cache: RefCell::default(),
        };
        b.set_row_count(rows, now);
        b
    }

    /// The board with cards `card` in size, in board units.
    pub fn with_card(mut self, card: Vec2) -> Self {
        self.card = card;
        self
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn row_count(&self) -> usize {
        self.lines.len()
    }

    /// Adds blank rows, or takes rows off the foot of the board.
    pub fn set_row_count(&mut self, rows: usize, now: f64) {
        self.lines.truncate(rows);
        while self.lines.len() < rows {
            let flaps = self
                .columns
                .iter()
                .map(|c| {
                    let n = if c.wide() { 1 } else { c.cards };
                    (0..n)
                        .map(|_| {
                            let (delay, pace) = self.rng.scatter();
                            Flap::new(c.drum, now, delay, pace)
                        })
                        .collect()
                })
                .collect();
            self.lines.push(Line {
                row: Row::default(),
                flaps,
            });
        }
    }

    /// Shows `rows`, from the top; rows past the end are blank. Rows from
    /// the first that differs down are set flipping, each [`STAGGER`] after
    /// the one above; rows above it, and cards that already show their
    /// symbol, stay still.
    pub fn set_rows(&mut self, rows: &[Row], now: f64) {
        let blank = Row::default();
        let mut first = None;
        for (i, line) in self.lines.iter_mut().enumerate() {
            let row = rows.get(i).unwrap_or(&blank);
            let mut changed = false;
            for (c, col) in self.columns.iter().enumerate() {
                let want = col.symbols(row.cell(c));
                if line.flaps[c]
                    .iter()
                    .zip(&want)
                    .any(|(f, w)| f.target() != w)
                {
                    changed = true;
                    let from = *first.get_or_insert(i);
                    let delay = (i - from) as f64 * STAGGER;
                    for (f, w) in line.flaps[c].iter_mut().zip(&want) {
                        f.set_after(w, now, delay);
                    }
                }
            }
            if changed || line.row != *row {
                line.row = row.clone();
            }
        }
    }

    /// The rows as they were last set.
    pub fn rows(&self) -> impl Iterator<Item = &Row> {
        self.lines.iter().map(|l| &l.row)
    }

    /// What the cards show just now, row by row, a string a column: for
    /// tests and for checking the board has landed.
    pub fn shown(&self) -> Vec<Vec<String>> {
        self.lines
            .iter()
            .map(|l| {
                l.flaps
                    .iter()
                    .map(|cards| cards.iter().map(Flap::shown).collect())
                    .collect()
            })
            .collect()
    }

    /// Spins every card once round its drum and lands it where it was, as a
    /// board refreshing; row by row from the top.
    pub fn refresh(&mut self, now: f64) {
        for (i, line) in self.lines.iter_mut().enumerate() {
            for cards in &mut line.flaps {
                for f in cards {
                    let (delay, pace) = self.rng.scatter();
                    let target = f.target().to_string();
                    f.turn(&target, now, i as f64 * STAGGER + delay * 0.3, pace);
                }
            }
        }
    }

    /// Moves every card on to `now`, flaps falling `speed` times as fast as
    /// usual, and says which flaps fell.
    pub fn tick(&mut self, now: f64, speed: f64) -> Vec<Flip> {
        let width = self.size().x;
        let mut out = Vec::new();
        for line in &mut self.lines {
            let mut x = 0.0;
            for (col, cards) in self.columns.iter().zip(&mut line.flaps) {
                let w = if col.wide() {
                    col.width(self.card)
                } else {
                    self.card.x
                };
                for f in cards {
                    if let Some(fall) = f.tick(now, speed) {
                        out.push(Flip {
                            pan: (x + w / 2.0) / width * 2.0 - 1.0,
                            size: self.card.y,
                            fall,
                        });
                    }
                    x += w + self.card.y * SPACING;
                }
                x += self.card.y * (COLUMN_GAP - SPACING);
            }
        }
        out
    }

    /// Whether any card is still flipping or waiting to.
    pub fn busy(&self, now: f64) -> bool {
        self.lines
            .iter()
            .any(|l| l.flaps.iter().flatten().any(|f| f.busy(now)))
    }

    fn has_captions(&self) -> bool {
        self.columns.iter().any(|c| !c.title.is_empty())
    }

    fn has_tabs(&self) -> bool {
        self.columns.iter().any(Column::wide)
    }

    /// The board's size in board units.
    pub fn size(&self) -> Vec2 {
        let cols: f32 = self.columns.iter().map(|c| c.width(self.card)).sum();
        let gaps = self.card.y * COLUMN_GAP * self.columns.len().saturating_sub(1) as f32;
        let tabs = if self.has_tabs() { TAB_GAP + TAB } else { 0.0 };
        let n = self.lines.len() as f32;
        let rows = n * self.card.y + (n - 1.0).max(0.0) * self.card.y * ROW_GAP;
        let captions = if self.has_captions() { CAPTIONS } else { 0.0 };
        vec2(cols + gaps + tabs, captions + rows)
    }

    /// Draws the board with its top left at `origin`, `scale` pixels to the
    /// board unit.
    pub fn draw(&self, ctx: &Context, painter: &Painter, origin: Pos2, scale: f32, style: &Style) {
        self.draw_lit(ctx, painter, origin, scale, style, None);
    }

    /// Draws the board as [`Board::draw`] does, lit by the lamps' `light`:
    /// for a board whose backdrop is lit the same way, rather than with
    /// the lamps' light laid over everything afterwards.
    pub fn draw_lit(
        &self,
        ctx: &Context,
        painter: &Painter,
        origin: Pos2,
        scale: f32,
        style: &Style,
        light: Option<&LightMap>,
    ) {
        let lit = |p: Pos2, c: Color32| light.map_or(c, |l| l.light(p, c));
        let u = |x: f32, y: f32| origin + vec2(x, y) * scale;
        let family = FontFamily::Name(FONT.into());
        let mut y = 0.0;
        if self.has_captions() {
            let caption = FontId::new(11.0 * scale, family.clone());
            let mut x = 0.0;
            for col in &self.columns {
                painter.text(
                    u(x, CAPTIONS - 8.0),
                    Align2::LEFT_BOTTOM,
                    &col.title,
                    caption.clone(),
                    lit(u(x, CAPTIONS - 8.0), style.caption),
                );
                x += col.width(self.card) + self.card.y * COLUMN_GAP;
            }
            y = CAPTIONS;
        }
        let font = FontId::new(self.card.y * 0.86 * scale, family);
        let size = self.card * scale;
        let now = ctx.input(|i| i.time);
        let ppp = ctx.pixels_per_point();
        // Cards standing still come from meshes made once and kept, all in
        // one mesh; only cards with a flap falling are drawn afresh.
        let mut cache = self.cache.borrow_mut();
        let cache = &mut *cache;
        cache.check(ctx, ppp);
        let mut still = Mesh::default();
        let mut moving = Vec::new();
        let snap = |p: Pos2| pos2((p.x * ppp).round() / ppp, (p.y * ppp).round() / ppp);
        // Lit from above, the cards throw soft shadows down onto the board:
        // one for each column's run of cards, which looks the same as one a
        // card and costs a tenth as much.
        let mut runs = Vec::new();
        for line in &self.lines {
            let ink = line.row.ink.unwrap_or(style.look.ink);
            let mut x = 0.0;
            for (c, (col, cards)) in self.columns.iter().zip(&line.flaps).enumerate() {
                let look = match line.row.tiles.get(c).copied().flatten() {
                    Some((card, ink)) => Look {
                        top: card,
                        bottom: darken(card, 0.9),
                        ink,
                        ..style.look
                    },
                    None => Look { ink, ..style.look },
                };
                let w = if col.wide() {
                    col.width(self.card) * scale
                } else {
                    size.x
                };
                let run =
                    Rect::from_min_size(snap(u(x, y)), vec2(col.width(self.card) * scale, size.y));
                runs.push(run);
                for f in cards {
                    let r = Rect::from_min_size(snap(u(x, y)), vec2(w, size.y));
                    if f.flipping(now) {
                        moving.push((f, r, look));
                    } else {
                        let mesh = cache.still(ctx, painter, f, r.size(), &font, &look);
                        let base = still.vertices.len() as u32;
                        still
                            .vertices
                            .extend(mesh.vertices.iter().map(|v| egui::epaint::Vertex {
                                pos: v.pos + r.min.to_vec2(),
                                ..*v
                            }));
                        still.indices.extend(mesh.indices.iter().map(|i| base + i));
                    }
                    x += w / scale + self.card.y * SPACING;
                }
                x += self.card.y * (COLUMN_GAP - SPACING);
            }
            y += self.card.y * (1.0 + ROW_GAP);
        }
        let mut by_hand = Vec::new();
        for (f, r, look) in moving {
            if !cache.flipping(ctx, painter, f, r, &font, &look, now, &mut still) {
                by_hand.push((f, r, look));
            }
        }
        if style.shadow > 0.0 {
            let shadows = cache.shadows(&runs, style.shadow);
            match light {
                Some(l) => {
                    let mut m = (*shadows).clone();
                    l.light_mesh(&mut m);
                    painter.add(m);
                }
                None => {
                    painter.add(shadows);
                }
            }
        }
        if let Some(l) = light {
            l.light_mesh(&mut still);
        }
        painter.add(still);
        for (f, r, look) in by_hand {
            f.draw(ctx, painter, r, &font, &look);
        }
        // The lit tabs, beside the remarks.
        let mut y = if self.has_captions() { CAPTIONS } else { 0.0 };
        for line in &self.lines {
            if let Some(tab) = line.row.tab {
                let x = self.size().x - TAB;
                let r = Rect::from_min_size(
                    u(x, y + self.card.y * 0.1),
                    vec2(TAB, self.card.y * 0.8) * scale,
                );
                painter.rect_filled(r, 1.5 * scale, tab);
                // Its glow on the board around it.
                painter.add(crate::lighting::halo(
                    r.center(),
                    r.width() * 1.6,
                    r.height() * 0.8,
                    Color32::from_rgba_premultiplied(tab.r() / 5, tab.g() / 5, tab.b() / 5, 0),
                ));
            }
            y += self.card.y * (1.0 + ROW_GAP);
        }
    }
}

fn darken(c: Color32, k: f32) -> Color32 {
    let f = |x: u8| (x as f32 * k).round() as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

/// Where a board of `size` units goes to sit centred in `rect`, as large as
/// fits with `pad` units around it: its origin and scale.
pub fn fit_in(rect: Rect, size: Vec2, pad: f32) -> (Pos2, f32) {
    let outer = size + Vec2::splat(pad * 2.0);
    let scale = (rect.width() / outer.x).min(rect.height() / outer.y);
    let origin = rect.center() - size * scale / 2.0;
    (pos2(origin.x, origin.y), scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flap::REMARKS;

    fn departures(rows: usize) -> Board {
        Board::new(
            vec![
                Column::time("TIME", 5),
                Column::text("FLIGHT", 7),
                Column::text("DESTINATION", 16),
                Column::text("GATE", 4),
                Column::words("REMARKS", 12, REMARKS),
            ],
            rows,
            0.0,
            Rng::with_seed(1),
        )
    }

    /// Runs the board until every card has landed, from `t`.
    fn settle(b: &mut Board, mut t: f64) -> (f64, usize) {
        let mut flips = 0;
        while b.busy(t) {
            flips += b.tick(t, 1.0).len();
            t += 0.005;
            assert!(t < 120.0, "the board never settled");
        }
        (t, flips)
    }

    fn row(time: &str, flight: &str, to: &str, gate: &str, remark: &str) -> Row {
        Row::new(&[time, flight, to, gate, remark])
    }

    #[test]
    fn cells_become_cards() {
        let b = departures(1);
        let c = &b.columns()[2];
        assert_eq!(c.symbols("São Paulo").concat(), "SAO PAULO       ");
        assert_eq!(c.symbols("").len(), 16);
        assert_eq!(b.columns()[4].symbols("Final call"), ["FINAL CALL"]);
        assert_eq!(b.columns()[4].symbols(""), [" "]);
        assert_eq!(b.columns()[0].symbols("9:05"), ["9", ":", "0", "5", " "]);
    }

    #[test]
    fn a_board_lands_on_its_rows() {
        let mut b = departures(3);
        b.set_rows(
            &[
                row("14:05", "QF 9", "London", "5", "Boarding"),
                row("14:20", "NZ 124", "Auckland", "11", "On time"),
            ],
            0.0,
        );
        settle(&mut b, 0.0);
        let shown = b.shown();
        assert_eq!(shown[0][0], "14:05");
        assert_eq!(shown[0][2].trim_end(), "LONDON");
        assert_eq!(shown[0][4], "BOARDING");
        assert_eq!(shown[1][1].trim_end(), "NZ 124");
        // The third row is blank cards.
        assert!(shown[2].iter().all(|s| s.trim().is_empty()));
    }

    #[test]
    fn rows_moving_up_ripple_down_from_the_top() {
        let a = row("14:05", "QF 9", "LONDON", "5", "DEPARTED");
        let b_ = row("14:20", "NZ 124", "AUCKLAND", "11", "BOARDING");
        let c = row("14:35", "SQ 238", "SINGAPORE", "9", "ON TIME");
        let mut b = departures(3);
        b.set_rows(&[a.clone(), b_.clone(), c.clone()], 0.0);
        let (t, _) = settle(&mut b, 0.0);
        // The top row leaves and the rest move up.
        b.set_rows(&[b_.clone(), c.clone()], t);
        // Each row sets off a stagger after the one above.
        let mut started = [None; 3];
        let mut now = t;
        while b.busy(now) {
            let before = b.shown();
            b.tick(now, 1.0);
            let after = b.shown();
            for r in 0..3 {
                if started[r].is_none() && before[r] != after[r] {
                    started[r] = Some(now - t);
                }
            }
            now += 0.001;
        }
        let s = started.map(|s| s.expect("every row changed"));
        assert!(s[0] < s[1] && s[1] < s[2], "{s:?}");
        assert!((s[1] - s[0] - STAGGER).abs() < 0.01, "{s:?}");
        assert_eq!(b.shown()[0][1].trim_end(), "NZ 124");
        assert!(b.shown()[2].iter().all(|s| s.trim().is_empty()));
    }

    #[test]
    fn unchanged_rows_and_cards_stay_still() {
        let mut b = departures(2);
        let top = row("14:05", "QF 9", "LONDON", "5", "BOARDING");
        b.set_rows(
            &[top.clone(), row("14:20", "NZ 1", "AUCKLAND", "1", "")],
            0.0,
        );
        let (t, _) = settle(&mut b, 0.0);
        // Only the second row's remark changes: one flap falls, two at most
        // (ON TIME is one flip from blank).
        b.set_rows(&[top, row("14:20", "NZ 1", "AUCKLAND", "1", "ON TIME")], t);
        let (_, flips) = settle(&mut b, t);
        assert_eq!(flips, 1);
    }

    #[test]
    fn the_board_has_a_size_for_its_rows() {
        let b = departures(12);
        let s = b.size();
        let b24 = departures(24);
        assert_eq!(b24.size().x, s.x);
        assert!((b24.size().y - s.y - 12.0 * CARD.y * (1.0 + ROW_GAP)).abs() < 0.01);
        // 44 cards and four column gaps, and the tab.
        assert!(s.x > 44.0 * CARD.x && s.x < 44.0 * CARD.x * 1.2, "{}", s.x);
    }

    #[test]
    fn flips_are_placed_across_the_board() {
        let mut b = departures(1);
        b.set_rows(&[row("", "", "", "", "BOARDING")], 0.0);
        let mut flips = Vec::new();
        let mut t = 0.0;
        while b.busy(t) {
            flips.extend(b.tick(t, 1.0));
            t += 0.005;
        }
        // The remarks are at the right of the board.
        assert!(!flips.is_empty());
        assert!(flips.iter().all(|f| f.pan > 0.4 && f.pan < 1.0));
    }
}
