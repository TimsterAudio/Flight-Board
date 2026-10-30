//! A board's cards as one mesh. Drawing each card with the painter, as the
//! clock does, clips each half of each letter, and every clip is a draw call
//! of its own: a few thousand a frame on a full board, which a software
//! renderer can't keep up with. Here each card's parts are tessellated once
//! and kept, letters are clipped by cutting their quads, and the whole board
//! goes to the screen in one mesh.

use std::collections::HashMap;
use std::f32::consts::PI;
use std::sync::Arc;

use egui::epaint::{TessellationOptions, Tessellator, Vertex};
use egui::{pos2, vec2, Color32, Context, FontId, Mesh, Painter, Pos2, Rect, Shape};

use crate::flap::{darken, glyph, Flap, Look};

/// What a card's mesh depends on, to find it by.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    symbol: String,
    /// The card's size and its font's, in bits.
    size: [u32; 3],
    look: [[u8; 4]; 5],
    shadow: u32,
}

impl Key {
    fn new(symbol: &str, size: egui::Vec2, font: &FontId, look: &Look, shadow: f32) -> Self {
        let c = |c: Color32| c.to_array();
        Self {
            symbol: symbol.to_string(),
            size: [size.x.to_bits(), size.y.to_bits(), font.size.to_bits()],
            look: [
                c(look.top),
                c(look.bottom),
                c(look.split),
                c(look.hinge),
                c(look.ink),
            ],
            shadow: shadow.to_bits(),
        }
    }
}

/// The parts of a card of one size and look, with its top left at the
/// origin: what doesn't move, and the two halves a flap can be.
struct Parts {
    /// The split between the halves, and the two halves.
    back: Mesh,
    top: Mesh,
    bottom: Mesh,
    pins: Mesh,
    /// The halves' edges.
    top_rect: Rect,
    bottom_rect: Rect,
}

/// Meshes made with a card's top left at the origin.
#[derive(Default)]
pub(crate) struct Cache {
    still: HashMap<Key, Arc<Mesh>>,
    parts: HashMap<Key, Arc<Parts>>,
    glyphs: HashMap<Key, Arc<Mesh>>,
    /// The cards' shadows, for where the cards were and how dark.
    shadows: Option<(Vec<Rect>, f32, Arc<Mesh>)>,
    /// What the meshes were made for: the font atlas's size, which changes
    /// when egui rebuilds it and moves every letter's place in it, and the
    /// screen's pixels per point.
    atlas: [usize; 2],
    ppp: f32,
}

/// Shapes tessellated into one mesh, with no clipping, and without the
/// soft edges egui gives shapes. Cards stand on whole pixels, so their
/// edges are sharp without them, and they treble the triangles, which is
/// what a software renderer is slowest at.
fn tessellate(ctx: &Context, shapes: Vec<Shape>) -> Mesh {
    let options = TessellationOptions {
        feathering: false,
        ..ctx.tessellation_options(|o| *o)
    };
    let atlas = ctx.fonts(|f| f.font_image_size());
    let mut t = Tessellator::new(ctx.pixels_per_point(), options, atlas, Vec::new());
    let mut mesh = Mesh::default();
    for shape in shapes {
        t.tessellate_shape(shape, &mut mesh);
    }
    mesh
}

/// The card's halves, as the flap draws them.
fn halves(rect: Rect) -> (Rect, Rect, u8, f32) {
    let h = rect.height();
    let r = (h * 0.07).round().max(1.0) as u8;
    let gap = (h * 0.025).max(1.0);
    let mid = rect.center().y;
    let top = Rect::from_min_max(rect.min, pos2(rect.max.x, mid - gap / 2.0));
    let bottom = Rect::from_min_max(pos2(rect.min.x, mid + gap / 2.0), rect.max);
    (top, bottom, r, gap)
}

/// A run of cards' soft shadow on the board below it, lit from above: a
/// band under its foot, darkest against the cards and fading down and at
/// either end. Where the cards hide the board there's no shadow to see, so
/// none is drawn, which is most of a blurred one's cost.
fn shadow(r: Rect, k: f32, out: &mut Mesh) {
    let h = r.height();
    let dark = Color32::from_black_alpha((k * 150.0) as u8);
    let (top, foot) = (r.max.y - h * 0.05, r.max.y + h * 0.3);
    let ease = h * 0.15;
    let xs = [
        r.min.x - ease,
        r.min.x + ease,
        r.max.x - ease,
        r.max.x + ease,
    ];
    let base = out.vertices.len() as u32;
    for (i, x) in xs.into_iter().enumerate() {
        let edge = i == 0 || i == 3;
        out.colored_vertex(pos2(x, top), if edge { Color32::TRANSPARENT } else { dark });
        out.colored_vertex(pos2(x, foot), Color32::TRANSPARENT);
    }
    for i in 0..3 {
        let a = base + 2 * i;
        out.add_triangle(a, a + 2, a + 1);
        out.add_triangle(a + 2, a + 3, a + 1);
    }
}

