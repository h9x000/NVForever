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
//!   shape, the dimmer ones around them the picture's own glow]. Lines end where the
//!   pictures' end, with the same fade where the picture fades a line out ([`Fade`]).
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
//!   ticks' blink), as a faded line's glow fades with its words. The condition bar's arrows
//!   are black, so they have no glow, as dark words have none
//!   (`crate::interface::dark_ink`).
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

/// One part of a shape and how it fades.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub form: Form,
    pub fades: Vec<Fade>,
}

impl Part {
    pub fn fade(mut self, f: Fade) -> Part {
        self.fades.push(f);
        self
    }

    fn strength(&self, p: Vec2) -> f32 {
        self.fades.iter().map(|f| f.at(p)).product()
    }
}

/// A rectangle from `x0, y0` to `x1, y1`.
pub fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Part {
    Part { form: Form::Rect(Rect::new(x0, y0, x1, y1)), fades: Vec::new() }
}

/// A polygon through `points`.
pub fn poly(points: &[(f32, f32)]) -> Part {
    Part { form: Form::Poly(points.iter().map(|&(x, y)| Vec2::new(x, y)).collect()), fades: Vec::new() }
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
}

impl Shape {
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
            })
            .collect();
        Shape { size, parts, glow: true, wrap: false }
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
            let p0 = (a * self.scale).round();
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
    let mut alpha = vec![0.0f32; w * h];
    let snap = Snap::new(shape, scale);
    let unit = |px: f32| px / scale;
    for part in &shape.parts {
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
    Mask { width: w, height: h, alpha }
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

/// The pictures drawn as shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Decoration {
    /// `glow_hud_left_seperatorglow` (atlas piece 256 x 128) over 512 x 256.
    LeftBracket,
    /// `glow_hud_right_seperatorglow`, the same.
    RightBracket,
    /// `glow_enemy_health_seperator.dds` (512 x 64) over 500 x 64.
    EnemyBracket,
    /// `repair_meter_pips.dds` (16 x 16) over 22 x 22: two black arrows.
    ConditionArrows,
    /// `glow_xp_bar_over.dds` (512 x 256) over the same.
    XpBracket,
    /// `glow_xp_bar_pointer.dds` (64 x 64) over the same.
    XpPointer,
    /// `glow_messages_radiation_seperator_right.dds` (512 x 128) over the same.
    RadiationBracket,
    /// `glow_messages_radiation_seperator_arrow.dds` (64 x 32) over 46 x 24.
    RadiationPointer,
    /// The compass's actor tick, `glow_hud_tick_mark` (atlas piece 32 x 32) over the same.
    ActorTick,
}

impl Decoration {
    pub const ALL: [Decoration; 9] = [
        Decoration::LeftBracket,
        Decoration::RightBracket,
        Decoration::EnemyBracket,
        Decoration::ConditionArrows,
        Decoration::XpBracket,
        Decoration::XpPointer,
        Decoration::RadiationBracket,
        Decoration::RadiationPointer,
        Decoration::ActorTick,
    ];

    /// The shape, in its picture's box.
    pub fn shape(self) -> Shape {
        match self {
            Decoration::LeftBracket => left_bracket(),
            Decoration::RightBracket => right_bracket(),
            Decoration::EnemyBracket => enemy_bracket(),
            Decoration::ConditionArrows => condition_arrows(),
            Decoration::XpBracket => xp_bracket(),
            Decoration::XpPointer => Shape::from_texels(Vec2::splat(64.0), Vec2::splat(64.0), vec![poly(&[(21.0, 29.0), (42.0, 29.0), (31.5, 40.0)])]),
            Decoration::RadiationBracket => radiation_bracket(),
            Decoration::RadiationPointer => Shape::from_texels(Vec2::new(64.0, 32.0), Vec2::new(46.0, 24.0), vec![poly(&[(32.5, 10.0), (43.0, 21.0), (22.0, 21.0)])]),
            // The bar brightens down to row 20 and stops at 27 (`crate::compass`'s
            // `TICK_BAR_ROWS`).
            Decoration::ActorTick => Shape::from_texels(Vec2::splat(32.0), Vec2::splat(32.0), vec![rect(7.0, 4.0, 13.0, 27.0).fade(Fade::y(4.0, 0.35, 20.0, 1.0))]),
        }
    }
}

/// The corner pieces' atlas pieces and their boxes.
const CORNER_TEXELS: Vec2 = Vec2::new(256.0, 128.0);
const CORNER_BOX: Vec2 = Vec2::new(512.0, 256.0);

