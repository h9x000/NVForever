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
//! - **The shapes** are lines, ticks and triangles at the places and sizes the pictures
//!   show them, measured in the pictures' texels [observed: each texture read with
//!   `fnvtool bsa extract`, 2026-11-27; the texels at half brightness and more are the
//!   shape, the dimmer ones around them the picture's own glow]. Every line is [`LINE`]
//!   thick, as Quick Loot's frame, and ends sharply where the picture's faded out; what is
//!   the mirror of something else is drawn as its pixels turned over, so the two are alike
//!   to the pixel [design: owner's test of 2026-11-27, "standardize all elements - same dark
//!   glow behind all elements and text, same line thickness", "make the lines end sharply
//!   instead of the gradual opacity change"].
//! - Beyond the HUD's own pieces: the crosshairs (both styles, the boxed one), the prompt's
//!   divider (`crate::reticle`), the compass's actor ticks and its quest, place and player
//!   marks (`crate::compass`), and the condition bars with their arrows cut out
//!   ([`SharpOnly`], [`SharpFill`]).
//! - **Drawn at the screen's own pixels**: each shape is drawn on the CPU into an image as
//!   many pixels across as its box covers on the screen ([`hud_scale`]), sampled texel for
//!   pixel. A rectangle's thin side (a line's thickness, a tick's width) is a whole number
//!   of pixels, as `crate::interface::line` makes the menus' lines, and its far edge is the
//!   pixel every other part ending there takes too, so corners meet ([`Snap`]); the slanted
//!   edges of the triangles are smoothed (16 samples a pixel).
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

    fn strength(&self, p: Vec2) -> f32 {
        self.fades.iter().map(|f| f.at(p)).product()
    }
}

/// A rectangle from `x0, y0` to `x1, y1`.
pub fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Part {
    Part { form: Form::Rect(Rect::new(x0, y0, x1, y1)), fades: Vec::new(), cut: false }
}

