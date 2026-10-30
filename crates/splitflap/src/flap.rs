//! One split-flap card: what it shows, how it steps between symbols, and how
//! it is drawn.
//!
//! A real card sits on a drum of flaps and can only move forward, one flap at
//! a time. Each card here has a drum too, so a change from `B` to `F` flips
//! through `C`, `D` and `E` on the way, quickly, and the last flip lands at
//! normal speed.

use std::collections::VecDeque;
use std::f32::consts::PI;

use egui::epaint::{ClippedShape, PathShape, Primitive};
use egui::{
    pos2, vec2, Color32, Context, CornerRadius, FontId, Mesh, Painter, Pos2, Rect, Shape, Stroke,
};

/// The duration of a flip that lands on the wanted symbol, in seconds.
const FLIP: f64 = 0.30;
/// The duration of a flip on the way to it.
const FAST: f64 = 0.07;

/// The letter drum's flaps: everything a name or a flight number can show.
pub const ALPHA: &str = " ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.-+:/'&,()";
/// The time drum's flaps: short, so times and gate numbers land quickly.
pub const TIME: &str = " 0123456789:";

/// The remarks printed on a board's word flaps, one phrase a flap, in the
/// order a flight or a train goes through them, so the usual changes take
/// few flips. The blank comes first, as on every drum.
pub const REMARKS: &[&str] = &[
    " ",
    "ON TIME",
    "EXPECTED",
    "DELAYED",
    "GATE OPEN",
    "BOARDING",
    "FINAL CALL",
    "GATE CLOSED",
    "DEPARTED",
    "LANDED",
    "ARRIVED",
    "BAGGAGE",
    "DIVERTED",
    "CANCELLED",
    "PLATFORM CHANGE",
    "SEE SCREENS",
];

/// The sky card's flaps, which are pictures rather than letters.
pub const MORNING: &str = "<morning>";
pub const DAY: &str = "<day>";
pub const EVENING: &str = "<evening>";
pub const NIGHT: &str = "<night>";

/// The symbols a card can show, in the order they come round.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Drum {
    /// 0 to 9: the units of hours, minutes and seconds.
    Digit,
    /// 0 to 5: the tens of minutes and seconds.
    Tens6,
    /// 0 to 2: the tens of hours on a 24-hour clock.
    Tens3,
    /// Blank or 1: the tens of hours on a 12-hour clock.
    Tens12,
    /// AM and PM.
    AmPm,
    /// Letters, digits and a little punctuation, for names and dates.
    Alpha,
    /// The sky: night, morning, day and evening, in the order a day goes.
    Sky,
    /// Blank, 0 to 9 and a colon: a board's times and gate numbers.
    Time,
    /// Whole phrases, one to a flap, as on a board's remarks: a wide card
    /// that says DELAYED in one flip rather than spelling it.
    Words(&'static [&'static str]),
}

impl Drum {
    fn len(self) -> usize {
        match self {
            Drum::Digit => 10,
            Drum::Tens6 => 6,
            Drum::Tens3 => 3,
            Drum::Tens12 => 2,
            Drum::AmPm => 2,
            Drum::Alpha => ALPHA.len(),
            Drum::Sky => 4,
            Drum::Time => TIME.len(),
            Drum::Words(w) => w.len(),
        }
    }