/// The uprights fade in over their top 4 texels and out over their bottom 4 or 5.
fn upright_top() -> Fade {
    Fade::y(4.0, 0.0, 8.5, 1.0)
}
fn upright_bottom() -> Fade {
    Fade::y(59.5, 1.0, 64.0, 0.0)
}

/// The left corner piece: the upright (x 6-9) broken under the double line (rows 32-34 and
/// 35-37) and joined to the lower line by a slant, the notch under it at 78-88 and the step
/// cap over its right end at 158 [observed: the atlas piece].
fn left_bracket() -> Shape {
    let end = Fade::x(176.0, 1.0, 179.0, 0.35);
    Shape::from_texels(
        CORNER_TEXELS,
        CORNER_BOX,
        vec![
            rect(6.0, 4.0, 9.0, 32.0).fade(upright_top()),
            rect(6.0, 32.0, 179.0, 34.0).fade(end),
            rect(10.0, 35.0, 179.0, 37.0).fade(end),
            poly(&[(10.0, 37.0), (13.0, 37.0), (9.0, 41.0), (6.0, 41.0)]),
            rect(6.0, 41.0, 9.0, 64.0).fade(upright_bottom()),
            poly(&[(78.0, 37.0), (88.0, 37.0), (83.0, 42.0)]),
            poly(&[(158.0, 24.0), (161.0, 24.0), (168.0, 32.0), (158.0, 32.0)]),
        ],
    )
}

/// The right corner piece: the double line from x 9 (faded in to 12) to the upright at
/// 181-184, the lower line broken at 103-107 by the divider (101-103, down from it), the
/// slant from the lower line to the upright, the step cap over the left end at 33
/// [observed: the atlas piece].
fn right_bracket() -> Shape {
    let start = Fade::x(8.0, 0.0, 12.0, 1.0);
    Shape::from_texels(
        CORNER_TEXELS,
        CORNER_BOX,
        vec![
            rect(181.0, 4.0, 184.0, 32.0).fade(upright_top()),
            rect(9.0, 32.0, 184.0, 34.0).fade(start),
            rect(9.0, 35.0, 103.0, 37.0).fade(start),
            rect(107.0, 35.0, 180.0, 37.0),
            rect(101.0, 37.0, 103.0, 64.0).fade(upright_bottom()),
            poly(&[(177.0, 37.0), (180.0, 37.0), (184.0, 41.0), (181.0, 41.0)]),
            rect(181.0, 41.0, 184.0, 64.0).fade(upright_bottom()),
            poly(&[(30.0, 24.0), (33.0, 24.0), (33.0, 32.0), (23.0, 32.0)]),
        ],
    )
}

/// The enemy's bracket: one line (rows 21-24) faded in from x 8 and out to 352, and the two
/// corner marks over it, right triangles with their upright sides at 55 and 307 [observed:
/// the texture].
fn enemy_bracket() -> Shape {
    Shape::from_texels(
        Vec2::new(512.0, 64.0),
        Vec2::new(500.0, 64.0),
        vec![
            rect(8.0, 21.0, 352.0, 24.0).fade(Fade::x(8.0, 0.0, 30.0, 1.0)).fade(Fade::x(330.0, 1.0, 352.0, 0.0)),
            poly(&[(55.0, 10.0), (55.0, 21.0), (44.0, 21.0)]),
            poly(&[(307.0, 10.0), (318.0, 21.0), (307.0, 21.0)]),
        ],
    )
}

/// The jamming threshold's two arrows, pointing in from the top and the bottom, their
/// middle 3.5 texels from the left [observed: the texture; `crate::hud::ARROWS_MIDDLE`].
fn condition_arrows() -> Shape {
    let mut s = Shape::from_texels(
        Vec2::splat(16.0),
        Vec2::splat(22.0),
        vec![poly(&[(1.0, 0.0), (6.0, 0.0), (3.5, 5.5)]), poly(&[(1.0, 16.0), (6.0, 16.0), (3.5, 10.5)])],
    );
    s.glow = false;
    s
}

