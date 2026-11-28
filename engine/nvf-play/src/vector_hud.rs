//! **The HUD's decorations in vector** [design: owner 2026-11-27, "would it be possible to
//! recreate the HUD elements/decoration so they look the same but are in vector and we can
//! apply the same dark glow to them as we do to text?"]: while the Menu Font row says Sharp
//! (`crate::vector_font::sharp`), the HUD's brackets, its bars' ticks, the condition bar's
//! arrows, the compass strip's rail and actor ticks, the enemy bracket, the experience
//! meter's bracket and pointer and the radiation meter's bracket and pointer are drawn from
//! the shapes here instead of the game's pictures, which are 1 texel to 2 interface units
//! (the corner brackets' atlas pieces) or 1 to 1, stretched over two pixels and more at 4K
//! [observed: the owner's screenshots; `interfaceshared0.dds` is 1024 texels square in
//! `Fallout - Textures2.bsa`, its bracket pieces 256 x 128 drawn over 512 x 256 units].
//! Original keeps the pictures, exactly as before.
//!
//! - **The shapes** are lines, ticks and triangles at the places the pictures show them
//!   [observed: each texture read with `fnvtool bsa extract`, 2026-11-27], drawn as the
//!   owner's SVG workshop drew them on the pixel grid of a 4K view with the HUD at 50%
//!   ([`GRID`], 1.125 pixels a unit) and approved [design: owner 2026-11-28, "perfect,
//!   implement all into game build"]. Every line is [`LINE`] thick, as Quick Loot's frame,
//!   and ends sharply; doubled lines stand a line's width apart; the corner pieces (HP, AP,
//!   XP, radiation, the corner message) are one width and height; every triangle has a right
//!   angle at its tip (a half arrow is its 45-degree half); what is the mirror of something
//!   else is drawn as its pixels turned over, so the two are alike to the pixel [design:
//!   owner's tests and reviews of 2026-11-27, "standardize all elements - same dark glow
//!   behind all elements and text, same line thickness", "make sure all spacing is equal
//!   and symmetrical"].
//! - Beyond the HUD's own pieces: the crosshairs (both styles, the boxed one), the prompt's
//!   divider (`crate::reticle`), the compass's actor ticks and its quest, place, player and
//!   door marks (`crate::compass`), the condition bars with their arrows cut out
//!   ([`SharpOnly`], [`SharpFill`]), the quest messages' objective boxes, the grenade
//!   indicator and the breath meter's ticks (`crate::hud_breath`, which draws its word in
//!   the sharp font). The Vault Boy pictures and the item icons stay the game's.
//! - **Drawn at the screen's own pixels**: each shape is drawn on the CPU into an image as
//!   many pixels across as its box covers on the screen ([`hud_scale`]), sampled texel for
//!   pixel, its design pixels laid on the screen's by one rule ([`Pen`]): an edge at
//!   round(u·s), a line whole(3·s) from its near edge, repeated marks a whole step apart, so
//!   at 4K 50% the pixels are the workshop's and at other sizes equal gaps stay equal. A
//!   rectangle's far edge is the pixel every other part ending there takes too, so corners
//!   meet ([`Snap`]); slanted edges are smoothed (16 samples a pixel, none on a 45-degree
//!   line, so mirrored slants cover alike).
//! - **The glow** is the sharp letters' (`crate::interface::GLOW_RINGS`, `DROP`), at the
//!   HUD font's em ([`GLOW_EM`]): two rings of eight dark copies of the shape, the outer
//!   fainter, and the drop shadow down and right, baked into the same image in black around
//!   the shape's white. The picture is drawn as the game's was, tinted by its node's colour
//!   (`crate::interface::amber`, the enemy's red): the white takes the colour, the black
//!   stays black, and the node's alpha fades both (the experience meter's fade, the enemy
//!   ticks' blink), as a faded line's glow fades with its words.
//! - [`Decor`] on a picture's node swaps the picture for the shape while Sharp and back
//!   (`follow_decor`, every frame that the switch or the view's size changed); the bars'
//!   ticks (`crate::hud`'s `pips`) and the compass strip (`crate::hud`, [`compass_rail`])
//!   are set out again themselves.

use crate::interface::{DROP, DROP_ALPHA, GLOW_RINGS};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use std::collections::HashMap;

/// The em the glow is measured in, in interface units: the HUD font's (font 7's) sharp em,
/// so a line of the HUD has the glow of the HUD's words [design].
pub const GLOW_EM: f32 = crate::vector_font::ENTRY_EM;

/// How far the glow reaches past a shape, in interface units: the outer ring and the drop
/// shadow's offset, and a unit for the smoothing [design].
pub fn glow_reach() -> f32 {
    let ring = GLOW_RINGS.iter().map(|r| r.0).fold(0.0, f32::max);
    (ring.max(DROP.x).max(DROP.y)) * GLOW_EM + 1.0
}

/// A rectangle's side this thin or thinner (interface units) is a line's thickness: a
/// whole number of pixels from its near edge ([`Snap`]) [design].
const THIN: f32 = 8.0;
/// Samples a pixel takes across (and down) for a slanted edge [design].
const SAMPLES: usize = 4;
/// Where in a pixel they stand, across and down: each set symmetric about the pixel's
/// middle, and the two apart, so no sample lies on a 45-degree line through a pixel's corner
/// or middle. A shape and its mirror (either way) then cover their pixels alike, and such a
/// diagonal covers exactly half its pixel [design: a sample on an edge counted for one side
/// only made mirrored diagonals 10/16 against 6/16].
const SAMPLE_X: [f32; SAMPLES] = [2.0 / 16.0, 6.0 / 16.0, 10.0 / 16.0, 14.0 / 16.0];
const SAMPLE_Y: [f32; SAMPLES] = [3.0 / 16.0, 7.0 / 16.0, 9.0 / 16.0, 13.0 / 16.0];

/// A thickness in whole pixels: `units` at `scale`, at least one; a hair over, so a length
/// that is a whole half in units (2 units at 2.25, 4.5 pixels) does not round down by the
/// float's error in `b - a` [design].
fn whole(units: f32, scale: f32) -> f32 {
    (units * scale + 1e-3).round().max(1.0)
}

/// A part's strength along one axis: `from.1` at `from.0` and before, `to.1` at `to.0` and
/// after, linearly between (a line faded out at its end).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fade {
    pub along_x: bool,
    pub from: (f32, f32),
    pub to: (f32, f32),
}

impl Fade {
    pub fn x(from: f32, a: f32, to: f32, b: f32) -> Fade {
        Fade { along_x: true, from: (from, a), to: (to, b) }
    }

    pub fn y(from: f32, a: f32, to: f32, b: f32) -> Fade {
        Fade { along_x: false, from: (from, a), to: (to, b) }
    }

    /// The strength at `p`.
    pub fn at(&self, p: Vec2) -> f32 {
        let v = if self.along_x { p.x } else { p.y };
        let span = self.to.0 - self.from.0;
        let t = if span.abs() < 1e-6 { f32::from(u8::from(v >= self.to.0)) } else { ((v - self.from.0) / span).clamp(0.0, 1.0) };
        self.from.1 + (self.to.1 - self.from.1) * t
    }

    fn scaled(self, k: Vec2) -> Fade {
        let s = if self.along_x { k.x } else { k.y };
        Fade { from: (self.from.0 * s, self.from.1), to: (self.to.0 * s, self.to.1), ..self }
    }
}

/// What a part covers: an upright rectangle (snapped to pixels) or a polygon (smoothed).
#[derive(Clone, Debug, PartialEq)]
pub enum Form {
    Rect(Rect),
    Poly(Vec<Vec2>),
}

/// One part of a shape and how it fades; a cut part takes its form out of the parts
/// drawn (the condition bar's arrows, a hollow mark's inside).
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub form: Form,
    pub fades: Vec<Fade>,
    pub cut: bool,
    /// A polygon whose corners are not set on whole pixels: its angles exactly as drawn (the
    /// crosshair's right-angled arrows, a slant) ([`polyline`]).
    pub exact: bool,
    /// A polygon of three points that is a rounded corner ([`corner_arc`]).
    pub ring: bool,
}

impl Part {
    pub fn fade(mut self, f: Fade) -> Part {
        self.fades.push(f);
        self
    }

    /// This part taken out of the shape.
    pub fn cut(mut self) -> Part {
        self.cut = true;
        self
    }

    /// This polygon's corners (a rounded corner's middle) as given, not moved onto whole
    /// pixels ([`Part::exact`]).
    pub fn as_drawn(mut self) -> Part {
        self.exact = true;
        self
    }

    fn strength(&self, p: Vec2) -> f32 {
        self.fades.iter().map(|f| f.at(p)).product()
    }
}

/// A rectangle from `x0, y0` to `x1, y1`.
pub fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Part {
    Part { form: Form::Rect(Rect::new(x0, y0, x1, y1)), fades: Vec::new(), cut: false, exact: false, ring: false }
}

/// A polygon through `points`.
pub fn poly(points: &[(f32, f32)]) -> Part {
    Part { form: Form::Poly(points.iter().map(|&(x, y)| Vec2::new(x, y)).collect()), fades: Vec::new(), cut: false, exact: false, ring: false }
}

/// A line `width` thick along `points` (open), its corners mitred and its ends square at the
/// points, as a polygon.
pub fn polyline(points: &[(f32, f32)], width: f32) -> Part {
    let v: Vec<Vec2> = points.iter().map(|&(x, y)| Vec2::new(x, y)).collect();
    let normal = |a: Vec2, b: Vec2| (b - a).normalize_or_zero().perp();
    let half = width / 2.0;
    let offset = |i: usize| -> Vec2 {
        let n = match i {
            0 => normal(v[0], v[1]),
            _ if i == v.len() - 1 => normal(v[i - 1], v[i]),
            _ => {
                let (n1, n2) = (normal(v[i - 1], v[i]), normal(v[i], v[i + 1]));
                let m = (n1 + n2).normalize_or_zero();
                return m * half / m.dot(n1).max(0.25);
            }
        };
        n * half
    };
    let mut out: Vec<Vec2> = (0..v.len()).map(|i| v[i] + offset(i)).collect();
    out.extend((0..v.len()).rev().map(|i| v[i] - offset(i)));
    Part { form: Form::Poly(out), fades: Vec::new(), cut: false, exact: true, ring: false }
}

/// A quarter circle [`LINE`] thick round `center`, where a line across and an upright meet,
/// from `across` (on the line's middle, the line ending there) to `down` (on the upright's
/// middle, the upright starting there): the game's corners, whose arc stands round the
/// corner's point and meets each line square on [observed: the corner message's bracket and
/// the prompt's divider, `interfaceshared0.dds`]. `center` is where the two lines' middles
/// meet; it stands on the lines' pixels as they are drawn ([`Snap`]). Kept as a polygon of
/// the three points ([`Part::ring`]), so moving the part moves it.
pub fn corner_arc(center: (f32, f32), across: (f32, f32), down: (f32, f32)) -> Part {
    let v = |p: (f32, f32)| Vec2::new(p.0, p.1);
    Part { form: Form::Poly(vec![v(center), v(across), v(down)]), fades: Vec::new(), cut: false, exact: false, ring: true }
}

/// A decoration: its box (the picture's node, interface units) and its parts in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub size: Vec2,
    pub parts: Vec<Part>,
    /// With the dark glow (not for dark shapes, `crate::interface::dark_ink`).
    pub glow: bool,
    /// Its left and right ends meet (the compass strip, a turn round).
    pub wrap: bool,
    /// Drawn, then turned over left to right within its first `flip_x` units (top to bottom
    /// within `flip_y`): a shape the mirror of another, on the same pixels turned over, so
    /// the two are alike to the pixel (the corner brackets, the crosshair's halves).
    pub flip_x: Option<f32>,
    pub flip_y: Option<f32>,
    /// Its left of `mirror_x` (units) drawn, and copied turned over onto its right: a shape
    /// alike to the pixel either side of its middle (the prompt's stems, the boxed
    /// crosshair).
    pub mirror_x: Option<f32>,
}

impl Shape {
    /// `parts` in a box of `size` units, with the glow.
    pub fn new(size: Vec2, parts: Vec<Part>) -> Shape {
        Shape { size, parts, glow: true, wrap: false, flip_x: None, flip_y: None, mirror_x: None }
    }

    /// `parts` measured in a picture's texels (`texels` across and down), set in a box of
    /// `size` units over which the picture was stretched.
    pub fn from_texels(texels: Vec2, size: Vec2, parts: Vec<Part>) -> Shape {
        let k = size / texels;
        let parts = parts
            .into_iter()
            .map(|p| Part {
                form: match p.form {
                    Form::Rect(r) => Form::Rect(Rect::from_corners(r.min * k, r.max * k)),
                    Form::Poly(v) => Form::Poly(v.into_iter().map(|q| q * k).collect()),
                },
                fades: p.fades.into_iter().map(|f| f.scaled(k)).collect(),
                ..p
            })
            .collect();
        Shape::new(size, parts)
    }
}

// ---- drawing ---------------------------------------------------------------------------

/// Where a shape's lengths fall on the pixels at `scale` pixels a unit: each length to the
/// nearest pixel, but a thin side's far edge (a line's) its near edge's pixel and its
/// thickness in whole pixels, at least one; and every other length that ends where a thin
/// side's far edge is takes that pixel, so corners meet [design: `crate::interface::line`,
/// `crate::message_menu::box_lines`].
pub struct Snap {
    scale: f32,
    /// (unit, pixel) of the thin sides' far edges, across and down.
    far: [Vec<(f32, f32)>; 2],
}

impl Snap {
    pub fn new(shape: &Shape, scale: f32) -> Snap {
        let mut far: [Vec<(f32, f32)>; 2] = [Vec::new(), Vec::new()];
        for p in &shape.parts {
            if let Form::Rect(r) = &p.form {
                for (axis, (a, b)) in [(0, (r.min.x, r.max.x)), (1, (r.min.y, r.max.y))] {
                    if b - a <= THIN {
                        let px = (a * scale).round() + whole(b - a, scale);
                        if !far[axis].iter().any(|f| (f.0 - b).abs() < 1e-3) {
                            far[axis].push((b, px));
                        }
                    }
                }
            }
        }
        Snap { scale, far }
    }

    /// The pixel edge for `v` units on `axis` (0 across, 1 down).
    pub fn edge(&self, axis: usize, v: f32) -> f32 {
        self.far[axis].iter().find(|f| (f.0 - v).abs() < 1e-3).map_or((v * self.scale).round(), |f| f.1)
    }