    fn symbol(self, i: usize) -> &'static str {
        match self {
            Drum::Digit => &"0123456789"[i..=i],
            Drum::Tens6 => &"012345"[i..=i],
            Drum::Tens3 => &"012"[i..=i],
            Drum::Tens12 => &" 1"[i..=i],
            Drum::AmPm => ["AM", "PM"][i],
            Drum::Alpha => &ALPHA[i..=i],
            Drum::Sky => [NIGHT, MORNING, DAY, EVENING][i],
            Drum::Time => &TIME[i..=i],
            Drum::Words(w) => w[i],
        }
    }

    fn index(self, s: &str) -> Option<usize> {
        (0..self.len()).find(|&i| self.symbol(i) == s)
    }

    /// The flaps that come round on the way from `from` to `to`, ending with
    /// `to`. A symbol the drum doesn't have is reached in one flip.
    pub fn steps(self, from: &str, to: &str) -> Vec<String> {
        let Some(b) = self.index(to) else {
            return vec![to.to_string()];
        };
        // A card that shows something off the drum (a blank digit card when
        // the clock starts) comes round from the drum's first flap.
        let mut i = match self.index(from) {
            Some(a) if a == b => return Vec::new(),
            Some(a) => (a + 1) % self.len(),
            None => 0,
        };
        let mut out = Vec::new();
        loop {
            out.push(self.symbol(i).to_string());
            if i == b {
                return out;
            }
            i = (i + 1) % self.len();
        }
    }

    /// One whole turn of the drum from `from`, then on to `to`: what a
    /// card does when the board refreshes.
    pub fn turn(self, from: &str, to: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(a) = self.index(from) {
            out.extend((1..=self.len()).map(|k| self.symbol((a + k) % self.len()).to_string()));
        }
        out.extend(self.steps(from, to));
        out
    }
}

/// The colours a card is drawn in.
#[derive(Clone, Copy)]
pub struct Look {
    pub top: Color32,
    pub bottom: Color32,
    pub split: Color32,
    pub hinge: Color32,
    pub ink: Color32,
}

/// A flap that has just fallen, for the sound.
#[derive(Clone, Copy, Debug)]
pub struct Fall {
    /// The last flip, onto the symbol the card was after.
    pub land: bool,
    /// Part of a start-up or a refresh, rather than the time moving on.
    pub show: bool,
}

pub struct Flap {
    drum: Drum,
    /// What the card showed before the flip under way.
    prev: String,
    /// What the flip under way lands on.
    cur: String,
    /// What the card is on its way to.
    target: String,
    queue: VecDeque<String>,
    start: f64,
    dur: f64,
    /// No flipping before this time, so cards set off a moment apart.
    hold: f64,
    /// How long this card's quick flips take, as a share of the usual: no
    /// two mechanisms on a real board run at quite the same speed.
    pace: f64,
    /// Whether the flips under way are a start-up or a refresh.
    show: bool,
}

impl Flap {
    /// A blank card that starts flipping `delay` seconds after `now`, its
    /// quick flips taking `pace` times as long as usual.
    pub fn new(drum: Drum, now: f64, delay: f64, pace: f64) -> Self {
        Self {
            drum,
            prev: " ".into(),
            cur: " ".into(),
            target: " ".into(),
            queue: VecDeque::new(),
            start: 0.0,
            dur: 0.0,
            hold: now + delay,
            pace,
            show: true,
        }
    }

    pub fn drum(&self) -> Drum {
        self.drum
    }

    /// What the card shows, or is landing on.
    pub fn shown(&self) -> &str {
        &self.cur
    }

    /// What the card is on its way to.
    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn set(&mut self, target: &str) {
        if self.target == target {
            return;
        }
        self.target = target.to_string();
        self.queue = self.drum.steps(&self.cur, target).into();
        self.pace = 1.0;
    }

    /// Sets the card on its way to `target`, setting off no sooner than
    /// `delay` seconds after `now`: for a board whose rows ripple down one
    /// after another.
    pub fn set_after(&mut self, target: &str, now: f64, delay: f64) {
        if self.target == target {
            return;
        }
        if !self.busy(now) {
            self.hold = self.hold.max(now + delay);
        }
        self.set(target);
    }

    /// Spins the card once round its drum, starting `delay` seconds after
    /// `now` with its quick flips taking `pace` times as long as usual, and
    /// lands it on `target`.
    pub fn turn(&mut self, target: &str, now: f64, delay: f64, pace: f64) {
        self.target = target.to_string();
        self.queue = self.drum.turn(&self.cur, target).into();
        self.hold = self.hold.max(now + delay);
        self.pace = pace;
        self.show = true;
    }