/// The experience meter's bracket: the double line (rows 64-67 and 70-73) faded in from x 14,
/// the nine ticks (4 wide, rows 49-64) 27 apart from 85, the upright at 364-368 from row 7
/// to 129, faded at both ends and broken between the lines [observed: the texture].
fn xp_bracket() -> Shape {
    let start = Fade::x(14.0, 0.0, 38.0, 1.0);
    let mut parts = vec![
        rect(8.0, 64.0, 364.0, 67.0).fade(start),
        rect(8.0, 70.0, 364.0, 73.0).fade(start),
        rect(364.0, 7.0, 368.0, 67.0).fade(Fade::y(7.0, 0.0, 41.0, 1.0)),
        rect(364.0, 70.0, 368.0, 129.0).fade(Fade::y(95.0, 1.0, 129.0, 0.0)),
    ];
    parts.extend((0..9).map(|i| {
        let x = 85.0 + 27.0 * i as f32;
        rect(x, 49.0, x + 4.0, 64.0)
    }));
    Shape::from_texels(Vec2::new(512.0, 256.0), Vec2::new(512.0, 256.0), parts)
}

/// The radiation meter's bracket: the line (rows 13-16) faded in from x 132 to 166, the
/// upright at 495-499 down to 106 (faded from 86), the two corner marks, right triangles
/// hanging from the line with their upright sides at 176 and 471, ticks every 20 from 192
/// to 452, the four at 232 … 412 long [observed: the texture; design: the owner 2026-11-27,
/// "the half arrow should have a right angle"].
fn radiation_bracket() -> Shape {
    let mut parts = vec![
        rect(132.0, 13.0, 495.0, 16.0).fade(Fade::x(132.0, 0.0, 166.0, 1.0)),
        rect(495.0, 13.0, 499.0, 106.0).fade(Fade::y(86.0, 1.0, 106.0, 0.0)),
        poly(&[(161.0, 16.0), (176.0, 16.0), (176.0, 30.0)]),
        poly(&[(471.0, 16.0), (486.0, 16.0), (471.0, 30.0)]),
    ];
    parts.extend((0..14).map(|i| {
        let x = 192.0 + 20.0 * i as f32;
        let long = matches!(i, 2 | 5 | 8 | 11);
        rect(x, 16.0, x + 3.0, if long { 30.0 } else { 19.0 })
    }));
    Shape::from_texels(Vec2::new(512.0, 128.0), Vec2::new(512.0, 128.0), parts)
}