/// Half a card, `colour`, its outer corners cut by `cut`: the top
/// half's top corners, or the bottom half's bottom ones.
fn half(r: Rect, top: bool, cut: f32, colour: Color32) -> Mesh {
    let (outer, inner) = if top {
        (r.min.y, r.max.y)
    } else {
        (r.max.y, r.min.y)
    };
    let toward = if top { cut } else { -cut };
    let points = [
        pos2(r.min.x, inner),
        pos2(r.min.x, outer + toward),
        pos2(r.min.x + cut, outer),
        pos2(r.max.x - cut, outer),
        pos2(r.max.x, outer + toward),
        pos2(r.max.x, inner),
    ];
    let mut m = Mesh::default();
    for p in points {
        m.colored_vertex(p, colour);
    }
    for k in 1..5 {
        m.add_triangle(0, k, k + 1);
    }
    m
}

/// Adds `m` to `out`, each vertex passed through `f`.
fn append_with(out: &mut Mesh, m: &Mesh, f: impl Fn(Vertex) -> Vertex) {
    let base = out.vertices.len() as u32;
    out.vertices.extend(m.vertices.iter().map(|v| f(*v)));
    out.indices.extend(m.indices.iter().map(|i| base + i));
}

/// Adds the letter quads of `m` to `out`, each vertex passed through `f`
/// and then cut to `clip`. Whether `m` was quads, as letters are; if it
/// wasn't, nothing is added.
fn append_clipped(out: &mut Mesh, m: &Mesh, clip: Rect, f: impl Fn(Vertex) -> Vertex) -> bool {
    let quads = m.vertices.len() / 4;
    if !m.vertices.len().is_multiple_of(4) || m.indices.len() != quads * 6 {
        return false;
    }
    for q in m.vertices.chunks_exact(4) {
        let v: Vec<Vertex> = q.iter().map(|v| f(*v)).collect();
        let pos = Rect::from_points(&v.iter().map(|v| v.pos).collect::<Vec<_>>());
        let uv = Rect::from_points(&v.iter().map(|v| v.uv).collect::<Vec<_>>());
        let cut = pos.intersect(clip);
        if cut.width() <= 0.0 || cut.height() <= 0.0 {
            continue;
        }
        let at = |p: Pos2| {
            let x = if pos.width() > 0.0 {
                (p.x - pos.min.x) / pos.width()
            } else {
                0.0
            };
            let y = if pos.height() > 0.0 {
                (p.y - pos.min.y) / pos.height()
            } else {
                0.0
            };
            pos2(uv.min.x + x * uv.width(), uv.min.y + y * uv.height())
        };
        out.add_rect_with_uv(
            cut,
            Rect::from_min_max(at(cut.min), at(cut.max)),
            v[0].color,
        );
    }
    true
}

impl Cache {
    /// Forgets the meshes if the fonts or the screen have changed since,
    /// or if there are too many.
    pub fn check(&mut self, ctx: &Context, ppp: f32) {
        let atlas = ctx.fonts(|f| f.font_image_size());
        let many = self.still.len() + self.glyphs.len() > 8000;
        if atlas != self.atlas || ppp != self.ppp || many {
            self.still.clear();
            self.parts.clear();
            self.glyphs.clear();
            self.shadows = None;
            self.atlas = atlas;
            self.ppp = ppp;
        }
    }

    /// The shadows of cards at `runs`, `shade` dark, kept until the cards
    /// move.
    pub fn shadows(&mut self, runs: &[Rect], shade: f32) -> Arc<Mesh> {
        if let Some((r, k, m)) = &self.shadows {
            if r == runs && *k == shade {
                return m.clone();
            }
        }
        let mut mesh = Mesh::default();
        for r in runs {
            shadow(*r, shade, &mut mesh);
        }
        let mesh = Arc::new(mesh);
        self.shadows = Some((runs.to_vec(), shade, mesh.clone()));
        mesh
    }