    fn progress(&self, now: f64) -> f32 {
        if self.dur <= 0.0 {
            1.0
        } else {
            ((now - self.start) / self.dur).clamp(0.0, 1.0) as f32
        }
    }

    /// Moves the card on to its next flap when the current flip is done,
    /// and says so. `speed` is how fast flaps fall, 1 being the usual.
    pub fn tick(&mut self, now: f64, speed: f64) -> Option<Fall> {
        if now < self.hold || self.progress(now) < 1.0 {
            return None;
        }
        let next = self.queue.pop_front()?;
        self.prev = std::mem::replace(&mut self.cur, next);
        self.start = now;
        let land = self.queue.is_empty();
        self.dur = if land { FLIP } else { FAST * self.pace } / speed.max(0.05);
        let fall = Fall {
            land,
            show: self.show,
        };
        if land {
            // From here on, changes are just the time moving on.
            self.show = false;
        }
        Some(fall)
    }

    pub fn busy(&self, now: f64) -> bool {
        !self.queue.is_empty() || self.progress(now) < 1.0
    }

    /// How far the flip under way has gone, from 0 to 1; 1 when the card
    /// is standing still.
    pub fn progress_at(&self, now: f64) -> f32 {
        self.progress(now)
    }

    /// What the card showed before the flip under way.
    pub fn previous(&self) -> &str {
        &self.prev
    }

    /// Whether a flap is falling just now, rather than the card standing
    /// still between flips.
    pub fn flipping(&self, now: f64) -> bool {
        self.progress(now) < 1.0
    }

    pub fn draw(&self, ctx: &Context, painter: &Painter, rect: Rect, font: &FontId, look: &Look) {
        let now = ctx.input(|i| i.time);
        let p = self.progress(now);
        let flipping = p < 1.0;

        let h = rect.height();
        let r = (h * 0.07).round().max(1.0) as u8;
        let gap = (h * 0.025).max(1.0);
        let mid = rect.center().y;
        let top = Rect::from_min_max(rect.min, pos2(rect.max.x, mid - gap / 2.0));
        let bottom = Rect::from_min_max(pos2(rect.min.x, mid + gap / 2.0), rect.max);
        let top_r = CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        };
        let bottom_r = CornerRadius {
            nw: 0,
            ne: 0,
            sw: r,
            se: r,
        };

        // The split, and the card's halves behind any moving flap: the new
        // symbol's top is already showing; the old one's bottom stays until
        // the flap lands on it.
        painter.rect_filled(rect.shrink2(egui::vec2(0.0, h * 0.02)), r, look.split);
        painter.rect_filled(top, top_r, look.top);
        painter.rect_filled(bottom, bottom_r, look.bottom);
        let under = if flipping { &self.prev } else { &self.cur };
        painter
            .with_clip_rect(top.intersect(painter.clip_rect()))
            .add(glyph(painter, rect, &self.cur, font, look.ink));
        painter
            .with_clip_rect(bottom.intersect(painter.clip_rect()))
            .add(glyph(painter, rect, under, font, look.ink));

        if flipping {
            if p < 0.5 {
                // The old top folds down towards the hinge, darkening as it
                // turns away from the light, and shades the half below it.
                let s = (PI * p).cos();
                let hinge = top.max.y;
                let flap = Rect::from_min_max(pos2(top.min.x, hinge - top.height() * s), top.max);
                let shade = 0.45 + 0.55 * s;
                painter.rect_filled(
                    bottom,
                    bottom_r,
                    Color32::from_black_alpha((70.0 * (1.0 - s)) as u8),
                );
                painter.rect_filled(flap, top_r, darken(look.top, shade));
                squashed(
                    ctx,
                    &painter.with_clip_rect(flap.intersect(painter.clip_rect())),
                    glyph(painter, rect, &self.prev, font, look.ink),
                    hinge,
                    s,
                    shade,
                );
            } else {
                // The new bottom swings down from the hinge.
                let s = -(PI * p).cos();
                let hinge = bottom.min.y;
                let flap =
                    Rect::from_min_max(bottom.min, pos2(bottom.max.x, hinge + bottom.height() * s));
                let shade = 0.6 + 0.4 * s;
                painter.rect_filled(flap, bottom_r, darken(look.bottom, shade));
                squashed(
                    ctx,
                    &painter.with_clip_rect(flap.intersect(painter.clip_rect())),
                    glyph(painter, rect, &self.cur, font, look.ink),
                    hinge,
                    s,
                    shade,
                );
            }
        }