    /// A rectangle's pixel edges: a thin side from its near edge.
    fn rect(&self, r: Rect) -> (f32, f32, f32, f32) {
        let side = |axis: usize, a: f32, b: f32| {
            // A near edge on another line's far edge takes its pixel too.
            let p0 = self.edge(axis, a);
            if b - a <= THIN {
                (p0, p0 + whole(b - a, self.scale))
            } else {
                (p0, self.edge(axis, b).max(p0 + 1.0))
            }
        };
        let (x0, x1) = side(0, r.min.x, r.max.x);
        let (y0, y1) = side(1, r.min.y, r.max.y);
        (x0, y0, x1, y1)
    }

    /// A polygon's corner in pixels: on a thin side's far edge where it lies on one.
    fn point(&self, p: Vec2) -> Vec2 {
        Vec2::new(self.edge(0, p.x), self.edge(1, p.y))
    }
}

/// A shape's strength at each pixel (0 to 1), `width` pixels a row.
#[derive(Clone, Debug, PartialEq)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub alpha: Vec<f32>,
}

impl Mask {
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.alpha[y * self.width + x]
    }

    /// The strength at `(x, y)` pixels, between pixel centres linearly; nothing outside, or
    /// across the left and right ends when `wrap`.
    fn sample(&self, x: f32, y: f32, wrap: bool) -> f32 {
        let (fx, fy) = (x - 0.5, y - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let get = |x: f32, y: f32| -> f32 {
            if y < 0.0 || y >= self.height as f32 {
                return 0.0;
            }
            let x = if wrap {
                x.rem_euclid(self.width as f32)
            } else if x < 0.0 || x >= self.width as f32 {
                return 0.0;
            } else {
                x
            };
            self.at(x as usize, y as usize)
        };
        let top = get(x0, y0) * (1.0 - tx) + get(x0 + 1.0, y0) * tx;
        let bottom = get(x0, y0 + 1.0) * (1.0 - tx) + get(x0 + 1.0, y0 + 1.0) * tx;
        top * (1.0 - ty) + bottom * ty
    }
}

/// The image size of `shape` at `scale` pixels a unit.
pub fn pixels(shape: &Shape, scale: f32) -> (usize, usize) {
    ((shape.size.x * scale).round().max(1.0) as usize, (shape.size.y * scale).round().max(1.0) as usize)
}