    /// A card standing still, `size` big: its halves, its symbol with the
    /// split across it, and its pins.
    pub fn still(
        &mut self,
        ctx: &Context,
        painter: &Painter,
        flap: &Flap,
        size: egui::Vec2,
        font: &FontId,
        look: &Look,
    ) -> Arc<Mesh> {
        let key = Key::new(flap.shown(), size, font, look, 0.0);
        if let Some(m) = self.still.get(&key) {
            return m.clone();
        }
        let parts = self.parts(ctx, size, look);
        let glyph = self.glyph(ctx, painter, flap.shown(), size, font, look.ink);
        let mut mesh = Mesh::default();
        mesh.append_ref(&parts.top);
        mesh.append_ref(&parts.bottom);
        mesh.append_ref(&glyph);
        mesh.append_ref(&parts.back);
        mesh.append_ref(&parts.pins);
        let mesh = Arc::new(mesh);
        self.still.insert(key, mesh.clone());
        mesh
    }

    fn parts(&mut self, ctx: &Context, size: egui::Vec2, look: &Look) -> Arc<Parts> {
        let key = Key::new("", size, &FontId::default(), look, 0.0);
        if let Some(p) = self.parts.get(&key) {
            return p.clone();
        }
        let ppp = ctx.pixels_per_point();
        let rect = Rect::from_min_size(Pos2::ZERO, size);
        let (top, bottom, r, gap) = halves(rect);
        // The rounded corners, cut square: at a card's size the cut
        // reads as the curve.
        let cut = (r as f32 * 0.6).max(1.0 / ppp);
        let mut back = Mesh::default();
        back.add_colored_rect(
            Rect::from_min_max(pos2(rect.min.x, top.max.y), pos2(rect.max.x, bottom.min.y)),
            look.split,
        );
        let pin = vec2((rect.width() * 0.05).max(1.5), gap * 2.6);
        let mid = rect.center().y;
        let mut pins = Mesh::default();
        for x in [rect.min.x + pin.x / 2.0, rect.max.x - pin.x / 2.0] {
            pins.add_colored_rect(Rect::from_center_size(pos2(x, mid), pin), look.hinge);
        }
        let parts = Arc::new(Parts {
            back,
            top: half(top, true, cut, look.top),
            bottom: half(bottom, false, cut, look.bottom),
            pins,
            top_rect: top,
            bottom_rect: bottom,
        });
        self.parts.insert(key, parts.clone());
        parts
    }

    fn glyph(
        &mut self,
        ctx: &Context,
        painter: &Painter,
        symbol: &str,
        size: egui::Vec2,
        font: &FontId,
        ink: Color32,
    ) -> Arc<Mesh> {
        let look = Look {
            top: ink,
            bottom: ink,
            split: ink,
            hinge: ink,
            ink,
        };
        let key = Key::new(symbol, size, font, &look, 0.0);
        if let Some(m) = self.glyphs.get(&key) {
            return m.clone();
        }
        let rect = Rect::from_min_size(Pos2::ZERO, size);
        let mesh = Arc::new(tessellate(
            ctx,
            vec![glyph(painter, rect, symbol, font, ink)],
        ));
        self.glyphs.insert(key, mesh.clone());
        mesh
    }