        // The pins the flaps hang on.
        let pin = egui::vec2((rect.width() * 0.05).max(1.5), gap * 2.6);
        for x in [rect.min.x + pin.x / 2.0, rect.max.x - pin.x / 2.0] {
            painter.rect_filled(Rect::from_center_size(pos2(x, mid), pin), 1.0, look.hinge);
        }
    }
}

/// `text` centred on the card, with every symbol on the same baseline.
pub(crate) fn glyph(
    painter: &Painter,
    card: Rect,
    text: &str,
    font: &FontId,
    ink: Color32,
) -> Shape {
    if let Some(picture) = picture(card, text, ink) {
        return picture;
    }
    let mut font = font.clone();
    let mut galley = painter.layout_no_wrap(text.to_string(), font.clone(), ink);
    // A phrase too wide for its card is printed smaller, as the words on a
    // remarks flap were.
    let room = card.width() * 0.92;
    if galley.size().x > room {
        font.size *= room / galley.size().x;
        galley = painter.layout_no_wrap(text.to_string(), font.clone(), ink);
    }
    // Centre on the ink of a digit rather than on the line, which has room
    // for descenders below.
    let reference = painter.layout_no_wrap("0".into(), font, ink);
    let ink_mid = reference
        .rows
        .first()
        .map(|row| row.pos.y + row.visuals.mesh_bounds.center().y)
        .unwrap_or(reference.size().y / 2.0);
    let pos = pos2(
        card.center().x - galley.size().x / 2.0,
        card.center().y - ink_mid,
    );
    Shape::galley(pos, galley, ink)
}

/// The sky card's pictures, solid in the card's ink: the sun rising over the
/// horizon, the sun, the sun setting into the sea, and the moon and stars.
fn picture(card: Rect, text: &str, ink: Color32) -> Option<Shape> {
    let c = card.center();
    // Half the picture's size.
    let u = (card.width() * 0.9).min(card.height() * 0.64) / 2.0;
    let line = Stroke::new(u * 0.13, ink);
    let ray = |centre: Pos2, angle: f32, from: f32, to: f32| {
        let d = vec2(angle.cos(), angle.sin());
        Shape::line_segment([centre + d * from, centre + d * to], line)
    };
    let mut shapes = Vec::new();
    match text {
        DAY => {
            shapes.push(Shape::circle_filled(c, u * 0.5, ink));
            for k in 0..8 {
                shapes.push(ray(c, k as f32 * PI / 4.0, u * 0.7, u * 0.98));
            }
        }
        MORNING => {
            let horizon = c.y + u * 0.3;
            let sun = pos2(c.x, horizon);
            shapes.push(Shape::convex_polygon(
                half_disc(sun, u * 0.55),
                ink,
                Stroke::NONE,
            ));
            for k in 0..5 {
                let a = PI + PI * (k as f32 + 1.0) / 6.0;
                shapes.push(ray(sun, a, u * 0.72, u * 0.98));
            }
            let h = |y: f32, half: f32| {
                Shape::line_segment([pos2(c.x - half, y), pos2(c.x + half, y)], line)
            };
            shapes.push(h(horizon + u * 0.08, u * 0.98));
            shapes.push(h(horizon + u * 0.36, u * 0.6));
        }
        EVENING => {
            let r = u * 0.78;
            shapes.push(Shape::convex_polygon(half_disc(c, r), ink, Stroke::NONE));
            // The lower half, sinking: bars across it, as the chords there.
            for dy in [0.2, 0.46, 0.72] {
                let y = c.y + r * dy;
                let half = (r * r - (r * dy) * (r * dy)).sqrt();
                shapes.push(Shape::line_segment(
                    [pos2(c.x - half, y), pos2(c.x + half, y)],
                    line,
                ));
            }
        }
        NIGHT => {
            shapes.push(crescent(c + vec2(-u * 0.12, u * 0.05), u * 0.72, ink));
            shapes.push(star(c + vec2(u * 0.6, -u * 0.58), u * 0.24, ink));
            shapes.push(star(c + vec2(u * 0.78, u * 0.06), u * 0.16, ink));
        }
        _ => return None,
    }
    Some(Shape::Vec(shapes))
}