/// A bar's tick (`hud_tick_mark.dds`: a block brightening from a seventh at its top to whole
/// at 17 of its 20 rows [observed: the texture]) `width` x `height` pixels, in a picture
/// `margin` pixels larger all round for its glow, drawn at 1 pixel a unit with the glow of
/// `scale` (the HUD's pixels a unit).
pub fn bar_tick(width: f32, height: f32, margin: f32, scale: f32) -> Shape {
    let (m, w, h) = (margin, width, height);
    let part = rect(m, m, m + w, m + h).fade(Fade::y(m, 34.0 / 255.0, m + h * 17.0 / 20.0, 1.0));
    Shape { size: Vec2::new(w + 2.0 * m, h + 2.0 * m), parts: vec![part], glow: true, wrap: false }.at_scale(scale)
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
/// Room between a letter and the rail's lines [observed: 2 to 4 units].
pub const LETTER_GAP: f32 = 3.0;

/// The compass strip's rail, 1024 x 64 units (360 degrees), without its letters (drawn in
/// the sharp font over it, `crate::hud`): between each two letters a double line (rows 29-31
/// and 33-34), a thick bar over it from 32 to 94 units past the letter's place, with a tick
/// up at each end (from row 26) and a long one through its middle (rows 23-37); the lines
/// stop `half[k]` units each side of letter `k`'s middle [observed: the atlas piece; its
/// thick bars stand 29 to 37 units past their letters, here all at 32, as a compass's
/// degrees are even].
pub fn compass_rail(half: [f32; 8]) -> Shape {
    let mut parts = Vec::new();
    for k in 0..8 {
        let c = 128.0 * k as f32;
        let (from, to) = (c + LETTER_OFFSET + half[k], c + 128.0 + LETTER_OFFSET - half[(k + 1) % 8]);
        parts.push(rect(from, 29.0, to, 31.0));
        parts.push(rect(from, 33.0, to, 34.0));
        parts.push(rect(c + 32.0, 29.0, c + 94.0, 34.0));
        parts.push(rect(c + 32.0, 26.0, c + 34.0, 29.0));
        parts.push(rect(c + 92.0, 26.0, c + 94.0, 29.0));
        parts.push(rect(c + 62.0, 23.0, c + 64.0, 37.0));
    }
    Shape { size: Vec2::new(1024.0, 64.0), parts, glow: true, wrap: true }
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

    /// The HUD's scales on the owner's 4K view at 50% and 100%, at 1080 and 720 lines, and
    /// some uneven ones.
    const SCALES: [f32; 7] = [1.125, 2.25, 1.125 * 0.6, 0.75, 1.0, 1.3, 0.5625];

    #[test]
    fn a_lines_thickness_is_whole_pixels_wherever_it_stands() {
        for s in SCALES {
            // The same 2-unit line at three heights: the same pixels thick.
            let shape = Shape { size: Vec2::new(40.0, 40.0), parts: vec![rect(1.0, 3.3, 39.0, 5.3), rect(1.0, 17.7, 39.0, 19.7), rect(1.0, 30.1, 39.0, 32.1)], glow: false, wrap: false };
            let m = ink(&shape, s);
            let rows = lit_rows(&m, m.width / 2);
            let want = whole(2.0, s) as usize;
            assert_eq!(rows.len(), 3 * want, "scale {s}: {rows:?}");
        }
    }

    #[test]
    fn corners_meet_where_a_line_ends_on_an_upright() {
        for s in SCALES {
            let shape = Decoration::RightBracket.shape();
            let snap = Snap::new(&shape, s);
            let m = ink(&shape, s);
            // The upright's outer edge (184 texels, 368 units) is the long line's end.
            let upright = lit_columns(&m, (30.0 * s) as usize);
            let line_row = ((64.0 * s).round() as usize).min(m.height - 1);
            let line = lit_columns(&m, line_row);
            assert_eq!(upright.last(), line.last(), "scale {s}: the line ends with the upright");
            assert_eq!(*line.last().unwrap() as f32 + 1.0, snap.edge(0, 368.0));
            // The upright is a whole 6 units' pixels wide, the line 4 units' high.
            assert_eq!(upright.len(), whole(6.0, s) as usize, "scale {s}");
            let x = (300.0 * s) as usize;
            let rows = lit_rows(&m, x);
            let (top, bottom) = rows.split_at(rows.iter().position(|&y| y > rows[0] + whole(4.0, s) as usize).unwrap_or(rows.len()));
            assert_eq!(top.len(), whole(4.0, s) as usize, "scale {s}: the upper line {rows:?}");
            assert_eq!(bottom.len(), whole(4.0, s) as usize, "scale {s}: the lower line {rows:?}");
            // And the upper upright stands on the line: no row between them.
            let col = upright[0];
            let up = lit_rows(&m, col);
            assert!(up.windows(2).take_while(|w| w[0] < line_row).all(|w| w[1] == w[0] + 1), "scale {s}: {up:?}");
        }
    }

    #[test]
    fn the_brackets_stand_where_the_pictures_lines_stand() {
        let s = 1.0;
        let m = ink(&Decoration::LeftBracket.shape(), s);
        // The double line at units 64-68 and 70-74; the upright at 12-18.
        assert_eq!(lit_rows(&m, 200), vec![64, 65, 66, 67, 70, 71, 72, 73]);
        assert_eq!(lit_columns(&m, 30), vec![12, 13, 14, 15, 16, 17]);
        // The notch under the line round 165, the step cap over it from 316.
        assert!(m.at(165, 77) > 0.9 && m.at(150, 77) == 0.0);
        assert!(m.at(318, 55) > 0.9 && m.at(312, 55) == 0.0);
        // The upright fades out at its ends.
        assert!(m.at(14, 8) < 0.1 && m.at(14, 30) > 0.99 && m.at(14, 126) < 0.2);
        let e = ink(&Decoration::EnemyBracket.shape(), 1.0);
        // Its corner marks' upright sides where `crate::hud::ENEMY_MARKS` sets the bar.
        let marks = (56.0 * 500.0 / 512.0, 307.0 * 500.0 / 512.0);
        assert!(e.at(marks.0 as usize - 2, 18) > 0.5 && e.at(marks.0 as usize + 2, 18) == 0.0, "the left mark ends at its bar");
        assert!(e.at(marks.1 as usize + 2, 18) > 0.5 && e.at(marks.1 as usize - 2, 18) == 0.0, "the right mark starts at its bar");
    }

    #[test]
    fn a_triangle_is_smoothed_and_as_large_as_drawn() {
        let shape = Shape { size: Vec2::new(30.0, 30.0), parts: vec![poly(&[(5.0, 5.0), (25.0, 5.0), (5.0, 25.0)])], glow: false, wrap: false };
        let m = ink(&shape, 1.0);
        let area: f32 = m.alpha.iter().sum();
        // (Samples on the slant itself count out: a little under.)
        assert!((area - 200.0).abs() < 3.0, "area {area}");
        // Its slant is smoothed: pixels partly covered.
        assert!(m.alpha.iter().any(|&a| a > 0.2 && a < 0.8));
        assert!(m.at(6, 6) == 1.0 && m.at(24, 24) == 0.0);
    }

    #[test]
    fn the_glow_is_dark_round_the_shape_and_fades_out() {
        let s = 2.0;
        let shape = Shape { size: Vec2::new(40.0, 40.0), parts: vec![rect(10.0, 19.0, 30.0, 21.0)], glow: true, wrap: false };
        let (w, h, rgba) = draw(&shape, s);
        assert_eq!((w, h), (80, 80));
        let px = |x: usize, y: usize| &rgba[(y * w + x) * 4..(y * w + x) * 4 + 4];
        // The line white and whole.
        assert_eq!(px(40, 40), &[255, 255, 255, 255]);
        // Just over it: black, part dark; further: lighter; far off: nothing.
        let near = px(40, 37);
        assert_eq!(&near[..3], &[0, 0, 0]);
        assert!(near[3] > 60, "near {near:?}");
        assert!(px(40, 34)[3] < near[3]);
        assert_eq!(px(40, 20)[3], 0);
        assert_eq!(px(5, 40)[3], 0);
        // The drop shadow: darker under the line than over it.
        assert!(px(40, 43)[3] > px(40, 37)[3], "under {:?} over {:?}", px(40, 43), px(40, 37));
        // Reaching no further than the glow's reach.
        let reach = (glow_reach() * s).ceil() as usize;
        assert_eq!(px(40, 42 + reach)[3], 0);
    }

    #[test]
    fn a_dark_shape_has_no_glow() {
        let (w, _, rgba) = draw(&Decoration::ConditionArrows.shape(), 2.0);
        // Away from the arrows (their middle 3.5 of 16 texels across): nothing.
        let x = (14.0 * 2.0) as usize;
        assert!((0..rgba.len() / 4 / w).all(|y| rgba[(y * w + x) * 4 + 3] == 0));
    }

    #[test]
    fn every_decoration_has_its_shape_in_its_box_with_room_for_the_glow() {
        for d in Decoration::ALL {
            let shape = d.shape();
            let m = ink(&shape, 1.0);
            assert!(m.alpha.iter().any(|&a| a > 0.9), "{d:?} draws nothing");
            // The glow fits in the box: no ink within its reach of the box's edges, but
            // where a picture's own line runs to its edge.
            if shape.glow && !matches!(d, Decoration::XpPointer | Decoration::RadiationPointer | Decoration::ActorTick) {
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
            // No line at a letter's middle; the double line beside it; the thick bar later.
            let at = |dx: i32| ((c + dx).rem_euclid(1024)) as usize;
            assert_eq!(lit_rows(&m, at(-1)), Vec::<usize>::new(), "letter {k}");
            assert_eq!(lit_rows(&m, at(15)), vec![29, 30, 33], "letter {k}");
            assert_eq!(lit_rows(&m, at(50)), vec![29, 30, 31, 32, 33], "letter {k}");
            assert_eq!(lit_rows(&m, at(62)), (23..37).collect::<Vec<_>>(), "letter {k}");
        }
        // The glow wraps: the strip's two ends glow alike.
        let g = glow(&m, 1.0, true);
        assert!((g[28 * 1024] - g[28 * 1024 + 1023]).abs() < 0.2);
        assert!(g[28 * 1024 + 15] > 0.2, "glow over the line");
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
        // 720 lines at 100%.
        let scale = 0.75;
        swap(&mut decor, &mut node, false, scale, &mut pictures, &mut images);
        assert_eq!(node.image, original, "Original: the picture");
        swap(&mut decor, &mut node, true, scale, &mut pictures, &mut images);
        assert_ne!(node.image, original, "Sharp: the shape");
        assert_eq!(node.rect, None);
        let image = images.get(&node.image).unwrap();
        assert_eq!((image.width(), image.height()), (384, 192), "512 x 256 units at 720 lines");
        // Drawn once a scale.
        let again = pictures.get(Decoration::LeftBracket, scale, &mut images);
        assert_eq!(again, node.image);
        swap(&mut decor, &mut node, false, scale, &mut pictures, &mut images);
        assert_eq!((node.image.clone(), node.rect), (original, rect), "Original again: the picture and its part");
    }
}