    /// Adds a card in the middle of a flip at `rect` to `out`, as
    /// [`Flap::draw`] draws it. Whether it could: a symbol that isn't
    /// letters, such as the sky's pictures, can't be cut this way, and must
    /// be drawn by the flap itself.
    #[allow(clippy::too_many_arguments)]
    pub fn flipping(
        &mut self,
        ctx: &Context,
        painter: &Painter,
        flap: &Flap,
        rect: Rect,
        font: &FontId,
        look: &Look,
        now: f64,
        out: &mut Mesh,
    ) -> bool {
        let p = flap.progress_at(now);
        let size = rect.size();
        let parts = self.parts(ctx, size, look);
        let cur = self.glyph(ctx, painter, flap.shown(), size, font, look.ink);
        let prev = self.glyph(ctx, painter, flap.previous(), size, font, look.ink);
        let at = rect.min.to_vec2();
        let moved = |v: Vertex| Vertex {
            pos: v.pos + at,
            ..v
        };
        // Try the letters first, so a card that can't be cut adds nothing.
        let mut letters = Mesh::default();
        let top = parts.top_rect.translate(at);
        let bottom = parts.bottom_rect.translate(at);
        // The new symbol's top is showing already; the old one's bottom
        // stays until the flap lands on it.
        if !append_clipped(&mut letters, &cur, top, moved)
            || !append_clipped(&mut letters, &prev, bottom, moved)
        {
            return false;
        }
        let mut flap_mesh = Mesh::default();
        let mut flap_letters = Mesh::default();
        let mut shade_bottom = None;
        if p < 0.5 {
            // The old top folds down towards the hinge, darkening as it
            // turns away from the light, and shades the half below it.
            let s = (PI * p).cos();
            let hinge = top.max.y;
            let shade = 0.45 + 0.55 * s;
            let squash = |v: Vertex| {
                let v = moved(v);
                Vertex {
                    pos: pos2(v.pos.x, hinge + (v.pos.y - hinge) * s),
                    color: darken(v.color, shade),
                    ..v
                }
            };
            append_with(&mut flap_mesh, &parts.top, squash);
            let fold = Rect::from_min_max(pos2(top.min.x, hinge - top.height() * s), top.max);
            if !append_clipped(&mut flap_letters, &prev, fold, squash) {
                return false;
            }
            shade_bottom = Some((70.0 * (1.0 - s)) as u8);
        } else {
            // The new bottom swings down from the hinge.
            let s = -(PI * p).cos();
            let hinge = bottom.min.y;
            let shade = 0.6 + 0.4 * s;
            let squash = |v: Vertex| {
                let v = moved(v);
                Vertex {
                    pos: pos2(v.pos.x, hinge + (v.pos.y - hinge) * s),
                    color: darken(v.color, shade),
                    ..v
                }
            };
            append_with(&mut flap_mesh, &parts.bottom, squash);
            let swing =
                Rect::from_min_max(bottom.min, pos2(bottom.max.x, hinge + bottom.height() * s));
            if !append_clipped(&mut flap_letters, &cur, swing, squash) {
                return false;
            }
        }
        append_with(out, &parts.back, moved);
        append_with(out, &parts.top, moved);
        append_with(out, &parts.bottom, moved);
        out.append(letters);
        if let Some(a) = shade_bottom {
            append_with(out, &parts.bottom, |v| Vertex {
                color: Color32::from_black_alpha(
                    (a as f32 * v.color.a() as f32 / 255.0).round() as u8
                ),
                ..moved(v)
            });
        }
        out.append(flap_mesh);
        out.append(flap_letters);
        append_with(out, &parts.pins, moved);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(pos: Rect, uv: Rect) -> Mesh {
        let mut m = Mesh::default();
        m.add_rect_with_uv(pos, uv, Color32::WHITE);
        m
    }

    #[test]
    fn a_cut_letter_keeps_its_place_in_the_atlas() {
        let m = quad(
            Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 20.0)),
            Rect::from_min_max(pos2(0.5, 0.0), pos2(0.6, 0.2)),
        );
        let mut out = Mesh::default();
        let clip = Rect::from_min_max(pos2(-5.0, 0.0), pos2(50.0, 10.0));
        assert!(append_clipped(&mut out, &m, clip, |v| v));
        let b = Rect::from_points(&out.vertices.iter().map(|v| v.pos).collect::<Vec<_>>());
        let uv = Rect::from_points(&out.vertices.iter().map(|v| v.uv).collect::<Vec<_>>());
        assert_eq!(b, Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 10.0)));
        assert!((uv.max.y - 0.1).abs() < 1e-6 && (uv.min.x - 0.5).abs() < 1e-6);
        // Cut away entirely, it adds nothing.
        let mut none = Mesh::default();
        let away = Rect::from_min_max(pos2(0.0, 30.0), pos2(10.0, 40.0));
        assert!(append_clipped(&mut none, &m, away, |v| v));
        assert!(none.vertices.is_empty());
    }

    #[test]
    fn what_is_not_quads_is_not_cut() {
        let mut m = Mesh::default();
        m.colored_vertex(pos2(0.0, 0.0), Color32::WHITE);
        m.colored_vertex(pos2(1.0, 0.0), Color32::WHITE);
        m.colored_vertex(pos2(0.0, 1.0), Color32::WHITE);
        m.add_triangle(0, 1, 2);
        assert!(!append_clipped(
            &mut Mesh::default(),
            &m,
            Rect::EVERYTHING,
            |v| v
        ));
    }
}