/// The top half of a disc, as a polygon.
fn half_disc(centre: Pos2, r: f32) -> Vec<Pos2> {
    (0..=24)
        .map(|k| {
            let a = PI + PI * k as f32 / 24.0;
            centre + vec2(a.cos(), a.sin()) * r
        })
        .collect()
}

/// A crescent moon: a disc of radius `r` at `centre` with a bite out of its
/// upper right, filled as a strip between its two edges, and outlined so
/// the edges are smooth.
fn crescent(centre: Pos2, r: f32, ink: Color32) -> Shape {
    let bite = centre + vec2(r * 0.42, -r * 0.3);
    let br = r * 0.82;
    // The tips, where the two circles cross, as angles round each circle.
    let d = (bite - centre).length();
    let a = (r * r - br * br + d * d) / (2.0 * d);
    let h = (r * r - a * a).max(0.0).sqrt();
    let along = (bite - centre) / d;
    let mid = centre + along * a;
    let across = vec2(-along.y, along.x);
    let (t1, t2) = (mid + across * h, mid - across * h);
    let angle = |o: Pos2, p: Pos2| (p.y - o.y).atan2(p.x - o.x);
    // The outer edge goes the long way round, away from the bite; the inner
    // edge the short way, inside the moon.
    let o1 = angle(centre, t1);
    let o2 = arc_through(o1, angle(centre, t2), angle(centre, bite) + PI);
    let i1 = angle(bite, t1);
    let i2 = arc_through(i1, angle(bite, t2), angle(bite, centre));
    const N: usize = 28;
    let point = |o: Pos2, rr: f32, a: f32| o + vec2(a.cos(), a.sin()) * rr;
    let outer: Vec<Pos2> = (0..=N)
        .map(|k| point(centre, r, o1 + (o2 - o1) * k as f32 / N as f32))
        .collect();
    let inner: Vec<Pos2> = (0..=N)
        .map(|k| point(bite, br, i1 + (i2 - i1) * k as f32 / N as f32))
        .collect();
    let mut mesh = Mesh::default();
    for k in 0..=N {
        mesh.colored_vertex(outer[k], ink);
        mesh.colored_vertex(inner[k], ink);
    }
    for k in 0..N as u32 {
        let (a, b, c, d) = (2 * k, 2 * k + 1, 2 * k + 2, 2 * k + 3);
        mesh.add_triangle(a, b, c);
        mesh.add_triangle(b, d, c);
    }
    let mut edge = outer;
    edge.extend(inner.into_iter().rev());
    Shape::Vec(vec![
        Shape::mesh(mesh),
        Shape::Path(PathShape::closed_line(edge, Stroke::new(r * 0.04, ink))),
    ])
}

/// The angle to go round to from `a` to reach `b`, by whichever way round
/// passes `via`: `b` itself, or `b` a whole turn off, so that going from `a`
/// to the result in a straight line of angles takes that way.
fn arc_through(a: f32, b: f32, via: f32) -> f32 {
    let turn = 2.0 * PI;
    let forward = (b - a).rem_euclid(turn);
    if (via - a).rem_euclid(turn) <= forward {
        a + forward
    } else {
        a - (turn - forward)
    }
}