/// Whether `p` is inside the polygon `v` (even-odd).
fn inside(v: &[Vec2], p: Vec2) -> bool {
    let mut odd = false;
    let mut j = v.len().wrapping_sub(1);
    for i in 0..v.len() {
        let (a, b) = (v[i], v[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// Draw `shape`'s parts at `scale` pixels a unit: the strongest part at each pixel.
pub fn ink(shape: &Shape, scale: f32) -> Mask {
    let (w, h) = pixels(shape, scale);
    let snap = Snap::new(shape, scale);
    let (drawn, cuts): (Vec<&Part>, Vec<&Part>) = shape.parts.iter().partition(|p| !p.cut);
    let mut alpha = cover(&drawn, &snap, w, h, scale);
    if !cuts.is_empty() {
        let out = cover(&cuts, &snap, w, h, scale);
        for (a, c) in alpha.iter_mut().zip(out) {
            *a *= 1.0 - c;
        }
    }
    // Turned over within its first `flip` units, or copied onto its right half.
    let rows = |alpha: &mut Vec<f32>, f: &dyn Fn(&mut [f32])| alpha.chunks_mut(w).for_each(f);
    if let Some(fx) = shape.flip_x {
        let n = ((fx * scale).round() as usize).min(w);
        rows(&mut alpha, &|row| row[..n].reverse());
    }
    if let Some(fy) = shape.flip_y {
        let n = ((fy * scale).round() as usize).min(h);
        for y in 0..n / 2 {
            for x in 0..w {
                alpha.swap(y * w + x, (n - 1 - y) * w + x);
            }
        }
    }
    if let Some(axis) = shape.mirror_x {
        // Column `x` and column `k - x` are each other's mirror.
        let k = (2.0 * axis * scale).round() as i64 - 1;
        rows(&mut alpha, &|row| {
            for x in 0..row.len() as i64 {
                if 2 * x > k && k - x >= 0 {
                    row[x as usize] = row[(k - x) as usize];
                }
            }
        });
    }
    Mask { width: w, height: h, alpha }
}

/// How much of each pixel `parts` cover (the strongest at each).
fn cover(parts: &[&Part], snap: &Snap, w: usize, h: usize, scale: f32) -> Vec<f32> {
    let mut alpha = vec![0.0f32; w * h];
    let unit = |px: f32| px / scale;
    for part in parts {
        match &part.form {
            Form::Rect(r) => {
                let (x0, y0, x1, y1) = snap.rect(*r);
                for y in (y0.max(0.0) as usize)..(y1.min(h as f32).max(0.0) as usize) {
                    for x in (x0.max(0.0) as usize)..(x1.min(w as f32).max(0.0) as usize) {
                        let a = part.strength(Vec2::new(unit(x as f32 + 0.5), unit(y as f32 + 0.5)));
                        let cell = &mut alpha[y * w + x];
                        *cell = cell.max(a);
                    }
                }
            }
            Form::Poly(points) if part.ring && points.len() == 3 => {
                let (center, across, down) = (points[0], points[1], points[2]);
                // The middle of the lines as drawn: a line's near edge's pixel and half its
                // whole-pixel thickness.
                let t = whole(LINE, scale);
                let c = if part.exact { center * scale } else { Vec2::new(snap.edge(0, center.x - LINE / 2.0), snap.edge(1, center.y - LINE / 2.0)) + t / 2.0 };
                let sign = Vec2::new((across.x - center.x).signum(), (down.y - center.y).signum());
                let r = (across.x - center.x).abs() * scale;
                let (lo, hi) = (c - (r + t), c + (r + t));
                for y in (lo.y.max(0.0) as usize)..(hi.y.min(h as f32).max(0.0) as usize) {
                    for x in (lo.x.max(0.0) as usize)..(hi.x.min(w as f32).max(0.0) as usize) {
                        let mut hits = 0;
                        for sy in SAMPLE_Y {
                            for sx in SAMPLE_X {
                                let p = Vec2::new(x as f32 + sx, y as f32 + sy);
                                let d = (p - c) * sign;
                                hits += usize::from(d.x >= 0.0 && d.y >= 0.0 && (d.length() - r).abs() <= t / 2.0);
                            }
                        }
                        if hits > 0 {
                            let cell = &mut alpha[y * w + x];
                            *cell = cell.max(hits as f32 / (SAMPLES * SAMPLES) as f32);
                        }
                    }
                }
            }
            Form::Poly(points) => {
                let v: Vec<Vec2> = points.iter().map(|&p| if part.exact { p * scale } else { snap.point(p) }).collect();
                let (lo, hi) = v.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
                let (x0, x1) = (lo.x.floor().max(0.0) as usize, (hi.x.ceil().max(0.0) as usize).min(w));
                let (y0, y1) = (lo.y.floor().max(0.0) as usize, (hi.y.ceil().max(0.0) as usize).min(h));
                for y in y0..y1 {
                    for x in x0..x1 {
                        let mut hits = 0;
                        for sy in SAMPLE_Y {
                            for sx in SAMPLE_X {
                                let p = Vec2::new(x as f32 + sx, y as f32 + sy);
                                hits += usize::from(inside(&v, p));
                            }
                        }
                        if hits > 0 {
                            let cover = hits as f32 / (SAMPLES * SAMPLES) as f32;
                            let a = cover * part.strength(Vec2::new(unit(x as f32 + 0.5), unit(y as f32 + 0.5)));
                            let cell = &mut alpha[y * w + x];
                            *cell = cell.max(a);
                        }
                    }
                }
            }
        }
    }
    alpha
}

/// The dark glow round `ink` at `scale` pixels a unit (each pixel's darkness, 0 to 1): the
/// sharp letters' rings of dark copies, each copy off one of eight ways, and the drop shadow
/// down and right (`crate::interface::GLOW_RINGS`, `DROP`, at [`GLOW_EM`]).
pub fn glow(ink: &Mask, scale: f32, wrap: bool) -> Vec<f32> {
    let em = GLOW_EM * scale;
    let mut copies: Vec<(Vec2, f32)> = Vec::new();
    for (r, alpha) in GLOW_RINGS {
        let (r, d) = (r * em, r * em * std::f32::consts::FRAC_1_SQRT_2);
        for off in [Vec2::new(r, 0.0), Vec2::new(-r, 0.0), Vec2::new(0.0, r), Vec2::new(0.0, -r), Vec2::new(d, d), Vec2::new(-d, d), Vec2::new(d, -d), Vec2::new(-d, -d)] {
            copies.push((off, alpha));
        }
    }
    copies.push((DROP * em, DROP_ALPHA));
    let reach = copies.iter().map(|c| c.0.abs().max_element()).fold(0.0, f32::max).ceil() as usize + 1;
    // Only near the ink: the rows and columns within the glow's reach of a lit pixel.
    let lit_rows: Vec<bool> = (0..ink.height).map(|y| (0..ink.width).any(|x| ink.at(x, y) > 0.0)).collect();
    let near = |y: usize| (y.saturating_sub(reach)..(y + reach + 1).min(ink.height)).any(|r| lit_rows[r]);
    let mut out = vec![0.0f32; ink.width * ink.height];
    for y in 0..ink.height {
        if !near(y) {
            continue;
        }
        for x in 0..ink.width {
            let c = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let clear: f32 = copies.iter().map(|(off, a)| 1.0 - a * ink.sample(c.x - off.x, c.y - off.y, wrap)).product();
            out[y * ink.width + x] = 1.0 - clear;
        }
    }
    out
}

/// A linear value (0 to 1) as an sRGB byte.
fn srgb_byte(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

/// `shape` drawn at `scale` pixels a unit as sRGB RGBA bytes, straight alpha: the shape in
/// white over its glow in black (without the glow, the shape alone), so a node's colour
/// tints the shape and leaves the glow black.
pub fn draw(shape: &Shape, scale: f32) -> (usize, usize, Vec<u8>) {
    let ink = ink(shape, scale);
    let dark = if shape.glow { glow(&ink, scale, shape.wrap) } else { vec![0.0; ink.alpha.len()] };
    let mut rgba = Vec::with_capacity(ink.alpha.len() * 4);
    for (a, g) in ink.alpha.iter().zip(&dark) {
        // The shape over its glow: a + g (1 - a); white the shape's part of that.
        let total = a + g * (1.0 - a);
        let white = if total > 0.0 { a / total } else { 0.0 };
        let v = srgb_byte(white);
        rgba.extend_from_slice(&[v, v, v, (total.clamp(0.0, 1.0) * 255.0).round() as u8]);
    }
    (ink.width, ink.height, rgba)
}

/// `shape` drawn at `scale` as an image for a node, sampled texel for pixel.
pub fn image(shape: &Shape, scale: f32) -> Image {
    let (w, h, rgba) = draw(shape, scale);
    let size = Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 };
    let mut image = Image::new(size, TextureDimension::D2, rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.sampler = ImageSampler::nearest();
    image
}

/// The HUD's pixels a unit for a view `physical_height` pixels high, at the HUD scale
/// (`crate::interface::ui_scale`).
pub fn hud_scale(physical_height: f32) -> f32 {
    (physical_height * crate::interface::ui_scale() / 960.0).max(0.1)
}

// ---- the decorations [design: the owner's SVG workshop, approved 2026-11-28] ---------------

/// Every line's thickness, as Quick Loot's frame (`crate::quick_loot`'s `FRAME`, drawn with
/// `crate::interface::line`): a whole number of pixels [design: owner 2026-11-27, "make them
/// the same thickness as the loot menu is", "standardize all elements"].
pub const LINE: f32 = 3.0;
/// Room round the condition bar's picture for its glow, in units.
pub const CONDITION_MARGIN: f32 = 6.0;

/// The design grid: the owner's 4K view with the HUD at 50%, 1.125 pixels a unit. The
/// workshop drew every decoration in those pixels, so that every edge, gap and step is a
/// whole pixel there and equal gaps are equal to the pixel [design: the owner's review of
/// 2026-11-27, "make sure all spacing is equal and symmetrical"]; the shapes here are its
/// numbers ([`Pen`]), approved as a whole [design: owner 2026-11-28, "perfect, implement all
/// into game build"].
pub const GRID: f32 = 1.125;

/// The corner pieces' width from the upright's outer side to the line's end, and their
/// uprights' height, in design pixels: the HP decoration's, for HP, AP, XP, radiation and the
/// corner message alike [design: the owner's third review].
const CORNER_W: f32 = 388.0;
const CORNER_H: f32 = 123.0;

/// The workshop's pixels laid on a scale's: an edge at `px` design pixels stands at
/// round(px / [`GRID`] · s), a length of `px` is that many whole pixels (at least one), a
/// line is [`LINE`] as whole(LINE · s) from its near edge, a row of repeated marks steps by a
/// whole length, and what is the mirror of something else is drawn as its pixels turned
/// over [design: the workshop's grid rule]. At 4K 50% that gives the workshop's pixels
/// exactly; at other sizes equal gaps stay equal.
#[derive(Clone, Copy, Debug)]
struct Pen {
    s: f32,
    /// A line's thickness, whole pixels.
    t: f32,
}

impl Pen {
    fn new(s: f32) -> Pen {
        Pen { s, t: whole(LINE, s) }
    }

    /// The pixel edge at `px` design pixels.
    fn at(self, px: f32) -> f32 {
        (px / GRID * self.s + 1e-3).round()
    }

    /// `px` design pixels as a whole number of pixels, at least one.
    fn len(self, px: f32) -> f32 {
        (px / GRID * self.s + 1e-3).round().max(1.0)
    }

    /// `px` design pixels unrounded (a curve's radius, a point's place).
    fn exact(self, px: f32) -> f32 {
        px / GRID * self.s
    }
}

/// A shape set out in the pixels of `scale`, in a box of `size` units.
fn in_pixels(size: Vec2, parts: Vec<Part>, scale: f32) -> Shape {
    Shape::new(size * scale, parts).at_scale(scale)
}

/// The pictures drawn as shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Decoration {
    /// `glow_hud_left_seperatorglow` (atlas piece 256 x 128) over 512 x 256.
    LeftBracket,
    /// `glow_hud_right_seperatorglow`, the same: the left one's mirror, with the divider.
    RightBracket,
    /// `glow_enemy_health_seperator.dds` (512 x 64) over 500 x 64.
    EnemyBracket,
    /// The condition bar's ground, [`CONDITION_MARGIN`] in, with the jamming threshold's two
    /// arrows (`repair_meter_pips.dds`) cut out of it, and its fill (the same, without the
    /// glow: it lies on the ground).
    ConditionGround,
    ConditionFill,
    /// `glow_xp_bar_over.dds` (512 x 256) over the same.
    XpBracket,
    /// `glow_xp_bar_pointer.dds` (64 x 64) over the same.
    XpPointer,
    /// `glow_messages_radiation_seperator_right.dds` (512 x 128) over the same.
    RadiationBracket,
    /// `glow_messages_radiation_seperator_arrow.dds` (64 x 32) over 46 x 24.
    RadiationPointer,
    /// The compass's actor tick, `glow_hud_tick_mark` (32 x 32) over the same.
    ActorTick,
    /// The compass's marks (64 x 64 over 38 x 38, `crate::compass`'s `LANDMARK_SIZE`): the
    /// quest marker, a place not yet found (an open chevron), a place found (a whole
    /// triangle), the player's own marker (a hollow quest marker), a door under the Show
    /// Doors on Compass tweak (the local map's `icon_local_door.dds` over the same).
    QuestMark,
    PlaceMark,
    FoundMark,
    PlayerMark,
    DoorMark,
    /// The crosshair's two halves (`glow_crosshair.dds` 64 x 64, each half 32 x 64), the
    /// right the left's mirror; the boxed one (`glow_crosshair_filled.dds`).
    CrossLeft,
    CrossRight,
    CrossFilled,
    /// The gun cross's four bars (`target_*.dds`, 8 x 32 and 32 x 8).
    GunTop,
    GunBottom,
    GunLeft,
    GunRight,
    /// The prompt's line across with its stem (`glow_hud_bottom_info_seperator_divider`,
    /// 512 x 128).
    PromptDivider,
    /// The corner message's bracket (`glow_messages_seperator_left.dds`, 512 x 256 over
    /// 470 x 192, `crate::corner_messages`): a line across its top and one down its left.
    MessageBracket,
    /// A quest message's objective box, open and done (`glow_square_small.dds`,
    /// `glow_square_filled_small.dds` over 26 x 26, `crate::quest_messages`'s `BOX`).
    QuestBoxOpen,
    QuestBoxDone,
    /// The grenade indicator (`explosive_indicator.dds`, 128 texels at zoom 120, cut to its
    /// box 151 wide, `crate::grenade_indicator`): the top of a ring with a point on it.
    Grenade,
    /// One of the breath meter's sixteen ticks (`Tick01` … `Tick16` of `AirTimer01.NIF`,
    /// `crate::hud_breath`), turned 22.5 degrees each clockwise from the top (`Tick16`), in
    /// the meter's box [`BREATH_MARGIN`] larger all round.
    BreathTick(u8),
}

impl Decoration {
    pub const ALL: [Decoration; 29] = [
        Decoration::LeftBracket,
        Decoration::RightBracket,
        Decoration::EnemyBracket,
        Decoration::ConditionGround,
        Decoration::ConditionFill,
        Decoration::XpBracket,
        Decoration::XpPointer,
        Decoration::RadiationBracket,
        Decoration::RadiationPointer,
        Decoration::ActorTick,
        Decoration::QuestMark,
        Decoration::PlaceMark,
        Decoration::FoundMark,
        Decoration::PlayerMark,
        Decoration::DoorMark,
        Decoration::CrossLeft,
        Decoration::CrossRight,
        Decoration::CrossFilled,
        Decoration::GunTop,
        Decoration::GunBottom,
        Decoration::GunLeft,
        Decoration::GunRight,
        Decoration::PromptDivider,
        Decoration::MessageBracket,
        Decoration::QuestBoxOpen,
        Decoration::QuestBoxDone,
        Decoration::Grenade,
        Decoration::BreathTick(16),
        Decoration::BreathTick(3),
    ];

    /// The shape, in its picture's box, as drawn at 960 lines and 100% (a pixel a unit).
    pub fn shape(self) -> Shape {
        self.shape_at(1.0)
    }

    /// The shape as drawn at `scale` pixels a unit: the workshop's design pixels laid on that
    /// scale's ([`Pen`]); the brackets' caps and marks stand where the bars' pips at that
    /// scale end ([`pips_in`]).
    pub fn shape_at(self, scale: f32) -> Shape {
        let p = Pen::new(scale);
        let t = p.t;
        let mark = |parts: Vec<Part>| in_pixels(Vec2::splat(MARK_BOX), parts, scale);
        match self {
            Decoration::LeftBracket => corner(false, scale),
            Decoration::RightBracket => corner(true, scale),
            Decoration::EnemyBracket => enemy_bracket(scale),
            Decoration::ConditionGround => condition_bar(true, scale),
            Decoration::ConditionFill => condition_bar(false, scale),
            Decoration::XpBracket => xp_bracket(scale),
            // The HUD's triangle, 24 x 12 design pixels, its tip on the box's middle.
            Decoration::XpPointer => {
                let (mid, y, half) = (p.at(36.0), p.at(33.0), p.len(12.0));
                in_pixels(Vec2::splat(64.0), vec![poly(&[(mid - half, y), (mid + half, y), (mid, y + half)]).as_drawn()], scale)
            }
            Decoration::RadiationBracket => radiation_bracket(scale),
            // The HUD's triangle pointing up, 18 x 9.
            Decoration::RadiationPointer => {
                let (mid, base, half) = (p.at(26.0), p.at(18.0), p.len(9.0));
                in_pixels(Vec2::new(46.0, 24.0), vec![poly(&[(mid - half, base), (mid + half, base), (mid, base - half)]).as_drawn()], scale)
            }
            // A line from under the corner bracket's lower line (the window standing 65 units
            // down the piece) to where the picture's bar ended; the actors' ticks stay above
            // the rail [design: owner 2026-11-27, "the NPC indicator goes into the HP bar and
            // decoration"; "NPC/ENEMY markers should stay at the top"].
            Decoration::ActorTick => {
                let x = p.at(10.0);
                in_pixels(Vec2::splat(32.0), vec![rect(x, p.at(12.0), x + t, p.at(30.0))], scale)
            }
            Decoration::QuestMark => mark(vec![poly(&quest_points(p)).as_drawn()]),
            Decoration::PlaceMark => mark(vec![poly(&place_points(p, true)).as_drawn()]),
            Decoration::FoundMark => mark(vec![poly(&place_points(p, false)).as_drawn()]),
            Decoration::PlayerMark => mark(vec![poly(&quest_points(p)).as_drawn(), poly(&player_inside(p)).as_drawn().cut()]),
            Decoration::DoorMark => mark(door(p)),
            Decoration::CrossLeft => in_pixels(Vec2::new(32.0, 64.0), vec![chevron(p, (32.0 * scale).round(), (64.0 * scale).round() / 2.0)], scale),
            Decoration::CrossRight => Shape { flip_x: Some(32.0), ..Decoration::CrossLeft.shape_at(scale) },
            Decoration::CrossFilled => {
                // The box's left side, its arms ending 5 design pixels short of the middle and
                // clear of the arrow, and the left arrow; the right half the left's mirror
                // [design: owner's test of 2026-11-27, "make sure they do not touch the
                // crosshair"].
                let m = (64.0 * scale).round();
                let (x, top, end) = (p.at(13.0), p.at(20.0), p.at(31.0));
                let parts = vec![rect(x, top, x + t, m - top), rect(x, top, end, top + t), rect(x, m - top - t, end, m - top), chevron(p, m / 2.0, m / 2.0)];
                Shape { mirror_x: Some(32.0), ..in_pixels(Vec2::splat(64.0), parts, scale) }
            }
            // The bar's middle line 2.5 units in, its inner end at the picture's edge toward
            // the middle (`crate::reticle`'s `BAR_MIDDLE`).
            Decoration::GunTop => Shape::new(Vec2::new(8.0, 32.0), vec![rect(1.0, 12.0, 1.0 + LINE, 32.0)]),
            Decoration::GunBottom => Shape { flip_y: Some(32.0), ..Decoration::GunTop.shape() },
            Decoration::GunLeft => Shape::new(Vec2::new(32.0, 8.0), vec![rect(12.0, 1.0, 32.0, 1.0 + LINE)]),
            Decoration::GunRight => Shape { flip_x: Some(32.0), ..Decoration::GunLeft.shape() },
            Decoration::PromptDivider => prompt_divider(scale),
            // The line across and the upright, [`CORNER_W`] wide and [`CORNER_H`] tall, joined
            // by a quarter circle round where their middles meet [observed: the atlas piece;
            // design: the inspection, "your version is missing the quarter circle"; the third
            // review].
            Decoration::MessageBracket => {
                let (x, y, r) = (p.at(34.0), p.at(12.0), p.len(15.0));
                let c = (x + t / 2.0, y + t / 2.0);
                let parts = vec![rect(x + r, y, x + p.len(CORNER_W), y + t), corner_arc(c, (c.0 + r, c.1), (c.0, c.1 + r)).as_drawn(), rect(x, y + r, x + t, y + p.len(CORNER_H))];
                in_pixels(Vec2::new(470.0, 192.0), parts, scale)
            }
            // A 12-pixel square, its outline a line's width, or filled.
            Decoration::QuestBoxOpen | Decoration::QuestBoxDone => {
                let (x, y, side) = (p.at(6.0), p.at(5.0), p.len(12.0));
                let mut parts = vec![rect(x, y, x + side, y + side)];
                if self == Decoration::QuestBoxOpen {
                    parts.push(rect(x + t, y + t, x + side - t, y + side - t).cut());
                }
                in_pixels(Vec2::splat(26.0), parts, scale)
            }
            Decoration::Grenade => grenade(p),
            Decoration::BreathTick(n) => breath_tick(p, n),
        }
    }
}

/// The compass marks' box (`crate::compass`'s `LANDMARK_SIZE`).
const MARK_BOX: f32 = 38.0;

/// The place, quest and player marks' tops in the compass window while Sharp, units: a
/// line's width under the letters' baseline (45 design pixels down the window) [design: the
/// owner's fourth review, "only location/quest markers should be on the bottom"].
pub const MARK_TOP: f32 = (45.0 + 3.0) / GRID;

/// Where a compass mark's shape stands in its box, units: its middle across (on the
/// bearing) and its top (at [`MARK_TOP`]; the actors' ticks keep their own place).
pub fn mark_anchor(kind: Decoration) -> Vec2 {
    match kind {
        Decoration::ActorTick => Vec2::new(11.5, 12.0) / GRID,
        Decoration::DoorMark => Vec2::new(23.0, 9.0) / GRID,
        _ => Vec2::new(13.0, 5.0) / GRID,
    }
}

/// The quest marker: pointed at the top by the HUD's triangle (90 degrees), 16 x 24 design
/// pixels, its foot notched by the same triangle 4 deep; mirror symmetric about its middle
/// (x 13) [design: the workshop's second review].
fn quest_points(p: Pen) -> Vec<(f32, f32)> {
    let (a, top, side, n) = (p.at(13.0), p.at(5.0), p.len(8.0), p.len(4.0));
    let bottom = top + p.len(24.0);
    vec![(a, top), (a + side, top + side), (a + side, bottom), (a + n, bottom), (a, bottom - n), (a - n, bottom), (a - side, bottom), (a - side, top + side)]
}

/// A place's mark: found, the HUD's triangle as wide as the quest marker (its base half a
/// pixel past each side, so its sharp corners light the marker's 16 columns); not found yet,
/// its two sides alone, an open chevron of two strokes a line across [design: the owner's
/// fourth review].
fn place_points(p: Pen, open: bool) -> Vec<(f32, f32)> {
    let (a, base) = (p.at(13.0), p.at(5.0) + p.len(8.0));
    let half = p.len(8.0) + 0.5;
    let tip = base - half;
    if !open {
        return vec![(a, tip), (a + half, base), (a - half, base)];
    }
    let k = p.t * std::f32::consts::SQRT_2;
    vec![(a, tip), (a + half, base), (a + half - k, base), (a, tip + k), (a - half + k, base), (a - half, base)]
}

/// The player's marker's inside: the quest marker with every edge moved a line's width in,
/// parallel (its mitred inset), so its outline is one line all round; its feet, narrower than
/// two lines, close up solid [design: the workshop's second review].
fn player_inside(p: Pen) -> Vec<(f32, f32)> {
    let (a, top, side, n, t) = (p.at(13.0), p.at(5.0), p.len(8.0), p.len(4.0), p.t);
    let bottom = top + p.len(24.0);
    let d = t * std::f32::consts::SQRT_2;
    let x = side - t;
    // Where the notch's inset edge meets the inner upright, or the foot's inset bottom.
    let low = bottom - n + side - t - d;
    let mut right = vec![(a + x, top + side - t + d)];
    if low <= bottom - t {
        right.push((a + x, low));
    } else {
        right.extend([(a + x, bottom - t), (a + n + d - t, bottom - t)]);
    }
    let mut out = vec![(a, top + d)];
    out.extend(right.iter().copied());
    out.push((a, bottom - n - d));
    out.extend(right.iter().rev().map(|&(x, y)| (2.0 * a - x, y)));
    out
}

/// A door: a block 24 design pixels square, its doorway cut out in perspective, the handle a
/// pixel's upright line a pixel from the doorway's edge, and the arrow pointing in cut out,
/// the HUD's triangle (6 deep, 12 tall) with its tip on the block's middle row and a 6-pixel
/// shaft from it out through the block's right edge [design: owner 2026-11-28, "center the
/// arrow and then run a 3px line from the arrow to the edge of the symbol to mimic the
/// vanilla icon", "make the arrow extended line 6px", "make the door handle thinner and make
/// the spacing from the door the same as the doorhandle width"].
fn door(p: Pen) -> Vec<Part> {
    let (x0, x1, y0) = (p.at(11.0), p.at(35.0), p.at(9.0));
    let half = p.len(12.0);
    let (mid, y1) = (y0 + half, y0 + 2.0 * half);
    let (dl, dr) = (p.at(13.0), p.at(23.0));
    let (outer, inner) = (p.len(10.0), p.len(9.0));
    let hw = p.len(1.0);
    let hx = dr - 2.0 * hw;
    // The doorway's top and bottom edges at `x`; the handle stays drawn, so the doorway is
    // cut either side of it.
    let edge = |x: f32| {
        let k = (x - dl) / (dr - dl).max(1.0);
        (mid - outer + (outer - inner) * k, mid + outer - (outer - inner) * k)
    };
    let side = |from: f32, to: f32| {
        let ((a0, a1), (b0, b1)) = (edge(from), edge(to));
        poly(&[(from, a0), (to, b0), (to, b1), (from, a1)]).as_drawn().cut()
    };
    let (tip, arrow, shaft) = (p.at(25.0), p.len(6.0), p.len(3.0));
    vec![
        rect(x0, y0, x1, y1),
        side(dl, hx),
        side(hx + hw, dr),
        poly(&[(tip, mid), (tip + arrow, mid - arrow), (tip + arrow, mid + arrow)]).as_drawn().cut(),
        rect(tip + arrow - 1.0, mid - shaft, x1 + 1.0, mid + shaft).cut(),
        rect(hx, mid - shaft, hx + hw, mid + shaft),
    ]
}

/// The crosshair's left arrow, pointing in: its arms at 45 degrees, a line thick and mitred
/// at the tip (a right angle), 10 design pixels wide, its outer tip 5 short of `edge` (the
/// middle), so it stands its width from its mirror; on the row `middle` [design: the
/// inspection, "make the arrows have perfect 90 degree angle"; owner's test of 2026-11-27,
/// "make the space the same as it the width of the arrow"].
fn chevron(p: Pen, edge: f32, middle: f32) -> Part {
    let o = p.t / (2.0 * std::f32::consts::SQRT_2);
    let tip = edge - p.len(5.0) - 2.0 * o;
    let run = p.len(10.0) - 3.0 * o;
    let l = tip - run;
    let c = middle;
    poly(&[(l - o, c - run + o), (l + o, c - run - o), (tip + 2.0 * o, c), (l + o, c + run + o), (l - o, c + run - o), (tip - 2.0 * o, c)]).as_drawn()
}

/// How far the chevron's tip stands from the crosshair's middle, in units.
pub const CHEVRON_TIP: f32 = 5.0 / GRID;

/// The width the right bracket is turned over in: its upright's outer side at 380 - 13.3 =
/// 366.7 units into its picture, 4 in from its piece's edge (`crate::hud`'s
/// `RIGHT_BRACKET_X`), so 370.7 of the piece, the left one's turned over about 192.
pub const CORNER_TURN: f32 = 380.0;

/// The corner brackets [design: the workshop]: the upright (15-18 design pixels) from 18
/// down, [`CORNER_H`] tall, broken under the double line (72 and a line's width under it,
/// [`CORNER_W`] long) and joined to the lower line by a quarter circle of radius 8 round where
/// their middles meet; the step cap, the half triangle 18 x 18, a pip's gap after the last
/// pip ([`pips_in`]), and the notch under the lower line, the HUD's triangle 22 x 11 on 187.
/// The right bracket is the same turned over within [`CORNER_TURN`] units [design: owner
/// 2026-11-27, "make sure the decoration on the right/left are identical"], with the divider
/// at 197 in place of the notch and the lower line broken 10 before it [observed: the
/// picture].
fn corner(right: bool, scale: f32) -> Shape {
    let p = Pen::new(scale);
    let t = p.t;
    let (x, y0, top) = (p.at(15.0), p.at(18.0), p.at(72.0));
    let (bottom, end, foot, r) = (top + 2.0 * t, x + p.len(CORNER_W), y0 + p.len(CORNER_H), p.len(8.0));
    let c = (x + t / 2.0, bottom + t / 2.0);
    let (cap, h) = (cap_px(scale), p.len(18.0));
    let mut parts = vec![
        rect(x, y0, x + t, top),
        rect(x, top, end, top + t),
        corner_arc(c, (c.0 + r, c.1), (c.0, c.1 + r)).as_drawn(),
        rect(x, bottom + r, x + t, foot),
        poly(&[(cap, top - h), (cap + h, top), (cap, top)]).as_drawn(),
    ];
    if right {
        let d = p.at(197.0);
        parts.extend([rect(x + r, bottom, d - p.len(10.0), bottom + t), rect(d, bottom, end, bottom + t), rect(d, bottom, d + t, foot)]);
    } else {
        let (n, half) = (p.at(187.0), p.len(11.0));
        parts.extend([rect(x + r, bottom, end, bottom + t), poly(&[(n - half, bottom + t), (n + half, bottom + t), (n, bottom + t + half)]).as_drawn()]);
    }
    Shape { flip_x: right.then_some(CORNER_TURN), ..in_pixels(Vec2::new(512.0, 256.0), parts, scale) }
}

/// The enemy bracket's line's top in its picture, units (its pips end 2 units over it).
const ENEMY_LINE: f32 = 21.0;
/// The enemy pips' middle in their picture, units: 191.5 design pixels across the enemy
/// piece, whose picture stands 7 units left of it (`crate::hud`'s `ENEMY_BRACKET_AT`).
const ENEMY_MIDDLE: f32 = 191.5 / GRID + 7.0;

/// The enemy's bracket: its line and the two half arrows over it, 12 x 12 design pixels,
/// their upright sides a pip's gap from the pips [design: the inspection, "move the half
/// arrows closer to the bars so they fit perfectly - same spacing as in between the bars"];
/// the line running as far past each as the HP line past its step cap [design: the third
/// review].
fn enemy_bracket(scale: f32) -> Shape {
    let p = Pen::new(scale);
    let (lefts, width) = pips_in(Decoration::EnemyBracket, scale);
    let (step, _) = crate::hud::tick_pixels(scale * 960.0);
    let gap = step - width;
    let (left, right) = (lefts[0] - gap, lefts[lefts.len() - 1] + width + gap);
    let (y, m) = ((ENEMY_LINE * scale).round(), p.len(12.0));
    let past = p.at(15.0) + p.len(CORNER_W) - cap_px(scale);
    let parts = vec![
        rect(left - past, y, right + past, y + p.t),
        poly(&[(left, y - m), (left, y), (left - m, y)]).as_drawn(),
        poly(&[(right, y - m), (right + m, y), (right, y)]).as_drawn(),
    ];
    in_pixels(Vec2::new(500.0, 64.0), parts, scale)
}

/// The health and action points bars' and the enemy bar's pips (`crate::hud::PIPS`, 37, and
/// `ENEMY_PIPS`, 31) in their bracket's picture at `scale`: each one's left in the picture's
/// pixels, and their width (`crate::hud::tick_pixels`). The health pips start a pip's gap
/// right of the upright's inner side, a whole step apart; the action points' are those
/// turned over within [`CORNER_TURN`]; the enemy's stand centred on [`ENEMY_MIDDLE`] [design:
/// the inspection, "align the hitpoints to the left edge … same spacing as in between the
/// bars"; "align the ap points to the right edge"].
pub fn pips_in(kind: Decoration, scale: f32) -> (Vec<f32>, f32) {
    let (step, width) = crate::hud::tick_pixels(scale * 960.0);
    let gap = step - width;
    let p = Pen::new(scale);
    let inner = p.at(15.0) + p.t;
    match kind {
        Decoration::RightBracket => {
            let turned = (CORNER_TURN * scale).round();
            ((0..37).map(|i| turned - inner - 37.0 * step + i as f32 * step).collect(), width)
        }
        Decoration::EnemyBracket => {
            let span = 31.0 * step - gap;
            let first = (ENEMY_MIDDLE * scale - span / 2.0).round();
            ((0..31).map(|i| first + i as f32 * step).collect(), width)
        }
        _ => ((0..37).map(|i| inner + gap + i as f32 * step).collect(), width),
    }
}

/// Where the left bracket's step cap's upright side stands at `scale`, in pixels: a pip's gap
/// after the last health pip ([`pips_in`]).
fn cap_px(scale: f32) -> f32 {
    let (lefts, width) = pips_in(Decoration::LeftBracket, scale);
    let (step, _) = crate::hud::tick_pixels(scale * 960.0);
    lefts[lefts.len() - 1] + width + (step - width)
}

/// The condition bar, 68 x 23 design pixels [`CONDITION_MARGIN`] in, with the jamming
/// threshold's two arrows cut out of it, the HUD's triangle 12 wide, pointing in from its top
/// and bottom edges at three quarters of its length (`crate::hud::JAM_CONDITION`) [design:
/// owner 2026-11-27, "the two white arrows are suppose to be a cutout into the shade"].
fn condition_bar(glow: bool, scale: f32) -> Shape {
    let p = Pen::new(scale);
    let x0 = (CONDITION_MARGIN * scale).round();
    let (w, h, half, bleed) = (p.len(68.0), p.len(23.0), p.len(6.0), p.len(1.0));
    let ax = x0 + (crate::hud::JAM_CONDITION * w).round();
    let (above, below) = (x0 - bleed, x0 + h + bleed);
    let parts = vec![
        rect(x0, x0, x0 + w, x0 + h),
        poly(&[(ax - half, above), (ax + half, above), (ax, above + half)]).as_drawn().cut(),
        poly(&[(ax - half, below), (ax + half, below), (ax, below - half)]).as_drawn().cut(),
    ];
    Shape { glow, ..in_pixels(Vec2::new(60.0, 20.0) + 2.0 * CONDITION_MARGIN, parts, scale) }
}

/// The vector condition bar's fill, units from the plain bar's left (where its clip stands,
/// `crate::hud`): the share `units` of `full` of the vector bar, which stands
/// [`CONDITION_MARGIN`] in its picture rounded to the pixel at `scale`.
pub fn sharp_fill_width(units: f32, full: f32, scale: f32) -> f32 {
    let p = Pen::new(scale);
    let x0 = (CONDITION_MARGIN * scale).round();
    let share = if full > 0.0 { (units / full).clamp(0.0, 1.0) } else { 0.0 };
    (x0 / scale - CONDITION_MARGIN + share * p.len(68.0) / scale).max(0.0)
}

/// The experience meter's bracket: [`CORNER_W`] wide from its upright's outer side (413
/// design pixels, kept: the meter is set from the view's right), the double line 72 and a
/// line's width under it, the upright from 18, [`CORNER_H`] tall, broken between the lines;
/// nine ticks a line wide from 96 at a 30-pixel step, from 55 to the line [design: the
/// third review].
fn xp_bracket(scale: f32) -> Shape {
    let p = Pen::new(scale);
    let t = p.t;
    let (up, top, y0) = (p.at(410.0), p.at(72.0), p.at(18.0));
    let (left, bottom, foot) = (up + t - p.len(CORNER_W), top + 2.0 * t, y0 + p.len(CORNER_H));
    let mut parts = vec![rect(left, top, up, top + t), rect(left, bottom, up, bottom + t), rect(up, y0, up + t, top + t), rect(up, bottom, up + t, foot)];
    let (first, step, from) = (p.at(96.0), p.len(30.0), p.at(55.0));
    parts.extend((0..9).map(|i| {
        let x = first + step * i as f32;
        rect(x, from, x + t, top)
    }));
    in_pixels(Vec2::new(512.0, 256.0), parts, scale)
}

/// The radiation meter's bracket: the line 15 design pixels down, [`CORNER_W`] long to its
/// upright's outer side (561, kept), the upright [`CORNER_H`] down; the two half arrows
/// (16 x 16, 45 degrees) hanging from the line with their upright sides at 193 and 535, and
/// between them fourteen knobs a line wide and fifteen equal gaps (a 23-pixel step), the
/// third, sixth, ninth and twelfth as long as the half arrows [design: the workshop; owner
/// 2026-11-27, "the half arrow should have a right angle"].
fn radiation_bracket(scale: f32) -> Shape {
    let p = Pen::new(scale);
    let t = p.t;
    let (outer, y) = (p.at(561.0), p.at(15.0));
    let (left, foot, under) = (outer - p.len(CORNER_W), y + p.len(CORNER_H), y + t);
    let (m, step, rl) = (p.len(16.0), p.len(23.0), p.at(193.0));
    let first = rl + step - t;
    let rr = first + 14.0 * step;
    let mut parts = vec![
        rect(left, y, outer, y + t),
        rect(outer - t, y, outer, foot),
        poly(&[(rl - m, under), (rl, under), (rl, under + m)]).as_drawn(),
        poly(&[(rr, under), (rr + m, under), (rr, under + m)]).as_drawn(),
    ];
    parts.extend((0..14).map(|i| {
        let x = first + step * i as f32;
        let long = matches!(i, 2 | 5 | 8 | 11);
        rect(x, under, x + t, under + if long { m } else { t })
    }));
    in_pixels(Vec2::new(512.0, 128.0), parts, scale)
}

/// The middle the prompt's divider is mirrored about, units (`crate::reticle`'s
/// `STEM_IN_PICTURE`).
const DIVIDER_AXIS: f32 = 160.5;

/// The prompt's divider: the top line 15 design pixels down, the lower a line's width under
/// it, broken by the two stems a line's width apart either side of [`DIVIDER_AXIS`], each
/// joined to the lower line by a quarter circle of radius 8 and running down to 66; the right
/// half the left's mirror [design: the workshop; owner 2026-11-27, "make sure the bottom 2
/// are identical just mirrored"]. (The workshop's middle stood a pixel left, at 179.5: here
/// it is the picture's 160.5 units, so the stems stay on the text's middle.)
fn prompt_divider(scale: f32) -> Shape {
    let p = Pen::new(scale);
    let t = p.t;
    // Columns x and m - 1 - x are each other's mirror ([`Shape::mirror_x`]).
    let m = (2.0 * DIVIDER_AXIS * scale).round();
    let right = ((m - t) / 2.0).floor();
    let stem = right - t;
    let (y, r) = (p.at(15.0), p.len(8.0));
    let low = y + 2.0 * t;
    let start = right - p.len(160.0);
    let c = (stem + t / 2.0, low + t / 2.0);
    let parts = vec![
        rect(start, y, m - start, y + t),
        rect(start, low, right - r, low + t),
        corner_arc(c, (c.0 - r, c.1), (c.0, c.1 + r)).as_drawn(),
        rect(stem, low + r, right, p.at(66.0)),
    ];
    Shape { mirror_x: Some(DIVIDER_AXIS), ..in_pixels(Vec2::new(512.0, 128.0), parts, scale) }
}

/// The grenade indicator's picture box, units: its width cut to the template's 151, its
/// height the picture's 128 texels at zoom 120 (`crate::grenade_indicator`).
pub const GRENADE_BOX: Vec2 = Vec2::new(151.0, 153.6);

/// The grenade indicator: the top of a ring (radius 81.1 design pixels round 86, 110.8, a
/// line thick, from -145 to -35 degrees) and the point on its top, the HUD's triangle 22 wide
/// and 11 tall, its base corners on the ring's outer edge and its base running down into the
/// ring along it, so the two join without a seam [design: the workshop].
fn grenade(p: Pen) -> Shape {
    let c = Vec2::new(p.exact(86.0), p.exact(110.8));
    let (r, half_line) = (p.exact(81.1), p.t / 2.0);
    let arc = |radius: f32, from: f32, to: f32, n: usize| -> Vec<(f32, f32)> {
        (0..=n)
            .map(|i| {
                let a = (from + (to - from) * i as f32 / n as f32).to_radians();
                (c.x + radius * a.cos(), c.y + radius * a.sin())
            })
            .collect()
    };
    let mut ring = arc(r + half_line, -145.0, -35.0, 64);
    ring.extend(arc(r - half_line, -35.0, -145.0, 64));
    let (out, half) = (r + half_line, p.exact(11.0));
    let corner = c.y - (out * out - half * half).sqrt();
    let (a_right, a_left) = ((corner - c.y).atan2(half).to_degrees(), (corner - c.y).atan2(-half).to_degrees());
    let mut point = vec![(c.x, corner - half), (c.x + half, corner)];
    point.extend(arc(r, a_right, a_left, 12));
    point.push((c.x - half, corner));
    in_pixels(GRENADE_BOX, vec![poly(&ring).as_drawn(), poly(&point).as_drawn()], p.s)
}

/// Room round the breath meter's 64-unit box for its ticks' glow, units: so that the box's
/// middle is a pixel's middle at 4K 50% and the upright and level ticks stand on whole pixels.
pub const BREATH_MARGIN: f32 = 5.5 / GRID;

/// Breath tick `n` (1 to 16): a line 9 design pixels long, its outer end 33.5 from the meter's
/// middle, turned 22.5 n degrees clockwise from the top [design: the workshop's 16 ticks 3 x
/// 9 round the middle; observed: `Tick16` at the top, `Tick01` just right of it,
/// `crate::hud_breath`].
fn breath_tick(p: Pen, n: u8) -> Shape {
    let size = Vec2::splat(64.0 + 2.0 * BREATH_MARGIN);
    let (w, h) = pixels(&Shape::new(size, Vec::new()), p.s);
    let c = Vec2::new(w as f32, h as f32) / 2.0;
    let (out, long, half) = (p.len(33.0) + 0.5, p.len(9.0), p.t / 2.0);
    let (sin, cos) = (22.5 * f32::from(n)).to_radians().sin_cos();
    let turn = |x: f32, y: f32| (c.x + x * cos - y * sin, c.y + x * sin + y * cos);
    let tick = [turn(-half, -out), turn(half, -out), turn(half, long - out), turn(-half, long - out)];
    in_pixels(size, vec![poly(&tick).as_drawn()], p.s)
}

/// The corner message's bracket's line's end, units across its picture (its upright's outer
/// side 34 design pixels in, [`CORNER_W`] on): where its text wraps while Sharp
/// (`crate::corner_messages`) [design: owner 2026-11-27, "Yes, wrap at the line"].
pub const MESSAGE_LINE_END: f32 = (34.0 + CORNER_W) / GRID;

/// The compass window's right end while Sharp, units across the health piece: where the HP
/// decoration's lines end (its upright's outer side 15 design pixels in, [`CORNER_W`] on), so
/// the rail and letters end on the same upright line as the bracket's lines [design: the
/// owner's third review].
pub const COMPASS_END: f32 = (15.0 + CORNER_W) / GRID;

/// A bar's tick (`hud_tick_mark.dds`: a block brightening from a seventh at its top to whole
/// at 17 of its 20 rows [observed: the texture]) `width` x `height` pixels, in a picture
/// `margin` pixels larger all round for its glow, drawn at 1 pixel a unit with the glow of
/// `scale` (the HUD's pixels a unit).
pub fn bar_tick(width: f32, height: f32, margin: f32, scale: f32) -> Shape {
    let (m, w, h) = (margin, width, height);
    let part = rect(m, m, m + w, m + h).fade(Fade::y(m, 34.0 / 255.0, m + h * 17.0 / 20.0, 1.0));
    Shape::new(Vec2::new(w + 2.0 * m, h + 2.0 * m), vec![part]).at_scale(scale)
}

impl Shape {
    /// A shape set in pixels drawn with the glow of `scale` pixels a unit: its lengths
    /// divided by `scale`, so drawing it at `scale` puts it on the same pixels.
    fn at_scale(self, scale: f32) -> Shape {
        let k = Vec2::splat(1.0 / scale);
        let mut s = Shape::from_texels(Vec2::ONE, k, self.parts);
        s.size = self.size * k;
        s.glow = self.glow;
        s
    }
}

// ---- the compass strip's rail ----------------------------------------------------------------

/// The strip's letters: their middles stand half a design pixel left of every 128th unit
/// (144 design pixels) [design: the workshop; observed: the strip's letters' ink, 1.3 left on
/// average].
pub const LETTER_OFFSET: f32 = -0.5 / GRID;
/// The letters' cap height and the line they stand on (45 design pixels) [observed: rows
/// 21-39 of the strip].
pub const LETTER_CAP: f32 = 19.0;
pub const LETTER_BASE: f32 = 40.0;
/// Room between a letter and the rail's line [observed: 2 to 4 units].
pub const LETTER_GAP: f32 = 3.0;

/// The compass strip's rail, 1024 x 64 units (360 degrees), at `scale`, without its letters
/// (drawn in the sharp font over it, `crate::hud`): between each two letters a line through
/// the letters' middle (33 design pixels down), stopping `half[k]` units each side of letter
/// `k`'s middle; a thick bar over it (two lines' worth, 36 to 107 past the letter's place,
/// centred between the letters) with a tick up at each end and one through its middle from 25
/// to 45 [design: the workshop]. The sectors start on the pixel nearest each 128th unit, so
/// the strip's ends meet.
pub fn compass_rail(half: [f32; 8], scale: f32) -> Shape {
    let p = Pen::new(scale);
    let t = p.t;
    let (line, bar, bar_h, ends, middle, foot) = (p.at(33.0), p.at(31.0), whole(2.0 * LINE, scale), p.at(27.0), p.at(25.0), p.at(45.0));
    let mut parts = Vec::new();
    for k in 0..8 {
        let c = 128.0 * k as f32;
        let b = (c * scale).round();
        let from = ((c + LETTER_OFFSET + half[k]) * scale).round();
        let to = ((c + 128.0 + LETTER_OFFSET - half[(k + 1) % 8]) * scale).round();
        parts.push(rect(from, line, to, line + t));
        let (l, r) = (b + p.len(36.0), b + p.len(107.0));
        parts.push(rect(l, bar, r, bar + bar_h));
        parts.push(rect(l, ends, l + t, bar));
        parts.push(rect(r - t, ends, r, bar));
        let m = ((l + r - t) / 2.0).floor();
        parts.push(rect(m, middle, m + t, foot));
    }
    Shape { wrap: true, ..in_pixels(Vec2::new(1024.0, 64.0), parts, scale) }
}

// ---- swapping the pictures ---------------------------------------------------------------

/// The shapes drawn so far at one scale: made again at another.
#[derive(Resource, Default)]
pub struct VectorPictures {
    scale: f32,
    made: HashMap<Decoration, Handle<Image>>,
}

impl VectorPictures {
    /// `kind` drawn at `scale` (pixels a unit).
    pub fn get(&mut self, kind: Decoration, scale: f32, images: &mut Assets<Image>) -> Handle<Image> {
        if self.scale != scale {
            self.made.clear();
            self.scale = scale;
        }
        self.made.entry(kind).or_insert_with(|| images.add(image(&kind.shape_at(scale), scale))).clone()
    }
}

/// A node whose picture is a [`Decoration`]: the shape while Sharp, the game's picture
/// (kept here from the node as spawned) while Original.
#[derive(Component)]
pub struct Decor {
    kind: Decoration,
    original: Option<(Handle<Image>, Option<Rect>)>,
    /// Drawn for (sharp, the scale's bits).
    drawn: Option<(bool, u32)>,
}

impl Decor {
    pub fn new(kind: Decoration) -> Decor {
        Decor { kind, original: None, drawn: None }
    }
}

/// Swap each [`Decor`]'s picture for its shape while Sharp (drawn for the view's size), back
/// to the game's picture while Original; only when the switch or the size changed, or the
/// node is new.
pub fn follow_decor(windows: Query<&Window, With<PrimaryWindow>>, mut decors: Query<(&mut Decor, &mut ImageNode)>, mut pictures: ResMut<VectorPictures>, mut images: ResMut<Assets<Image>>) {
    let Ok(window) = windows.single() else { return };
    let scale = hud_scale(window.physical_height() as f32);
    let sharp = crate::vector_font::sharp();
    let key = (sharp, scale.to_bits());
    for (mut decor, mut node) in &mut decors {
        if decor.drawn != Some(key) {
            swap(&mut decor, &mut node, sharp, scale, &mut pictures, &mut images);
        }
    }
}

/// [`follow_decor`] for one node: the shape at `scale` when `sharp`, else the game's
/// picture.
fn swap(decor: &mut Decor, node: &mut ImageNode, sharp: bool, scale: f32, pictures: &mut VectorPictures, images: &mut Assets<Image>) {
    let original = decor.original.get_or_insert_with(|| (node.image.clone(), node.rect)).clone();
    if sharp {
        node.image = pictures.get(decor.kind, scale, images);
        node.rect = None;
    } else {
        node.image = original.0;
        node.rect = original.1;
    }
    decor.drawn = Some((sharp, scale.to_bits()));
}

/// A node drawn only while Sharp (a shape with no picture to swap: the condition bar's) or
/// only while Original (what that shape stands for).
#[derive(Component, Clone, Copy)]
pub struct SharpOnly;
#[derive(Component, Clone, Copy)]
pub struct OriginalOnly;

/// Show each [`SharpOnly`] node while Sharp and each [`OriginalOnly`] one while Original.
#[allow(clippy::type_complexity)]
pub fn follow_sharp_parts(mut sharp: Query<&mut Visibility, (With<SharpOnly>, Without<OriginalOnly>)>, mut original: Query<&mut Visibility, (With<OriginalOnly>, Without<SharpOnly>)>) {
    let on = crate::vector_font::sharp();
    let show = |v: bool| if v { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut sharp {
        v.set_if_neq(show(on));
    }
    for mut v in &mut original {
        v.set_if_neq(show(!on));
    }
}

/// A bar's fill in vector: a node cut to the share of the plain fill `source` (which the
/// HUD sets, `crate::hud`) of its full width (`.1`, units), holding the bar's shape in the
/// fill's colour.
#[derive(Component, Clone, Copy)]
pub struct SharpFill(pub Entity, pub f32);

/// Each [`SharpFill`] cut to its plain fill's share of the vector bar
/// ([`sharp_fill_width`]) and its shape in that fill's colour.
pub fn follow_sharp_fills(windows: Query<&Window, With<PrimaryWindow>>, mut fills: Query<(&SharpFill, &mut Node, &Children)>, sources: Query<(&Node, &BackgroundColor), Without<SharpFill>>, mut inks: Query<&mut ImageNode>) {
    let Ok(window) = windows.single() else { return };
    let scale = hud_scale(window.physical_height() as f32);
    let _hud = crate::interface::hud_units();
    for (fill, mut node, children) in &mut fills {
        let Ok((source, colour)) = sources.get(fill.0) else { continue };
        // The plain fill's width in units (both are `crate::interface::u`'s).
        let width = match (source.width, crate::interface::u(1.0)) {
            (Val::Vh(v), Val::Vh(one)) if one > 0.0 => crate::interface::u(sharp_fill_width(v / one, fill.1, scale)),
            _ => source.width,
        };
        if node.width != width {
            node.width = width;
        }
        for child in children.iter() {
            if let Ok(mut ink) = inks.get_mut(child) {
                if ink.color != colour.0 {
                    ink.color = colour.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows (or columns) of a mask's pixel column `x` (row `y`) at least half strong.
    fn lit_rows(m: &Mask, x: usize) -> Vec<usize> {
        (0..m.height).filter(|&y| m.at(x, y) >= 0.5).collect()
    }
    fn lit_columns(m: &Mask, y: usize) -> Vec<usize> {
        (0..m.width).filter(|&x| m.at(x, y) >= 0.5).collect()
    }
    fn row(m: &Mask, y: usize) -> &[f32] {
        &m.alpha[y * m.width..(y + 1) * m.width]
    }

    /// The HUD's scales on the owner's 4K view at 50% and 100%, at 1080 and 720 lines, and
    /// some uneven ones.
    const SCALES: [f32; 7] = [1.125, 2.25, 1.125 * 0.6, 0.75, 1.0, 1.3, 0.5625];

    #[test]
    fn a_lines_thickness_is_whole_pixels_wherever_it_stands() {
        for s in SCALES {
            // The same line at three heights: the same pixels thick, Quick Loot's frame's.
            let shape = Shape::new(Vec2::new(40.0, 40.0), vec![rect(1.0, 3.3, 39.0, 3.3 + LINE), rect(1.0, 17.7, 39.0, 17.7 + LINE), rect(1.0, 30.1, 39.0, 30.1 + LINE)]);
            let m = ink(&shape, s);
            let rows = lit_rows(&m, m.width / 2);
            let want = crate::interface::line_pixels(LINE, 1.0, s * 960.0) as usize;
            assert_eq!(rows.len(), 3 * want, "scale {s}: {rows:?}");
        }
    }

    #[test]
    fn every_line_is_the_same_thickness() {
        for s in SCALES {
            let want = whole(LINE, s) as usize;
            let left = ink(&Decoration::LeftBracket.shape_at(s), s);
            // The upright, the two long lines.
            assert_eq!(lit_columns(&left, (40.0 * s) as usize).len(), want, "scale {s}: upright");
            let rows = lit_rows(&left, (200.0 * s) as usize);
            assert_eq!(rows.len(), 2 * want, "scale {s}: the double line {rows:?}");
            let xp = ink(&Decoration::XpBracket.shape_at(s), s);
            assert_eq!(lit_rows(&xp, (40.0 * s) as usize).len(), 2 * want, "scale {s}: the meter's lines");
            let enemy = ink(&Decoration::EnemyBracket.shape_at(s), s);
            assert_eq!(lit_rows(&enemy, (150.0 * s) as usize).len(), want, "scale {s}: the enemy's line");
            let rad = ink(&Decoration::RadiationBracket.shape_at(s), s);
            // (Between the left half arrow and the first knob.)
            assert_eq!(lit_rows(&rad, (180.0 * s) as usize).len(), want, "scale {s}: the radiation line");
        }
    }

    #[test]
    fn lines_end_sharply() {
        // No fading ends: every lit pixel of the brackets' lines is whole.
        let m = ink(&Decoration::LeftBracket.shape(), 1.0);
        assert!(row(&m, 65).iter().all(|&a| a == 0.0 || a == 1.0));
        assert!(lit_rows(&m, 15).iter().filter(|&&y| y < 64).all(|&y| m.at(15, y) == 1.0));
        let e = ink(&Decoration::EnemyBracket.shape(), 1.0);
        assert!(row(&e, 22).iter().all(|&a| a == 0.0 || a == 1.0));
    }

    #[test]
    fn the_right_bracket_is_the_left_one_turned_over_to_the_pixel() {
        for s in SCALES {
            let (l, r) = (ink(&Decoration::LeftBracket.shape_at(s), s), ink(&Decoration::RightBracket.shape_at(s), s));
            let n = (CORNER_TURN * s).round() as usize;
            // Over the lower line (the notch and the divider are below it) the two are
            // mirrors within the turned width; the rest of the right picture is empty.
            for y in 0..(69.0 * s).round() as usize {
                let (a, b) = (row(&l, y), row(&r, y));
                assert!((0..n).all(|x| a[x] == b[n - 1 - x]), "scale {s}: row {y}");
                assert!(b[n..].iter().all(|&v| v == 0.0));
            }
            // The upright's lower part too.
            let y = (100.0 * s) as usize;
            assert_eq!(lit_columns(&r, y).iter().map(|&x| n - 1 - x).min(), lit_columns(&l, y).first().copied(), "scale {s}");
        }
    }

    #[test]
    fn corners_meet_where_lines_join() {
        for s in SCALES {
            let m = ink(&Decoration::LeftBracket.shape_at(s), s);
            // The upright stands on the top line: no row between them.
            let col = lit_columns(&m, (30.0 * s) as usize)[0];
            let up = lit_rows(&m, col);
            let line_top = (64.0 * s).round() as usize;
            assert!(up.windows(2).take_while(|w| w[0] < line_top).all(|w| w[1] == w[0] + 1), "scale {s}: {up:?}");
            // The prompt's stems meet the lower line.
            let d = ink(&Decoration::PromptDivider.shape_at(s), s);
            let stem = lit_columns(&d, (40.0 * s) as usize)[0];
            let rows = lit_rows(&d, stem);
            assert!(rows.windows(2).all(|w| w[1] == w[0] + 1 || w[0] < (19.0 * s).round() as usize), "scale {s}: {rows:?}");
        }
    }

    #[test]
    fn mirrored_shapes_are_alike_to_the_pixel() {
        for s in SCALES {
            let d = ink(&Decoration::PromptDivider.shape_at(s), s);
            let k = (2.0 * 160.5 * s).round() as usize - 1;
            for y in 0..d.height {
                let r = row(&d, y);
                assert!((0..=k / 2).all(|x| r[x] == r[k - x]), "scale {s}: divider row {y}");
            }
            let (left, right) = (ink(&Decoration::CrossLeft.shape_at(s), s), ink(&Decoration::CrossRight.shape_at(s), s));
            for y in 0..left.height {
                let (a, b) = (row(&left, y), row(&right, y));
                assert!((0..a.len()).all(|x| a[x] == b[a.len() - 1 - x]), "scale {s}: crosshair row {y}");
            }
            let (top, bottom) = (ink(&Decoration::GunTop.shape_at(s), s), ink(&Decoration::GunBottom.shape_at(s), s));
            assert!((0..top.height).all(|y| row(&top, y) == row(&bottom, top.height - 1 - y)), "scale {s}: the gun bars");
        }
    }

    #[test]
    fn the_crosshairs_arrows_stand_their_width_apart_and_clear_of_the_box() {
        let s = 2.0;
        let left = ink(&Decoration::CrossLeft.shape_at(s), s);
        let row = (31.5 * s) as usize;
        let l = &left;
        let lit: Vec<usize> = (0..l.height).flat_map(|y| (0..l.width).filter(move |&x| l.at(x, y) > 0.0)).collect();
        let (lo, hi) = (*lit.iter().min().unwrap() as f32, *lit.iter().max().unwrap() as f32 + 1.0);
        // The gap to the mirror (twice the tip's room to the middle) is the arrow's width.
        let gap = 2.0 * (left.width as f32 - hi);
        assert!((gap - (hi - lo)).abs() <= 2.0, "gap {gap} width {}", hi - lo);
        assert!(left.at(left.width - 1, row) == 0.0);
        // The boxed crosshair: no pixel of the box touches the arrow (a clear pixel between).
        let b = ink(&Decoration::CrossFilled.shape_at(s), s);
        let m = (64.0 * s).round();
        let arrow = ink(&in_pixels(Vec2::splat(64.0), vec![chevron(Pen::new(s), m / 2.0, m / 2.0)], s), s);
        for y in 1..b.height - 1 {
            for x in 1..b.width / 2 {
                if arrow.at(x, y) > 0.0 {
                    continue;
                }
                let near = (y - 1..=y + 1).any(|v| (x - 1..=x + 1).any(|u| arrow.at(u, v) > 0.0));
                assert!(!(near && b.at(x, y) > 0.0), "the box touches the arrow at {x}, {y}: {} {:?}", b.at(x, y), (y - 1..=y + 1).map(|v| (x - 1..=x + 1).map(|u| arrow.at(u, v)).collect::<Vec<_>>()).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    fn the_pips_stand_a_gap_from_the_upright_and_the_cap_at_every_scale() {
        for s in SCALES {
            let (step, width) = crate::hud::tick_pixels(s * 960.0);
            let gap = step - width;
            let left = ink(&Decoration::LeftBracket.shape_at(s), s);
            let (pips, w) = pips_in(Decoration::LeftBracket, s);
            assert_eq!(w, width);
            // The upright's inner side: the last lit column of its row 40 units down.
            let upright_end = *lit_columns(&left, (40.0 * s) as usize).last().unwrap() as f32 + 1.0;
            assert_eq!(pips[0] - upright_end, gap, "scale {s}: the first pip");
            assert!(pips.windows(2).all(|p| p[1] - p[0] == step));
            // The cap's upright side: the first lit column of the cap's row.
            let cap_row = (60.0 * s) as usize;
            let cap = lit_columns(&left, cap_row).into_iter().find(|&x| x as f32 > upright_end).unwrap() as f32;
            assert_eq!(cap - (pips[36] + w), gap, "scale {s}: the cap");
            // The action points': the same turned over.
            let (ap, _) = pips_in(Decoration::RightBracket, s);
            let n = (CORNER_TURN * s).round();
            for (a, b) in pips.iter().zip(ap.iter().rev()) {
                assert_eq!(n - (a + w), *b, "scale {s}");
            }
            // The enemy's marks a gap either side of its pips.
            let e = ink(&Decoration::EnemyBracket.shape_at(s), s);
            let (ep, _) = pips_in(Decoration::EnemyBracket, s);
            let row = (17.0 * s) as usize;
            let lit = lit_columns(&e, row);
            let left_mark = *lit.iter().filter(|&&x| (x as f32) < ep[0]).max().unwrap() as f32 + 1.0;
            let right_mark = *lit.iter().filter(|&&x| (x as f32) > ep[30]).min().unwrap() as f32;
            assert_eq!(ep[0] - left_mark, gap, "scale {s}: the left mark");
            assert_eq!(right_mark - (ep[30] + w), gap, "scale {s}: the right mark");
        }
    }

    #[test]
    fn the_arrows_meet_at_a_right_angle() {
        // The arrow's outer edges run at 45 degrees: along its upper arm, a row lower ends a
        // pixel further right.
        let s = 4.0;
        let m = ink(&Decoration::CrossLeft.shape_at(s), s);
        let end = |y: usize| (0..m.width).rev().find(|&x| m.at(x, y) >= 0.5).unwrap() as i32;
        let rows: Vec<usize> = ((27.0 * s) as usize..(31.0 * s) as usize).collect();
        assert!(rows.windows(2).all(|w| (end(w[1]) - end(w[0]) - 1).abs() <= 1), "{:?}", rows.iter().map(|&y| end(y)).collect::<Vec<_>>());
        let slope = (end(rows[rows.len() - 1]) - end(rows[0])) as f32 / (rows.len() - 1) as f32;
        assert!((slope - 1.0).abs() < 0.1, "slope {slope}");
    }

    #[test]
    fn rounded_corners_meet_their_lines() {
        for s in SCALES {
            // The message bracket: the arc's ends on the line's and the upright's pixels.
            let m = ink(&Decoration::MessageBracket.shape_at(s), s);
            let line_top = Pen::new(s).at(12.0) as usize;
            let up_left = Pen::new(s).at(34.0) as usize;
            // Along the corner's diagonal some pixel is lit (the arc), and no gap opens between
            // the arc and either line on their rows and columns.
            let row = lit_columns(&m, line_top + 1);
            assert!(row.windows(2).all(|w| w[1] == w[0] + 1), "scale {s}: the top row {row:?}");
            let col = lit_rows(&m, up_left + 1);
            assert!(col.windows(2).all(|w| w[1] == w[0] + 1), "scale {s}: the left column {col:?}");
            // The corner itself is round: its outer corner pixel is clear.
            assert_eq!(m.at(up_left, line_top), 0.0, "scale {s}");
        }
    }

    #[test]
    fn the_half_arrows_have_a_right_angle() {
        // The radiation bracket's marks: an upright side, every row of it lit to the same
        // column, from the line down to the point.
        let s = 2.25;
        let m = ink(&Decoration::RadiationBracket.shape_at(s), s);
        let p = Pen::new(s);
        let under = (p.at(15.0) + p.t) as usize;
        let (top, bottom) = (under + 1, under + p.len(16.0) as usize - 2);
        let (step, first) = (p.len(23.0), p.at(193.0) + p.len(23.0) - p.t);
        for (side, inner) in [(p.at(193.0), false), (first + 14.0 * step, true)] {
            let edge = |y: usize| {
                // Only the triangle: from its upright side across its 16 design pixels.
                let (from, to) = if inner { (side - 1.0, side + p.len(16.0)) } else { (side - p.len(16.0), side + 1.0) };
                let lit: Vec<usize> = (0..m.width).filter(|&x| m.at(x, y) >= 0.5 && (from..to).contains(&(x as f32))).collect();
                if inner { lit[0] } else { *lit.last().unwrap() }
            };
            let want = edge(top);
            assert!((top..bottom).all(|y| edge(y) == want), "side at {side}: {:?}", (top..bottom).map(edge).collect::<Vec<_>>());
        }
    }

    #[test]
    fn the_condition_arrows_are_cut_out_of_the_bar() {
        let s = 1.0;
        let m = ink(&Decoration::ConditionGround.shape_at(s), s);
        let (x, top) = ((CONDITION_MARGIN + 45.0) as usize, CONDITION_MARGIN as usize);
        // The arrows' tips' rows: clear at the arrow, the bar either side of it.
        assert_eq!(m.at(x, top + 1), 0.0);
        assert_eq!(m.at(x, top + 18), 0.0);
        assert_eq!(m.at(x - 6, top + 1), 1.0);
        // The bar's middle row is whole across.
        assert!((top..top + 60).all(|i| m.at(i, top + 10) == 1.0));
        // The ground glows, the fill does not.
        assert!(Decoration::ConditionGround.shape().glow && !Decoration::ConditionFill.shape().glow);
    }

    #[test]
    fn the_actor_tick_stays_under_the_corner_bracket() {
        // The compass window stands 65 units down the health piece (`crate::hud`'s
        // `COMPASS_AT`), the bracket's lower line ends at 72.
        let m = ink(&Decoration::ActorTick.shape(), 1.0);
        let first = (0..m.height).find(|&y| row(&m, y).iter().any(|&a| a > 0.0)).unwrap();
        assert!(65.0 + first as f32 >= 72.0 + 2.0, "starts at {first}");
    }

    #[test]
    fn a_triangle_is_smoothed_and_as_large_as_drawn() {
        let shape = Shape::new(Vec2::new(30.0, 30.0), vec![poly(&[(5.0, 5.0), (25.0, 5.0), (5.0, 25.0)])]);
        let m = ink(&shape, 1.0);
        let area: f32 = m.alpha.iter().sum();
        // (Samples on the slant itself count out: a little under.)
        assert!((area - 200.0).abs() < 3.0, "area {area}");
        assert!(m.alpha.iter().any(|&a| a > 0.2 && a < 0.8));
        assert!(m.at(6, 6) == 1.0 && m.at(24, 24) == 0.0);
    }

    #[test]
    fn a_hollow_mark_is_empty_inside() {
        // The player's marker at 4K 50%: its outline a line across, its middle clear.
        let m = ink(&Decoration::PlayerMark.shape_at(G), G);
        assert_eq!(lit_columns(&m, 20), [5, 6, 7, 18, 19, 20]);
        let q = ink(&Decoration::QuestMark.shape_at(G), G);
        assert_eq!(lit_columns(&q, 20), (5..21).collect::<Vec<_>>());
    }

    #[test]
    fn the_glow_is_dark_round_the_shape_and_fades_out() {
        let s = 2.0;
        let shape = Shape::new(Vec2::new(40.0, 40.0), vec![rect(10.0, 19.0, 30.0, 21.0)]);
        let (w, h, rgba) = draw(&shape, s);
        assert_eq!((w, h), (80, 80));
        let px = |x: usize, y: usize| &rgba[(y * w + x) * 4..(y * w + x) * 4 + 4];
        assert_eq!(px(40, 40), &[255, 255, 255, 255]);
        let near = px(40, 37);
        assert_eq!(&near[..3], &[0, 0, 0]);
        assert!(near[3] > 60, "near {near:?}");
        assert!(px(40, 34)[3] < near[3]);
        assert_eq!(px(40, 20)[3], 0);
        assert_eq!(px(5, 40)[3], 0);
        assert!(px(40, 43)[3] > px(40, 37)[3], "under {:?} over {:?}", px(40, 43), px(40, 37));
        let reach = (glow_reach() * s).ceil() as usize;
        assert_eq!(px(40, 42 + reach)[3], 0);
    }

    #[test]
    fn every_decoration_has_its_shape_in_its_box_with_room_for_the_glow() {
        for d in Decoration::ALL {
            let shape = d.shape();
            let m = ink(&shape, 1.0);
            assert!(m.alpha.iter().any(|&a| a > 0.9), "{d:?} draws nothing");
            // The glow fits in the box but where a bar's picture runs to its edge (the gun
            // bars and the crosshair's halves meet at the middle; the marks' boxes are small).
            if !matches!(d, Decoration::CrossLeft | Decoration::CrossRight | Decoration::GunTop | Decoration::GunBottom | Decoration::GunLeft | Decoration::GunRight | Decoration::ActorTick | Decoration::QuestMark | Decoration::PlaceMark | Decoration::FoundMark | Decoration::PlayerMark | Decoration::ConditionFill | Decoration::XpPointer | Decoration::RadiationPointer) {
                let r = glow_reach() as usize;
                let edge = |x: usize, y: usize| x < r || y < r || x >= m.width - r || y >= m.height - r;
                for y in 0..m.height {
                    for x in 0..m.width {
                        assert!(!(edge(x, y) && m.at(x, y) > 0.0), "{d:?} inks {x}, {y} at its box's edge");
                    }
                }
            }
        }
    }

    #[test]
    fn a_bar_tick_brightens_downwards_on_its_pixels() {
        let scale = 2.25;
        let (w, h, m) = (7.0, 45.0, 5.0);
        let shape = bar_tick(w, h, m, scale);
        let ink = ink(&shape, scale);
        assert_eq!((ink.width, ink.height), (17, 55));
        assert_eq!(lit_columns(&ink, 45), (5..12).collect::<Vec<_>>());
        assert!(ink.at(8, 5) < 0.2 && ink.at(8, 49) == 1.0);
        assert!((5..50).all(|y| ink.at(8, y) > 0.0));
        assert_eq!(ink.at(8, 4), 0.0);
        assert_eq!(ink.at(8, 50), 0.0);
    }

    #[test]
    fn the_rail_runs_round_with_gaps_for_the_letters() {
        let rail = compass_rail([6.0; 8], 1.0);
        let m = ink(&rail, 1.0);
        assert_eq!(m.width, 1024);
        for k in 0..8 {
            let c = 128 * k;
            let at = |dx: i32| ((c + dx).rem_euclid(1024)) as usize;
            assert_eq!(lit_rows(&m, at(-1)), Vec::<usize>::new(), "letter {k}");
            assert_eq!(lit_rows(&m, at(15)).len(), whole(LINE, 1.0) as usize, "letter {k}: {:?}", lit_rows(&m, at(15)));
            assert_eq!(lit_rows(&m, at(50)).len(), 2 * whole(LINE, 1.0) as usize, "letter {k}");
            assert_eq!(lit_rows(&m, at(63)), (22..40).collect::<Vec<_>>(), "letter {k}");
        }
        let g = glow(&m, 1.0, true);
        assert!((g[28 * 1024] - g[28 * 1024 + 1023]).abs() < 0.2);
        assert!(g[28 * 1024 + 15] > 0.2, "glow over the line");
    }

    #[test]
    fn fades_run_between_their_ends() {
        let f = Fade::x(10.0, 0.0, 20.0, 1.0);
        assert_eq!((f.at(Vec2::new(0.0, 0.0)), f.at(Vec2::new(15.0, 0.0)), f.at(Vec2::new(30.0, 0.0))), (0.0, 0.5, 1.0));
        let g = Fade::y(4.0, 1.0, 8.0, 0.0).scaled(Vec2::new(1.0, 2.0));
        assert_eq!(g.at(Vec2::new(0.0, 12.0)), 0.5);
    }

    #[test]
    fn the_sharp_switch_swaps_the_pictures_and_back() {
        let mut images = Assets::<Image>::default();
        let mut pictures = VectorPictures::default();
        let original = images.add(Image::default());
        let rect = Some(Rect::new(0.0, 0.5, 1.0, 1.0));
        let mut decor = Decor::new(Decoration::LeftBracket);
        let mut node = ImageNode { rect, ..ImageNode::new(original.clone()) };
        let scale = 0.75;
        swap(&mut decor, &mut node, false, scale, &mut pictures, &mut images);
        assert_eq!(node.image, original, "Original: the picture");
        swap(&mut decor, &mut node, true, scale, &mut pictures, &mut images);
        assert_ne!(node.image, original, "Sharp: the shape");
        assert_eq!(node.rect, None);
        let image = images.get(&node.image).unwrap();
        assert_eq!((image.width(), image.height()), (384, 192), "512 x 256 units at 720 lines");
        let again = pictures.get(Decoration::LeftBracket, scale, &mut images);
        assert_eq!(again, node.image);
        swap(&mut decor, &mut node, false, scale, &mut pictures, &mut images);
        assert_eq!((node.image.clone(), node.rect), (original, rect), "Original again: the picture and its part");
    }

    // ---- the workshop's numbers [design: the owner's SVG workshop, approved 2026-11-28] ----
    //
    // At 4K with the HUD at 50% (`G`, 1.125 pixels a unit) each shape's pixels are the
    // workshop's (`tools/gen_px.py` there, its pixels); at 2.25 (4K at 100%) the same rule
    // keeps the equal gaps, lines and angles equal.

    /// 4K at 50%: the workshop's grid.
    const G: f32 = GRID;
    /// 4K at 100%: another scale for the grid rule.
    const S2: f32 = 2.25;

    fn span(from: usize, to: usize) -> Vec<usize> {
        (from..to).collect()
    }

    /// The columns of row `y` at least half lit, as runs (first, end).
    fn runs(m: &Mask, y: usize) -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for x in lit_columns(m, y) {
            match out.last_mut() {
                Some(r) if r.1 == x => r.1 = x + 1,
                _ => out.push((x, x + 1)),
            }
        }
        out
    }

    /// Whether row `y`'s lit columns end one further right (or left) each row down: a 45-degree
    /// edge.
    fn slants(ends: &[i64]) -> bool {
        ends.windows(2).all(|w| (w[1] - w[0]).abs() == 1)
    }

    #[test]
    fn the_hp_corner_is_the_workshops() {
        let m = ink(&Decoration::LeftBracket.shape_at(G), G);
        // The upright 15-18 from 18, the top line 72-75 to 403, the lower 78-81 from the
        // quarter circle, the upright again from 86 to 141: 388 wide, 123 tall.
        assert_eq!(lit_columns(&m, 40), [15, 16, 17]);
        assert_eq!(lit_rows(&m, 200), [72, 73, 74, 78, 79, 80]);
        assert_eq!(runs(&m, 73), [(15, 403)]);
        assert_eq!(lit_rows(&m, 16).first(), Some(&18));
        assert_eq!(lit_rows(&m, 16).last(), Some(&140));
        assert_eq!(runs(&m, 80).last().unwrap().1, 403);
        assert!(lit_rows(&m, 16).contains(&86) && !lit_rows(&m, 16).contains(&80), "the upright broken under the lines");
        // The quarter circle: the corner's outer pixel clear, the arc on its diagonal.
        assert_eq!(m.at(15, 78), 0.0);
        assert!(m.at(22, 85) >= 0.5, "the arc at 45 degrees");
        // The pips: 37 of 7 a 9 step from 20, the cap a gap after the last at 353.
        let (pips, w) = pips_in(Decoration::LeftBracket, G);
        assert_eq!((pips[0], pips[1] - pips[0], w, pips[36]), (20.0, 9.0, 7.0, 344.0));
        assert_eq!(cap_px(G), 353.0);
        // The step cap: the half triangle 18 x 18 on the line, its upright side at 353.
        assert!((54..72).all(|y| m.at(353, y) >= 0.5) && m.at(352, 60) == 0.0);
        let ends: Vec<i64> = (55..72).map(|y| *lit_columns(&m, y).iter().filter(|&&x| x >= 353).max().unwrap() as i64).collect();
        assert!(slants(&ends) && ends[16] == 370, "{ends:?}");
        // The notch: the HUD's triangle 22 x 11 under the lower line, symmetric about 187.
        let notch = |y: usize| runs(&m, y).into_iter().filter(|r| r.0 > 100).collect::<Vec<_>>();
        assert_eq!(notch(81), [(176, 198)]);
        for y in 81..92 {
            let r = notch(y);
            assert!(r.len() == 1 && r[0].0 + r[0].1 == 2 * 187, "row {y}: {r:?}");
        }
        assert!(notch(92).is_empty());
    }

    #[test]
    fn the_ap_corner_is_the_hp_one_turned_with_its_divider() {
        let r = ink(&Decoration::RightBracket.shape_at(G), G);
        // Turned within 428 pixels (380 units): column x of HP is 427 - x here.
        assert_eq!(lit_columns(&r, 100), [228, 229, 230, 410, 411, 412]);
        // The lower line broken 10 pixels before the divider (HP's 187-197).
        let row = lit_columns(&r, 79);
        assert!((231..241).all(|x| !row.contains(&x)) && row.contains(&230) && row.contains(&241), "{row:?}");
        // The pips the HP's turned over, flush to the upright.
        let (ap, w) = pips_in(Decoration::RightBracket, G);
        assert_eq!(410.0 - (ap[36] + w), 2.0);
    }

    #[test]
    fn the_enemy_bracket_is_the_workshops() {
        let m = ink(&Decoration::EnemyBracket.shape_at(G), G);
        let (pips, w) = pips_in(Decoration::EnemyBracket, G);
        assert_eq!((pips[0], pips[30] + w), (61.0, 338.0));
        // The line 24-27 (3), 50 pixels past each half arrow's upright side (59 and 340), as
        // the HP line past its step cap; symmetric about the pips' middle.
        assert_eq!(lit_rows(&m, 200), [24, 25, 26]);
        assert_eq!(runs(&m, 25), [(9, 390)]);
        assert_eq!((9 + 390) as f32 / 2.0, (pips[0] + pips[30] + w) / 2.0);
        // The half arrows 12 x 12, their upright sides a pip's gap from the pips.
        assert!((13..24).all(|y| m.at(58, y) >= 0.5 && m.at(59, y) == 0.0 && m.at(340, y) >= 0.5 && m.at(339, y) == 0.0));
        let ends: Vec<i64> = (13..24).map(|y| *lit_columns(&m, y).iter().filter(|&&x| x < 59).min().unwrap() as i64).collect();
        assert!(slants(&ends), "{ends:?}");
    }

    #[test]
    fn the_condition_bar_is_the_workshops() {
        let m = ink(&Decoration::ConditionGround.shape_at(G), G);
        assert_eq!((m.width, m.height), (81, 36));
        // The bar 68 x 23 at 7, 7; the arrows cut at 75% (58), 12 wide, 5 deep inside.
        assert_eq!(runs(&m, 18), [(7, 75)]);
        assert_eq!(lit_rows(&m, 20), span(7, 30));
        assert!(m.at(58, 11) <= 0.5 && m.at(58, 12) == 1.0 && m.at(58, 25) <= 0.5 && m.at(58, 24) == 1.0);
        for y in 1..36 {
            for x in 40..76 {
                if (50..66).contains(&x) {
                    assert!((m.at(x, y) - m.at(115 - x, y)).abs() < 1e-6, "the arrows symmetric about 58: {x}, {y}");
                }
                assert!((m.at(x, y) - m.at(x, 36 - y)).abs() < 1e-6, "top and bottom alike: {x}, {y}");
            }
        }
        // The fill at 80%: 54 pixels of the bar (the plain fill's 47.2 of 59 units).
        let fill = sharp_fill_width(0.8 * 59.0, 59.0, G) * G;
        assert!((fill - (0.25 + 0.8 * 68.0)).abs() < 1e-3, "{fill}");
    }

    #[test]
    fn the_xp_and_radiation_meters_are_the_workshops() {
        let m = ink(&Decoration::XpBracket.shape_at(G), G);
        // 388 wide (25-413), the double line 72-75 and 78-81, the upright 18-141 broken
        // between the lines, nine ticks 3 wide every 30 from 96, from 55 to the line.
        assert_eq!(runs(&m, 73), [(25, 413)]);
        assert_eq!(lit_rows(&m, 100), [72, 73, 74, 78, 79, 80]);
        assert_eq!(lit_columns(&m, 100), [410, 411, 412]);
        assert!(lit_columns(&m, 76).is_empty());
        assert_eq!((lit_rows(&m, 411)[0], *lit_rows(&m, 411).last().unwrap()), (18, 140));
        assert_eq!(runs(&m, 60).into_iter().filter(|r| r.0 < 400).collect::<Vec<_>>(), (0..9).map(|i| (96 + 30 * i, 99 + 30 * i)).collect::<Vec<_>>());
        assert_eq!(lit_rows(&m, 97)[0], 55);
        let p = ink(&Decoration::XpPointer.shape_at(G), G);
        assert_eq!(runs(&p, 33), [(24, 48)]);
        assert!((33..45).all(|y| runs(&p, y).iter().all(|r| r.0 + r.1 == 72)));
        let r = ink(&Decoration::RadiationBracket.shape_at(G), G);
        // The line 173-561 at 15-18, the upright 558-561 down to 138; the half arrows' sides
        // at 193 and 535, 16 x 16; fourteen knobs 3 wide every 23 from 213, the gaps all 20.
        assert_eq!(runs(&r, 16), [(173, 561)]);
        assert_eq!((lit_rows(&r, 559)[0], *lit_rows(&r, 559).last().unwrap()), (15, 137));
        let knobs: Vec<(usize, usize)> = (0..14).map(|i| (213 + 23 * i, 216 + 23 * i)).collect();
        let row = runs(&r, 19);
        assert_eq!(&row[1..15], &knobs[..]);
        assert_eq!((row[0].1, row[15].0), (193, 535));
        assert_eq!(runs(&r, 30).iter().filter(|k| k.1 - k.0 == 3 && k.0 < 540).map(|k| k.0).collect::<Vec<_>>(), [259, 328, 397, 466]);
        assert!((18..34).all(|y| r.at(192, y) >= 0.5 && r.at(193, y) == 0.0 && r.at(535, y) >= 0.5 && r.at(534, y) == 0.0));
        let ptr = ink(&Decoration::RadiationPointer.shape_at(G), G);
        assert_eq!(runs(&ptr, 17), [(17, 35)]);
        assert!((9..18).all(|y| runs(&ptr, y).iter().all(|k| k.0 + k.1 == 52)));
    }

    #[test]
    fn the_message_bracket_and_its_wrap_are_the_workshops() {
        let m = ink(&Decoration::MessageBracket.shape_at(G), G);
        // The line 12-15 from the quarter circle to 422, the upright 34-37 down to 135, the
        // quarter circle of radius 15 round where their middles meet.
        assert_eq!(lit_rows(&m, 200), [12, 13, 14]);
        assert_eq!(runs(&m, 13).last().unwrap().1, 422);
        assert_eq!(lit_columns(&m, 100), [34, 35, 36]);
        assert_eq!(*lit_rows(&m, 35).last().unwrap(), 134);
        assert_eq!(m.at(34, 12), 0.0);
        assert!(m.at(45, 23) >= 0.5 || m.at(46, 24) >= 0.5, "the arc at 45 degrees");
        // While Sharp the text wraps at the line's end: 422 / 1.125 - 130 units.
        assert!((crate::corner_messages::wrap(true) - (422.0 / G - crate::corner_messages::TEXT_X)).abs() < 1e-4);
        assert!((crate::corner_messages::wrap(true) - 245.111).abs() < 1e-3);
        assert_eq!(crate::corner_messages::wrap(false), crate::corner_messages::WRAP);
    }

    #[test]
    fn the_prompt_divider_is_the_workshops_a_pixel_right() {
        let d = ink(&Decoration::PromptDivider.shape_at(G), G);
        // The workshop's stems 175-178 and 181-184 about 179.5, here about 180.5 (the
        // picture's 160.5 units): 176-179 and 182-185, 3 apart; the top line 15-18, the
        // lower 21-24 to the quarter circles, the stems down to 66.
        assert_eq!(lit_columns(&d, 50), [176, 177, 178, 182, 183, 184]);
        assert_eq!(runs(&d, 16), [(19, 342)]);
        assert_eq!(lit_rows(&d, 100), [15, 16, 17, 21, 22, 23]);
        assert_eq!(runs(&d, 22).first().unwrap(), &(19, 171));
        assert_eq!(*lit_rows(&d, 177).last().unwrap(), 65);
    }

    #[test]
    fn the_crosshairs_are_the_workshops() {
        let l = ink(&Decoration::CrossLeft.shape_at(G), G);
        // The arrow 21-31 (10 wide), its tip 5 from the middle (36): the gap to its mirror
        // its width.
        let lit: Vec<usize> = (0..l.height).flat_map(|y| (0..l.width).filter(|&x| l.at(x, y) > 0.0).collect::<Vec<_>>()).collect();
        assert_eq!((*lit.iter().min().unwrap(), *lit.iter().max().unwrap() + 1), (21, 31));
        assert!((CHEVRON_TIP * G - 5.0).abs() < 1e-4);
        // Symmetric about its middle row (36).
        assert!((0..72).all(|y| (0..36).all(|x| (l.at(x, y) - l.at(x, 71 - y)).abs() < 1e-6)));
        let b = ink(&Decoration::CrossFilled.shape_at(G), G);
        assert_eq!(lit_columns(&b, 36).iter().filter(|&&x| x < 20).copied().collect::<Vec<_>>(), [13, 14, 15]);
        assert_eq!(lit_rows(&b, 18), [20, 21, 22, 49, 50, 51]);
        assert_eq!(runs(&b, 21), [(13, 31), (41, 59)]);
    }

    #[test]
    fn the_compass_marks_are_the_workshops() {
        let q = ink(&Decoration::QuestMark.shape_at(G), G);
        assert_eq!((q.width, q.height), (43, 43));
        // 16 x 24 (5-21, 5-29), symmetric about 13, the foot notched 4 deep.
        assert_eq!((lit_rows(&q, 13)[0], lit_rows(&q, 12)[0]), (5, 5));
        assert_eq!(*lit_rows(&q, 5).last().unwrap(), 28);
        assert!(q.at(12, 28) == 0.0 && q.at(13, 28) == 0.0 && q.at(6, 28) == 1.0 && q.at(19, 28) == 1.0);
        for y in 0..43 {
            for x in 0..26 {
                assert!((q.at(x, y) - q.at(25 - x, y)).abs() < 1e-6, "quest {x}, {y}");
            }
        }
        // A place found: its base on the same 16 columns, 8 tall; one not found its two
        // sides, 3 across; the player's marker a 3-pixel outline.
        let f = ink(&Decoration::FoundMark.shape_at(G), G);
        assert_eq!(runs(&f, 12), [(5, 21)]);
        assert!(lit_columns(&f, 4).is_empty() && !lit_columns(&f, 5).is_empty());
        let p = ink(&Decoration::PlaceMark.shape_at(G), G);
        assert_eq!(runs(&p, 12).len(), 2);
        assert!(p.at(12, 12) == 0.0 && p.at(13, 12) == 0.0);
        let pl = ink(&Decoration::PlayerMark.shape_at(G), G);
        assert_eq!(runs(&pl, 20), [(5, 8), (18, 21)]);
        // The actor's tick 10-13 from 12 to 30.
        let a = ink(&Decoration::ActorTick.shape_at(G), G);
        assert_eq!(lit_columns(&a, 20), [10, 11, 12]);
        assert_eq!((lit_rows(&a, 11)[0], *lit_rows(&a, 11).last().unwrap()), (12, 29));
        // The marks' tops 3 under the letters' baseline (45): the window's 48.
        assert!((MARK_TOP * G - 48.0).abs() < 1e-4);
        assert!((mark_anchor(Decoration::QuestMark) * G - Vec2::new(13.0, 5.0)).length() < 1e-4);
    }

    #[test]
    fn the_door_mark_is_the_workshops() {
        let d = ink(&Decoration::DoorMark.shape_at(G), G);
        // The block 11-35 by 9-33; the doorway cut 13-23; the handle 21-22 (1 px, 1 from the
        // doorway's edge), 18-24; the arrow's tip at 25 on the middle (21), 6 deep and 12
        // tall; the shaft 18-24 (6) out through the right edge.
        assert_eq!(lit_rows(&d, 11), span(9, 33));
        assert_eq!(runs(&d, 9), [(11, 35)]);
        assert_eq!(runs(&d, 20), [(11, 13), (21, 22), (23, 26)]);
        assert_eq!(lit_rows(&d, 21), span(9, 33));
        assert_eq!(lit_rows(&d, 14), [9, 10, 31, 32]);
        assert!((18..24).all(|y| (30..35).all(|x| d.at(x, y) == 0.0)), "the shaft");
        assert!(d.at(33, 17) == 1.0 && d.at(33, 24) == 1.0);
        for y in 15..27 {
            // The arrow symmetric about the middle row boundary (21).
            assert!((25..35).all(|x| (d.at(x, y) - d.at(x, 41 - y)).abs() < 1e-6), "row {y}");
        }
        assert_eq!(mark_anchor(Decoration::DoorMark) * G, Vec2::new(23.0, 9.0));
    }

    #[test]
    fn the_quest_boxes_grenade_and_breath_are_the_workshops() {
        let open = ink(&Decoration::QuestBoxOpen.shape_at(G), G);
        assert_eq!((open.width, open.height), (29, 29));
        assert_eq!(runs(&open, 5), [(6, 18)]);
        assert_eq!(runs(&open, 10), [(6, 9), (15, 18)]);
        let done = ink(&Decoration::QuestBoxDone.shape_at(G), G);
        assert_eq!(runs(&done, 10), [(6, 18)]);
        assert_eq!(lit_rows(&done, 10), span(5, 17));
        // The grenade: symmetric about 86, the point's tip at 17.94 and its base corners on
        // the ring's outer edge, the ring a line across.
        let g = ink(&Decoration::Grenade.shape_at(G), G);
        for y in 0..g.height {
            for x in 20..86 {
                assert!((g.at(x, y) - g.at(171 - x, y)).abs() <= 0.07, "grenade {x}, {y}");
            }
        }
        assert_eq!(lit_rows(&g, 85).first(), Some(&18));
        let top_of_ring = |x: usize| lit_rows(&g, x).first().copied().unwrap();
        assert_eq!(top_of_ring(60), (110.8 - (82.6f32.powi(2) - 25.5f32.powi(2)).sqrt()).round() as usize);
        assert_eq!(lit_rows(&g, 60).len(), 3);
        // The breath meter's ticks: 3 x 9, the top one 40-43 by 8-17 round 41.5; each turned
        // a quarter is the one four on.
        let t16 = ink(&Decoration::BreathTick(16).shape_at(G), G);
        assert_eq!((t16.width, t16.height), (83, 83));
        assert_eq!(runs(&t16, 10), [(40, 43)]);
        assert_eq!(lit_rows(&t16, 41), span(8, 17));
        assert!(t16.alpha.iter().all(|&a| a == 0.0 || a == 1.0), "the upright tick on whole pixels");
        for n in [1u8, 2, 3, 16] {
            let (a, b) = (ink(&Decoration::BreathTick(n).shape_at(G), G), ink(&Decoration::BreathTick((n % 16) + 4).shape_at(G), G));
            for y in 0..83 {
                for x in 0..83 {
                    // (The samples' pattern is not square: a turned slant covers within 2/16.)
                    assert!((a.at(x, y) - b.at(82 - y, x)).abs() <= 2.0 / 16.0 + 1e-4, "tick {n} at {x}, {y}");
                }
            }
        }
    }

    #[test]
    fn the_rail_is_the_workshops() {
        let rail = ink(&compass_rail([10.0; 8], G), G);
        assert_eq!(rail.width, 1152);
        // Between N and NE (from 144): the bar 180-251 by 31-38, its end ticks 27-31, the
        // middle tick 214-217 from 25 to 45, the line 33-36.
        assert_eq!(runs(&rail, 32).iter().filter(|r| r.0 >= 144 && r.1 <= 288).copied().collect::<Vec<_>>(), [(180, 251)]);
        assert_eq!(runs(&rail, 28).iter().filter(|r| r.0 >= 144 && r.1 <= 288).copied().collect::<Vec<_>>(), [(180, 183), (214, 217), (248, 251)]);
        assert_eq!(lit_rows(&rail, 215), span(25, 45));
        assert_eq!(lit_rows(&rail, 160), [33, 34, 35]);
        // The letters' middles half a pixel left of every 144.
        assert!((LETTER_OFFSET * G + 0.5).abs() < 1e-5);
    }

    /// At another scale (4K at 100%) the grid rule keeps the workshop's equalities: every
    /// line one width, the double lines a line apart, the repeated marks' gaps equal, the
    /// corner pieces one width and height, the triangles at 45 degrees, mirrors exact.
    #[test]
    fn the_grid_rule_holds_at_another_scale() {
        for s in [S2, 1.3, 0.75] {
            let p = Pen::new(s);
            let t = p.t as usize;
            let hp = ink(&Decoration::LeftBracket.shape_at(s), s);
            let rows = lit_rows(&hp, (200.0 * s) as usize);
            assert_eq!(rows.len(), 2 * t, "scale {s}");
            assert_eq!(rows[t] - rows[t - 1] - 1, t, "scale {s}: the double line's gap a line's width");
            let up = lit_columns(&hp, (40.0 * s) as usize);
            let line = runs(&hp, rows[0]);
            let width = line[0].1 - up[0];
            let height = lit_rows(&hp, up[0]).last().unwrap() + 1 - lit_rows(&hp, up[0])[0];
            assert_eq!((width as f32, height as f32), (p.len(CORNER_W), p.len(CORNER_H)), "scale {s}: HP");
            // The step cap and the notch at 45 degrees.
            let cap = cap_px(s) as usize;
            let ends: Vec<i64> = ((rows[0] - p.len(18.0) as usize + 1)..rows[0]).map(|y| *lit_columns(&hp, y).iter().filter(|&&x| x >= cap).max().unwrap() as i64).collect();
            assert!(slants(&ends), "scale {s}: the cap {ends:?}");
            // XP, radiation and the message bracket: as wide and tall as HP.
            let xp = ink(&Decoration::XpBracket.shape_at(s), s);
            let xrow = runs(&xp, lit_rows(&xp, (100.0 * s) as usize)[0]);
            assert_eq!((xrow[0].1 - xrow[0].0) as f32, p.len(CORNER_W), "scale {s}: XP");
            let rad = ink(&Decoration::RadiationBracket.shape_at(s), s);
            let rrow = runs(&rad, lit_rows(&rad, (300.0 * s) as usize)[0]);
            assert_eq!((rrow[0].1 - rrow[0].0) as f32, p.len(CORNER_W), "scale {s}: radiation");
            let knobs = runs(&rad, lit_rows(&rad, (300.0 * s) as usize)[0] + t);
            let gaps: Vec<usize> = knobs[..knobs.len() - 1].windows(2).map(|k| k[1].0 - k[0].1).collect();
            assert!(gaps.windows(2).all(|g| g[0] == g[1]), "scale {s}: the knobs' gaps {gaps:?}");
            let msg = ink(&Decoration::MessageBracket.shape_at(s), s);
            let col = lit_columns(&msg, (100.0 * s) as usize)[0];
            let mrows = lit_rows(&msg, col);
            assert_eq!((mrows.last().unwrap() + 1 - p.at(12.0) as usize) as f32, p.len(CORNER_H), "scale {s}: message");
            // The enemy line as far past each half arrow as HP's past its cap.
            let e = ink(&Decoration::EnemyBracket.shape_at(s), s);
            let (ep, w) = pips_in(Decoration::EnemyBracket, s);
            let gap = ep[1] - ep[0] - w;
            let erow = runs(&e, lit_rows(&e, (150.0 * s) as usize)[0]);
            let past = p.at(15.0) + p.len(CORNER_W) - cap_px(s);
            assert_eq!(((ep[0] - gap) - erow[0].0 as f32, erow[0].1 as f32 - (ep[30] + w + gap)), (past, past), "scale {s}: enemy");
            // The prompt's stems a line's width apart (or a pixel more) about its middle.
            let d = ink(&Decoration::PromptDivider.shape_at(s), s);
            let stems = runs(&d, (40.0 * s) as usize);
            assert_eq!(stems.len(), 2, "scale {s}");
            assert!((t..=t + 1).contains(&(stems[1].0 - stems[0].1)), "scale {s}: {stems:?}");
            assert_eq!(stems[0].0 + stems[1].1, (2.0 * DIVIDER_AXIS * s).round() as usize, "scale {s}: mirrored");
            // The XP ticks a whole step apart; the door's handle a pixel's width from the doorway.
            let ticks: Vec<(usize, usize)> = runs(&xp, lit_rows(&xp, (100.0 * s) as usize)[0] - 3).into_iter().filter(|k| (k.0 as f32) < p.at(400.0)).collect();
            assert!(ticks.windows(2).all(|k| (k[1].0 - k[0].0) as f32 == p.len(30.0)), "scale {s}: {ticks:?}");
            let door = ink(&Decoration::DoorMark.shape_at(s), s);
            let mid = (p.at(9.0) + p.len(12.0)) as usize;
            let r = runs(&door, mid - 1);
            assert_eq!(r[1].1 - r[1].0, p.len(1.0) as usize, "scale {s}: the handle {r:?}");
            assert_eq!(r[2].0 - r[1].1, p.len(1.0) as usize, "scale {s}: the handle's gap {r:?}");
        }
    }
}