/// A polygon through `points`.
pub fn poly(points: &[(f32, f32)]) -> Part {
    Part { form: Form::Poly(points.iter().map(|&(x, y)| Vec2::new(x, y)).collect()), fades: Vec::new(), cut: false }
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
    let mut out: Vec<(f32, f32)> = (0..v.len()).map(|i| (v[i] + offset(i)).into()).collect();
    out.extend((0..v.len()).rev().map(|i| -> (f32, f32) { (v[i] - offset(i)).into() }));
    poly(&out)
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
                cut: p.cut,
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
            Form::Poly(points) => {
                let v: Vec<Vec2> = points.iter().map(|&p| snap.point(p)).collect();
                let (lo, hi) = v.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
                let (x0, x1) = (lo.x.floor().max(0.0) as usize, (hi.x.ceil().max(0.0) as usize).min(w));
                let (y0, y1) = (lo.y.floor().max(0.0) as usize, (hi.y.ceil().max(0.0) as usize).min(h));
                let step = 1.0 / SAMPLES as f32;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let mut hits = 0;
                        for sy in 0..SAMPLES {
                            for sx in 0..SAMPLES {
                                let p = Vec2::new(x as f32 + (sx as f32 + 0.5) * step, y as f32 + (sy as f32 + 0.5) * step);
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

// ---- the decorations [observed: the textures, measured 2026-11-27, module notes] ------------

/// Every line's thickness, as Quick Loot's frame (`crate::quick_loot`'s `FRAME`, drawn with
/// `crate::interface::line`): a whole number of pixels [design: owner 2026-11-27, "make them
/// the same thickness as the loot menu is", "standardize all elements"].
pub const LINE: f32 = 3.0;
/// Room round the condition bar's picture for its glow, in units.
pub const CONDITION_MARGIN: f32 = 6.0;

/// The pictures drawn as shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Decoration {
    /// `glow_hud_left_seperatorglow` (atlas piece 256 x 128) over 512 x 256.
    LeftBracket,
    /// `glow_hud_right_seperatorglow`, the same: the left one's mirror, with the divider.
    RightBracket,
    /// `glow_enemy_health_seperator.dds` (512 x 64) over 500 x 64.
    EnemyBracket,
    /// The condition bar's ground, 60 x 20 [`CONDITION_MARGIN`] in, with the jamming
    /// threshold's two arrows (`repair_meter_pips.dds`) cut out of it, and its fill (the same,
    /// without the glow: it lies on the ground).
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
    /// quest marker, a place not yet found (a hollow triangle), a place found (a whole one),
    /// the player's own marker (a hollow quest marker).
    QuestMark,
    PlaceMark,
    FoundMark,
    PlayerMark,
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
}

impl Decoration {
    pub const ALL: [Decoration; 23] = [
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
        Decoration::CrossLeft,
        Decoration::CrossRight,
        Decoration::CrossFilled,
        Decoration::GunTop,
        Decoration::GunBottom,
        Decoration::GunLeft,
        Decoration::GunRight,
        Decoration::PromptDivider,
        Decoration::MessageBracket,
    ];

    /// The shape, in its picture's box.
    pub fn shape(self) -> Shape {
        let marks = |parts: Vec<Part>| Shape::from_texels(Vec2::splat(64.0), Vec2::splat(MARK_BOX), parts);
        match self {
            Decoration::LeftBracket => corner(false),
            Decoration::RightBracket => corner(true),
            Decoration::EnemyBracket => enemy_bracket(),
            Decoration::ConditionGround => condition_bar(true),
            Decoration::ConditionFill => condition_bar(false),
            Decoration::XpBracket => xp_bracket(),
            Decoration::XpPointer => Shape::new(Vec2::splat(64.0), vec![poly(&[(21.0, 29.0), (42.0, 29.0), (31.5, 40.0)])]),
            Decoration::RadiationBracket => radiation_bracket(),
            Decoration::RadiationPointer => Shape::from_texels(Vec2::new(64.0, 32.0), Vec2::new(46.0, 24.0), vec![poly(&[(32.5, 10.0), (43.0, 21.0), (22.0, 21.0)])]),
            // A line from under the corner bracket's lower line (the window standing 65 units
            // down the piece, the line ending at 72) to where the picture's bar ended (row 27,
            // `crate::compass`'s `TICK_BAR_ROWS`) [design: owner 2026-11-27, "the NPC indicator
            // goes into the HP bar and decoration"].
            Decoration::ActorTick => Shape::new(Vec2::splat(32.0), vec![rect(8.5, 11.0, 8.5 + LINE, 27.0)]),
            Decoration::QuestMark => marks(vec![poly(&QUEST)]),
            Decoration::PlaceMark => marks(vec![poly(&PLACE), poly(&[(19.5, 13.5), (25.0, 18.0), (14.0, 18.0)]).cut()]),
            Decoration::FoundMark => marks(vec![poly(&PLACE)]),
            Decoration::PlayerMark => marks(vec![poly(&QUEST), poly(&[(19.5, 12.5), (27.5, 22.5), (27.5, 38.5), (19.5, 34.0), (11.5, 38.5), (11.5, 22.5)]).cut()]),
            Decoration::CrossLeft => Shape::new(Vec2::new(32.0, 64.0), vec![chevron()]),
            Decoration::CrossRight => Shape { flip_x: Some(32.0), ..Shape::new(Vec2::new(32.0, 64.0), vec![chevron()]) },
            Decoration::CrossFilled => {
                // The box's left side, its arms stopping short of the middle, a line's
                // thickness clear of the chevron all round, and the left chevron; the right
                // half the left's mirror [design: owner's test of 2026-11-27, "make sure they
                // do not touch the crosshair"].
                let (x0, top, bottom) = (13.0, 18.9, 44.1);
                let parts = vec![rect(x0, top, x0 + LINE, bottom), rect(x0, top, 27.0, top + LINE), rect(x0, bottom - LINE, 27.0, bottom), chevron()];
                Shape { mirror_x: Some(32.0), ..Shape::new(Vec2::splat(64.0), parts) }
            }
            // The bar's middle line 2.5 units in, its inner end at the picture's edge toward
            // the middle (`crate::reticle`'s `BAR_MIDDLE`).
            Decoration::GunTop => Shape::new(Vec2::new(8.0, 32.0), vec![rect(1.0, 12.0, 1.0 + LINE, 32.0)]),
            Decoration::GunBottom => Shape { flip_y: Some(32.0), ..Decoration::GunTop.shape() },
            Decoration::GunLeft => Shape::new(Vec2::new(32.0, 8.0), vec![rect(12.0, 1.0, 32.0, 1.0 + LINE)]),
            Decoration::GunRight => Shape { flip_x: Some(32.0), ..Decoration::GunLeft.shape() },
            Decoration::PromptDivider => prompt_divider(),
            // [observed: the picture's line rows 14-17 from x 33 and its upright x 33-36 down to
            // row 178, both faded at their far ends; here ending sharply where they fade]
            Decoration::MessageBracket => {
                let (x, y) = (33.0 * 470.0 / 512.0, 14.0 * 0.75);
                Shape::new(Vec2::new(470.0, 192.0), vec![rect(x, y, 430.0, y + LINE), rect(x, y, x + LINE, 130.0)])
            }
        }
    }
}

/// The compass marks' box (`crate::compass`'s `LANDMARK_SIZE`).
const MARK_BOX: f32 = 38.0;
/// The quest marker's outline: pointed at the top, its foot notched [observed:
/// `glow_hud_compass_objective_marker.dds`, rows 8-43].
const QUEST: [(f32, f32); 8] = [(19.5, 7.5), (31.0, 21.0), (31.0, 44.0), (28.0, 44.0), (19.5, 38.5), (11.0, 44.0), (8.0, 44.0), (8.0, 21.0)];
/// A place's triangle [observed: `glow_hud_compass_landmark(_discovered).dds`, rows 8-20].
const PLACE: [(f32, f32); 3] = [(19.5, 7.5), (30.5, 21.0), (8.5, 21.0)];

/// The crosshair's left arrow, pointing in [observed: `glow_crosshair.dds`, rows 26-36],
/// standing as far from the right one as it is wide (its tip 4.4 units left of the middle,
/// [`CHEVRON_TIP`]) [design: owner's test of 2026-11-27, "the crosshair should have some
/// space between them by default - make the space the same as it the width of the arrow";
/// the picture's tips are 2 units either side of the middle].
fn chevron() -> Part {
    let a = CHEVRON_LEFT;
    polyline(&[(a, 26.0), (a + 5.5, 31.5), (a, 37.0)], LINE)
}

/// The chevron's left points: its ends' outer corners stand `LINE / 2 / sqrt 2` left of them
/// and its tip's outer point `5.5 + LINE / sqrt 2` right, so it is 8.7 units wide and
/// stands 8.7 from its mirror at 20.
const CHEVRON_LEFT: f32 = 20.0;
/// How far the chevron's tip stands from the crosshair's middle, in units.
pub const CHEVRON_TIP: f32 = 32.0 - (CHEVRON_LEFT + 5.5 + LINE * std::f32::consts::FRAC_1_SQRT_2);

/// The width the right bracket is turned over in: its upright's outer side at 380 - 13.5 =
/// 366.5 units into its picture, 4 in from its piece's edge (`crate::hud`'s
/// `RIGHT_BRACKET_X`), so 370.5 of the piece, a mirror of the left one's about 384 / 2.
pub const CORNER_TURN: f32 = 380.0;

/// The corner brackets, measured on the left one [observed: the atlas piece, module notes]:
/// the upright (x 13.5), broken under the double line (rows 64 and 69) and joined to the
/// lower line by a slant, the step cap over the line's right end at 316 and the notch under
/// it at 166. The right bracket is the same turned over within [`CORNER_TURN`] units, so
/// that its upright stands where the picture's did and the action points' bar stands as far
/// from its cap and upright as the health bar from its own [design: owner 2026-11-27, "make
/// sure the decoration on the right/left are identical"]; in place of the notch, its divider
/// at 204 (here 176) with the lower line broken beside it [observed].
/// The left corner bracket's upright's inner side and its step cap's upright side, in its
/// picture's units ([`corner`]); the right bracket's are these turned over within
/// [`CORNER_TURN`].
pub const CORNER_UPRIGHT_INNER: f32 = 13.5 + LINE;
pub const CORNER_CAP: f32 = 316.0;
/// The enemy bracket's two marks' upright sides, in its picture's units ([`enemy_bracket`]).
pub const ENEMY_MARK_SIDES: (f32, f32) = (55.0 * 500.0 / 512.0, 307.0 * 500.0 / 512.0);

fn corner(right: bool) -> Shape {
    let (u0, top, bottom, end) = (13.5, 64.0, 69.0, 358.0);
    let mut parts = vec![
        rect(u0, 16.0, u0 + LINE, top),
        rect(u0, top, end, top + LINE),
        polyline(&[(22.5, bottom + LINE / 2.0), (u0 + LINE / 2.0, bottom + LINE / 2.0 + 7.0)], LINE),
        rect(u0, 76.0, u0 + LINE, 125.0),
        poly(&[(316.0, 48.0), (322.0, 48.0), (336.0, 64.0), (316.0, 64.0)]),
    ];
    if right {
        parts.extend([rect(20.0, bottom, 166.0, bottom + LINE), rect(174.5, bottom, end, bottom + LINE), rect(174.5, bottom, 174.5 + LINE, 125.0)]);
    } else {
        parts.extend([rect(20.0, bottom, end, bottom + LINE), poly(&[(156.0, bottom + LINE), (176.0, bottom + LINE), (166.0, bottom + LINE + 10.0)])]);
    }
    Shape { flip_x: right.then_some(CORNER_TURN), ..Shape::new(Vec2::new(512.0, 256.0), parts) }
}

/// The enemy's bracket: one line (rows 21-24) and the two corner marks over it, right
/// triangles with their upright sides at 55 and 307 [observed: the texture].
fn enemy_bracket() -> Shape {
    Shape::from_texels(
        Vec2::new(512.0, 64.0),
        Vec2::new(500.0, 64.0),
        vec![rect(16.0, 21.0, 346.0, 21.0 + LINE), poly(&[(55.0, 10.0), (55.0, 21.0), (44.0, 21.0)]), poly(&[(307.0, 10.0), (318.0, 21.0), (307.0, 21.0)])],
    )
}

/// The condition bar (60 x 20, [`CONDITION_MARGIN`] in) with the jamming threshold's two
/// arrows cut out of it, pointing in from its top and bottom edges at three quarters of
/// its length (`crate::hud::JAM_CONDITION`) [observed: `repair_meter_pips.dds`, the arrows 5
/// texels wide and 5.5 deep of 16, drawn over 22 units; design: owner 2026-11-27, "the two
/// white arrows are suppose to be a cutout into the shade"].
fn condition_bar(glow: bool) -> Shape {
    let m = CONDITION_MARGIN;
    let x = m + crate::hud::JAM_CONDITION * 60.0;
    let (half, deep) = (3.5, 6.5);
    let parts = vec![
        rect(m, m, m + 60.0, m + 20.0),
        poly(&[(x - half, m - 1.0), (x + half, m - 1.0), (x, m + deep)]).cut(),
        poly(&[(x - half, m + 21.0), (x + half, m + 21.0), (x, m + 20.0 - deep)]).cut(),
    ];
    Shape { glow, ..Shape::new(Vec2::new(60.0 + 2.0 * m, 20.0 + 2.0 * m), parts) }
}

/// The experience meter's bracket: the double line (rows 64 and 70) from x 20, the nine
/// ticks (rows 49-64) 27 apart from 85.5, the upright at 364.5 broken between the lines
/// [observed: the texture].
fn xp_bracket() -> Shape {
    let up = 364.5;
    let mut parts = vec![rect(20.0, 64.0, up, 64.0 + LINE), rect(20.0, 70.0, up, 70.0 + LINE), rect(up, 24.0, up + LINE, 64.0 + LINE), rect(up, 70.0, up + LINE, 113.0)];
    parts.extend((0..9).map(|i| {
        let x = 85.5 + 27.0 * i as f32;
        rect(x, 49.0, x + LINE, 64.0)
    }));
    Shape::new(Vec2::new(512.0, 256.0), parts)
}

/// The radiation meter's bracket: the line (rows 13-16) from x 148, the upright at 495.5
/// down to 96, the two corner marks, right triangles hanging from the line with their
/// upright sides at 176 and 471, ticks every 20 from 192 to 452, the four at 232 … 412 long
/// [observed: the texture; design: the owner 2026-11-27, "the half arrow should have a right
/// angle"].
fn radiation_bracket() -> Shape {
    let up = 495.5;
    let mut parts = vec![
        rect(148.0, 13.0, up + LINE, 13.0 + LINE),
        rect(up, 13.0, up + LINE, 96.0),
        poly(&[(161.0, 16.0), (176.0, 16.0), (176.0, 30.0)]),
        poly(&[(471.0, 16.0), (486.0, 16.0), (471.0, 30.0)]),
    ];
    parts.extend((0..14).map(|i| {
        let x = 192.0 + 20.0 * i as f32;
        let long = matches!(i, 2 | 5 | 8 | 11);
        rect(x, 16.0, x + LINE, if long { 30.0 } else { 19.0 })
    }));
    Shape::new(Vec2::new(512.0, 128.0), parts)
}

/// The prompt's divider: the double line (rows 13 and 19), the lower broken by the stem, two
/// uprights down to row 59 either side of the middle at 160.5 (`crate::reticle`'s
/// `STEM_IN_PICTURE`), the right the left's mirror [observed: the texture; design: owner
/// 2026-11-27, "make sure the bottom 2 are identical just mirrored"].
fn prompt_divider() -> Shape {
    let (middle, stem) = (160.5, 158.5);
    let parts = vec![rect(16.0, 13.0, 2.0 * middle - 16.0, 13.0 + LINE), rect(16.0, 19.0, stem, 19.0 + LINE), rect(stem - LINE, 19.0 + LINE, stem, 59.0)];
    Shape { mirror_x: Some(middle), ..Shape::new(Vec2::new(512.0, 128.0), parts) }
}

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

/// The strip's letters: their middles stand a unit left of every 128th unit (N at -1, …)
/// [observed: the strip's letters' ink, 1.3 left on average].
pub const LETTER_OFFSET: f32 = -1.0;
/// The letters' cap height and the line they stand on [observed: rows 21-39 of the strip].
pub const LETTER_CAP: f32 = 19.0;
pub const LETTER_BASE: f32 = 40.0;
/// Room between a letter and the rail's line [observed: 2 to 4 units].
pub const LETTER_GAP: f32 = 3.0;

/// The compass strip's rail, 1024 x 64 units (360 degrees), without its letters (drawn in
/// the sharp font over it, `crate::hud`): between each two letters a line through the
/// letters' middle (rows 29.5-32.5), a thick bar over it from 32 to 94 units past the
/// letter's place (two lines' thickness), with a tick up at each end and a long one through
/// its middle; the line stops `half[k]` units each side of letter `k`'s middle [observed:
/// the atlas piece, its double line, thick bars and ticks; its thick bars stand 29 to 37
/// units past their letters, here all at 32, as a compass's degrees are even; design: one
/// line thickness, owner 2026-11-27].
pub fn compass_rail(half: [f32; 8]) -> Shape {
    let mut parts = Vec::new();
    let (y, bar) = (29.5, 28.0);
    for k in 0..8 {
        let c = 128.0 * k as f32;
        let (from, to) = (c + LETTER_OFFSET + half[k], c + 128.0 + LETTER_OFFSET - half[(k + 1) % 8]);
        parts.push(rect(from, y, to, y + LINE));
        parts.push(rect(c + 32.0, bar, c + 94.0, bar + 2.0 * LINE));
        parts.push(rect(c + 32.0, 24.0, c + 32.0 + LINE, bar));
        parts.push(rect(c + 94.0 - LINE, 24.0, c + 94.0, bar));
        parts.push(rect(c + 63.0 - LINE / 2.0, 22.0, c + 63.0 + LINE / 2.0, 40.0));
    }
    Shape { wrap: true, ..Shape::new(Vec2::new(1024.0, 64.0), parts) }
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
        self.made.entry(kind).or_insert_with(|| images.add(image(&kind.shape(), scale))).clone()
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

/// A bar's fill in vector: a node cut to the width of the plain fill `source` (which the
/// HUD sets, `crate::hud`), holding the bar's shape in the fill's colour.
#[derive(Component, Clone, Copy)]
pub struct SharpFill(pub Entity);

/// Each [`SharpFill`] as wide as its plain fill and its shape in that fill's colour.
pub fn follow_sharp_fills(mut fills: Query<(&SharpFill, &mut Node, &Children)>, sources: Query<(&Node, &BackgroundColor), Without<SharpFill>>, mut inks: Query<&mut ImageNode>) {
    for (fill, mut node, children) in &mut fills {
        let Ok((source, colour)) = sources.get(fill.0) else { continue };
        if node.width != source.width {
            node.width = source.width;
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
            let left = ink(&Decoration::LeftBracket.shape(), s);
            // The upright, the two long lines.
            assert_eq!(lit_columns(&left, (40.0 * s) as usize).len(), want, "scale {s}: upright");
            let rows = lit_rows(&left, (200.0 * s) as usize);
            assert_eq!(rows.len(), 2 * want, "scale {s}: the double line {rows:?}");
            let xp = ink(&Decoration::XpBracket.shape(), s);
            assert_eq!(lit_rows(&xp, (40.0 * s) as usize).len(), 2 * want, "scale {s}: the meter's lines");
            let enemy = ink(&Decoration::EnemyBracket.shape(), s);
            assert_eq!(lit_rows(&enemy, (150.0 * s) as usize).len(), want, "scale {s}: the enemy's line");
            let rad = ink(&Decoration::RadiationBracket.shape(), s);
            assert_eq!(lit_rows(&rad, (300.0 * s) as usize).len(), want, "scale {s}: the radiation line");
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
            let (l, r) = (ink(&Decoration::LeftBracket.shape(), s), ink(&Decoration::RightBracket.shape(), s));
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
            let m = ink(&Decoration::LeftBracket.shape(), s);
            // The upright stands on the top line: no row between them.
            let col = lit_columns(&m, (30.0 * s) as usize)[0];
            let up = lit_rows(&m, col);
            let line_top = (64.0 * s).round() as usize;
            assert!(up.windows(2).take_while(|w| w[0] < line_top).all(|w| w[1] == w[0] + 1), "scale {s}: {up:?}");
            // The prompt's stems meet the lower line.
            let d = ink(&Decoration::PromptDivider.shape(), s);
            let stem = lit_columns(&d, (40.0 * s) as usize)[0];
            let rows = lit_rows(&d, stem);
            assert!(rows.windows(2).all(|w| w[1] == w[0] + 1 || w[0] < (19.0 * s).round() as usize), "scale {s}: {rows:?}");
        }
    }

    #[test]
    fn mirrored_shapes_are_alike_to_the_pixel() {
        for s in SCALES {
            let d = ink(&Decoration::PromptDivider.shape(), s);
            let k = (2.0 * 160.5 * s).round() as usize - 1;
            for y in 0..d.height {
                let r = row(&d, y);
                assert!((0..=k / 2).all(|x| r[x] == r[k - x]), "scale {s}: divider row {y}");
            }
            let (left, right) = (ink(&Decoration::CrossLeft.shape(), s), ink(&Decoration::CrossRight.shape(), s));
            for y in 0..left.height {
                let (a, b) = (row(&left, y), row(&right, y));
                assert!((0..a.len()).all(|x| a[x] == b[a.len() - 1 - x]), "scale {s}: crosshair row {y}");
            }
            let (top, bottom) = (ink(&Decoration::GunTop.shape(), s), ink(&Decoration::GunBottom.shape(), s));
            assert!((0..top.height).all(|y| row(&top, y) == row(&bottom, top.height - 1 - y)), "scale {s}: the gun bars");
        }
    }

    #[test]
    fn the_crosshairs_arrows_stand_their_width_apart_and_clear_of_the_box() {
        let s = 2.0;
        let left = ink(&Decoration::CrossLeft.shape(), s);
        let row = (31.5 * s) as usize;
        let l = &left;
        let lit: Vec<usize> = (0..l.height).flat_map(|y| (0..l.width).filter(move |&x| l.at(x, y) > 0.0)).collect();
        let (lo, hi) = (*lit.iter().min().unwrap() as f32, *lit.iter().max().unwrap() as f32 + 1.0);
        // The gap to the mirror (twice the tip's room to the middle) is the arrow's width.
        let gap = 2.0 * (left.width as f32 - hi);
        assert!((gap - (hi - lo)).abs() <= 2.0, "gap {gap} width {}", hi - lo);
        assert!(left.at(left.width - 1, row) == 0.0);
        // The boxed crosshair: no pixel of the box touches the arrow (a clear pixel between).
        let b = ink(&Decoration::CrossFilled.shape(), s);
        let arrow = ink(&Shape::new(Vec2::splat(64.0), vec![chevron()]), s);
        for y in 1..b.height - 1 {
            for x in 1..b.width / 2 {
                if arrow.at(x, y) > 0.0 {
                    continue;
                }
                let near = (y - 1..=y + 1).any(|v| (x - 1..=x + 1).any(|u| arrow.at(u, v) > 0.0));
                assert!(!(near && b.at(x, y) > 0.0), "the box touches the arrow at {x}, {y}");
            }
        }
    }

    #[test]
    fn the_half_arrows_have_a_right_angle() {
        // The radiation bracket's marks: an upright side, every row of it lit to the same
        // column, from the line down to the point.
        let s = 2.25;
        let m = ink(&Decoration::RadiationBracket.shape(), s);
        let (top, bottom) = ((16.0 * s) as usize + 1, (30.0 * s) as usize - 2);
        for (side, inner) in [(176.0 * s, false), (471.0 * s, true)] {
            let edge = |y: usize| {
                // Only the triangle: from its upright side across its 15 units.
                let (from, to) = if inner { (side - s, side + 16.0 * s) } else { (side - 16.0 * s, side + s) };
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
        let m = ink(&Decoration::ConditionGround.shape(), s);
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
        let m = ink(&Decoration::PlaceMark.shape(), 2.0);
        let k = 2.0 * MARK_BOX / 64.0;
        // The triangle's middle clear, its sides lit.
        assert_eq!(m.at((19.5 * k) as usize, (16.5 * k) as usize), 0.0);
        assert!(m.at((19.5 * k) as usize, (20.0 * k) as usize) > 0.5);
        let q = ink(&Decoration::QuestMark.shape(), 2.0);
        assert!(q.at((19.5 * k) as usize, (30.0 * k) as usize) == 1.0);
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
        let rail = compass_rail([6.0; 8]);
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
}