/// A four-pointed twinkle of radius `r`.
fn star(centre: Pos2, r: f32, ink: Color32) -> Shape {
    let points: Vec<Pos2> = (0..8)
        .map(|k| {
            let a = k as f32 * PI / 4.0 - PI / 2.0;
            let rr = if k % 2 == 0 { r } else { r * 0.32 };
            centre + vec2(a.cos(), a.sin()) * rr
        })
        .collect();
    let mut mesh = Mesh::default();
    mesh.colored_vertex(centre, ink);
    for p in &points {
        mesh.colored_vertex(*p, ink);
    }
    for k in 0..8u32 {
        mesh.add_triangle(0, 1 + k, 1 + (k + 1) % 8);
    }
    Shape::Vec(vec![
        Shape::mesh(mesh),
        Shape::Path(PathShape::closed_line(points, Stroke::new(r * 0.08, ink))),
    ])
}

/// Draws `shape` squashed towards the line at `hinge` by `s`, as a flap
/// seen turning edge-on.
fn squashed(ctx: &Context, painter: &Painter, shape: Shape, hinge: f32, s: f32, shade: f32) {
    let clipped = ClippedShape {
        clip_rect: Rect::EVERYTHING,
        shape,
    };
    for prim in ctx.tessellate(vec![clipped], ctx.pixels_per_point()) {
        if let Primitive::Mesh(mut mesh) = prim.primitive {
            for v in &mut mesh.vertices {
                v.pos.y = hinge + (v.pos.y - hinge) * s;
                v.color = darken(v.color, shade);
            }
            painter.add(Shape::mesh(mesh));
        }
    }
}

pub(crate) fn darken(c: Color32, k: f32) -> Color32 {
    let f = |x: u8| (x as f32 * k).round() as u8;
    Color32::from_rgba_premultiplied(f(c.r()), f(c.g()), f(c.b()), c.a())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_go_forward_and_wrap() {
        assert_eq!(Drum::Digit.steps("3", "4"), ["4"]);
        assert_eq!(Drum::Digit.steps("9", "0"), ["0"]);
        assert_eq!(Drum::Tens6.steps("5", "0"), ["0"]);
        assert_eq!(Drum::Alpha.steps("B", "E"), ["C", "D", "E"]);
        assert!(Drum::Digit.steps("7", "7").is_empty());
    }

    #[test]
    fn a_refresh_turns_the_whole_drum() {
        assert_eq!(Drum::Tens3.turn("1", "1"), ["2", "0", "1"]);
        assert_eq!(Drum::Tens3.turn("1", "2"), ["2", "0", "1", "2"]);
        assert_eq!(Drum::Alpha.turn("A", "A").len(), Drum::Alpha.len());
        // A card that is still blank just comes round to its symbol.
        assert_eq!(Drum::Digit.turn(" ", "1"), ["0", "1"]);
    }

    #[test]
    fn only_start_ups_and_refreshes_are_shows() {
        let mut f = Flap::new(Drum::Digit, 0.0, 0.0, 1.0);
        f.set("1");
        let mut falls = Vec::new();
        let mut t = 0.0;
        for _ in 0..200 {
            falls.extend(f.tick(t, 1.0));
            t += 0.01;
        }
        // Blank to 0, then 1.
        assert_eq!(falls.len(), 2);
        assert!(falls.iter().all(|f| f.show));
        assert!(falls[1].land && !falls[0].land);
        // The time moving on is not a show.
        f.set("2");
        let fall = (0..100).find_map(|_| {
            t += 0.01;
            f.tick(t, 1.0)
        });
        assert!(fall.is_some_and(|f| !f.show && f.land));
        // A refresh is.
        f.turn("2", t, 0.0, 1.0);
        t += 1.0;
        assert!(f.tick(t, 1.0).is_some_and(|f| f.show));
    }

    #[test]
    fn arcs_go_the_way_round_asked() {
        // From 0 to a quarter turn, the short way and the long way.
        assert!((arc_through(0.0, PI / 2.0, PI / 4.0) - PI / 2.0).abs() < 1e-5);
        assert!((arc_through(0.0, PI / 2.0, PI) - (PI / 2.0 - 2.0 * PI)).abs() < 1e-5);
    }

    #[test]
    fn the_sky_goes_round_in_the_order_of_a_day() {
        assert_eq!(Drum::Sky.steps(NIGHT, MORNING), [MORNING]);
        assert_eq!(Drum::Sky.steps(EVENING, NIGHT), [NIGHT]);
        assert_eq!(Drum::Sky.turn(DAY, DAY), [EVENING, NIGHT, MORNING, DAY]);
        let card = Rect::from_min_size(pos2(0.0, 0.0), vec2(30.0, 32.0));
        for s in [MORNING, DAY, EVENING, NIGHT] {
            assert!(picture(card, s, Color32::WHITE).is_some());
        }
        assert!(picture(card, "A", Color32::WHITE).is_none());
    }

    #[test]
    fn the_flip_speed_scales_the_flips() {
        // How long a card takes from A to Z at a speed.
        let took = |speed: f64| {
            let mut f = Flap::new(Drum::Alpha, 0.0, 0.0, 1.0);
            f.set("A");
            let mut t = 0.0;
            while f.busy(t) || t == 0.0 {
                f.tick(t, 1.0);
                t += 0.001;
            }
            let from = t;
            f.set("Z");
            while f.busy(t) {
                f.tick(t, speed);
                t += 0.001;
            }
            t - from
        };
        let normal = took(1.0);
        assert!(
            (took(2.0) / normal - 0.5).abs() < 0.05,
            "{}",
            took(2.0) / normal
        );
        assert!((took(0.5) / normal - 2.0).abs() < 0.1);
    }

    #[test]
    fn a_blank_card_comes_round_from_the_start() {
        assert_eq!(Drum::Digit.steps(" ", "2"), ["0", "1", "2"]);
        assert_eq!(Drum::AmPm.steps(" ", "PM"), ["AM", "PM"]);
    }

    #[test]
    fn symbols_off_the_drum_take_one_flip() {
        assert_eq!(Drum::Alpha.steps("A", "é"), ["é"]);
    }

    #[test]
    fn the_time_drum_is_short() {
        assert_eq!(Drum::Time.steps(" ", "1"), ["0", "1"]);
        assert_eq!(Drum::Time.steps("9", ":"), [":"]);
        assert_eq!(Drum::Time.steps(":", " "), [" "]);
    }

    #[test]
    fn word_flaps_flip_a_phrase_at_a_time() {
        let words = Drum::Words(REMARKS);
        assert_eq!(words.steps("ON TIME", "DELAYED"), ["EXPECTED", "DELAYED"]);
        assert_eq!(words.steps(" ", "ON TIME"), ["ON TIME"]);
        assert_eq!(words.turn("BOARDING", "BOARDING").len(), REMARKS.len());
        assert_eq!(REMARKS[0], " ");
    }

    #[test]
    fn a_delayed_set_waits_its_turn() {
        let mut f = Flap::new(Drum::Alpha, 0.0, 0.0, 1.0);
        f.set_after("A", 0.0, 0.5);
        assert!(f.tick(0.4, 1.0).is_none());
        assert!(f.tick(0.5, 1.0).is_some());
        assert_eq!(f.target(), "A");
    }

    #[test]
    fn a_card_lands_on_its_target() {
        let mut f = Flap::new(Drum::Alpha, 0.0, 0.0, 1.0);
        f.set("C");
        let mut t = 0.0;
        while f.busy(t) || t == 0.0 {
            f.tick(t, 1.0);
            t += 0.01;
            assert!(t < 5.0);
        }
        assert_eq!(f.cur, "C");
    }
}
