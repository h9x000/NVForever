//! **The menus' lines and marks in vector** [design: owner 2026-11-27, second test of the
//! sharp letters: "disconnected lines are a common issue - make sure all corner lines
//! connect", "make the lines end sharply instead of the gradual opacity change - make them the
//! same thickness as the loot menu is", "standardize all elements - same gray background
//! opacity and same dark glow behind all elements and text, same line thickness, same
//! scrollbars"]: while the Menu Font row says Sharp ([`on`]), the menus' frames, brackets,
//! scroll bars, the titles' arrows and the container's transfer arrow are drawn from the
//! shapes here; Original keeps the game's pictures.
//!
//! - **One kit for every box**: a [`Drawing`] is a box's lines and marks in interface units
//!   from its top left; [`spawn`] bakes it with `crate::vector_hud`'s drawing at the screen's
//!   own pixels (a line a whole number of pixels thick from its near edge, every other length
//!   ending on a line's far edge on that same pixel, so corners meet) with the sharp letters'
//!   dark glow, and sets the picture over the box, tinted the node's colour.
//! - **Lines** are [`LINE`] thick, Quick Loot's frame line (`crate::quick_loot`'s `FRAME`),
//!   and end square: the game's corner lines that fade out ([`STUB`] long) end sharp.
//! - **Arrows** are solid triangles as the titles' (`interface\main\main_arrow_left.dds`,
//!   17.5 by 35 units): the scroll bars' ends are the same triangle turned, their marker a
//!   thin bar ([`scrollbar`]).
//! - **The backing** of a box is plain black at the Box Background row's darkness
//!   (`crate::interface::box_shade`), over the box only ([`backing`]).
//! - Baked pictures are kept by what they draw and the pixels a unit (made again at another
//!   view size or scale).

use crate::interface::{at, box_shade};
use crate::vector_hud::{self, Part, Shape};
use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::Mutex;

/// A frame's line, in interface units: Quick Loot's (`crate::quick_loot`'s `FRAME`).
pub const LINE: f32 = 3.0;
/// The short line at a bracket's corner, as the game's fading ones are long [documented:
/// the menus' `CORNER`, 40].
pub const STUB: f32 = 40.0;
/// The titles' arrows: 17.5 by 35 [documented: the menus' `ARROW`].
pub const ARROW: Vec2 = Vec2::new(17.5, 35.0);

/// Whether the menus draw their lines and marks here (the Menu Font row says Sharp).
pub fn on() -> bool {
    crate::vector_font::sharp()
}

/// The screen's physical pixels to an interface unit here and now (`crate::interface::u`'s
/// scale); 1 before the view's size is known.
pub fn px_per_unit() -> f32 {
    let (h, _) = crate::interface::view_px();
    if h <= 0.0 {
        return 1.0;
    }
    (h * crate::interface::unit_scale() / 960.0).max(0.1)
}

/// A direction an arrow points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// A box's lines and marks, in interface units from its top left.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    pub size: Vec2,
    pub parts: Vec<Part>,
}

impl Drawing {
    pub fn new(w: f32, h: f32) -> Drawing {
        Drawing { size: Vec2::new(w, h), parts: Vec::new() }
    }

    /// A line across from `x0` to `x1`, its top edge on `y`.
    pub fn hline(mut self, x0: f32, x1: f32, y: f32) -> Drawing {
        self.parts.push(vector_hud::rect(x0.min(x1), y, x0.max(x1), y + LINE));
        self
    }

    /// A line across from `x0` to `x1`, its bottom edge on `y`.
    pub fn hline_up(mut self, x0: f32, x1: f32, y: f32) -> Drawing {
        self.parts.push(vector_hud::rect(x0.min(x1), y - LINE, x0.max(x1), y));
        self
    }

    /// A line down from `y0` to `y1`, its left edge on `x`.
    pub fn vline(mut self, x: f32, y0: f32, y1: f32) -> Drawing {
        self.parts.push(vector_hud::rect(x, y0.min(y1), x + LINE, y0.max(y1)));
        self
    }

    /// A line down from `y0` to `y1`, its right edge on `x`.
    pub fn vline_left(mut self, x: f32, y0: f32, y1: f32) -> Drawing {
        self.parts.push(vector_hud::rect(x - LINE, y0.min(y1), x, y0.max(y1)));
        self
    }

    /// The four sides of the box from `(x0, y0)` to `(x1, y1)`, inside it.
    pub fn frame(self, x0: f32, y0: f32, x1: f32, y1: f32) -> Drawing {
        self.hline(x0, x1, y0).hline_up(x0, x1, y1).vline(x0, y0, y1).vline_left(x1, y0, y1)
    }

    /// A bracket open below: the line across `x0`..`x1` at `y` (top edge) and a [`STUB`]
    /// down from each end, inside the ends.
    pub fn bracket_down(self, x0: f32, x1: f32, y: f32) -> Drawing {
        self.hline(x0, x1, y).vline(x0, y, y + STUB).vline_left(x1, y, y + STUB)
    }

    /// A bracket open above: the line across `x0`..`x1` with its bottom edge at `y` and a
    /// [`STUB`] up from each end.
    pub fn bracket_up(self, x0: f32, x1: f32, y: f32) -> Drawing {
        self.hline_up(x0, x1, y).vline(x0, y - STUB, y).vline_left(x1, y - STUB, y)
    }

    /// A solid triangle filling the box `x, y, w, h`, pointing `dir`.
    pub fn arrow(mut self, x: f32, y: f32, w: f32, h: f32, dir: Dir) -> Drawing {
        let (x1, y1, mx, my) = (x + w, y + h, x + w / 2.0, y + h / 2.0);
        let points = match dir {
            Dir::Left => [(x1, y), (x, my), (x1, y1)],
            Dir::Right => [(x, y), (x1, my), (x, y1)],
            Dir::Up => [(x, y1), (mx, y), (x1, y1)],
            Dir::Down => [(x, y), (x1, y), (mx, y1)],
        };
        self.parts.push(vector_hud::poly(&points));
        self
    }

    /// A filled rectangle (a scroll marker, a tick).
    pub fn block(mut self, x0: f32, y0: f32, x1: f32, y1: f32) -> Drawing {
        self.parts.push(vector_hud::rect(x0, y0, x1, y1));
        self
    }

    /// A polygon through `points`.
    pub fn poly(mut self, points: &[(f32, f32)]) -> Drawing {
        self.parts.push(vector_hud::poly(points));
        self
    }

    /// The shape to bake: the drawing moved `margin` in from a box `margin` larger on every
    /// side, room for the glow.
    fn shape(&self, margin: f32) -> Shape {
        let d = Vec2::splat(margin);
        let parts = self
            .parts
            .iter()
            .map(|p| {
                let mut p = p.clone();
                p.form = match p.form {
                    vector_hud::Form::Rect(r) => vector_hud::Form::Rect(Rect::from_corners(r.min + d, r.max + d)),
                    vector_hud::Form::Poly(v) => vector_hud::Form::Poly(v.into_iter().map(|q| q + d).collect()),
                };
                p
            })
            .collect();
        Shape { size: self.size + 2.0 * d, parts, glow: true, wrap: false }
    }
}

/// The baked pictures, by what they draw and the pixels a unit.
static BAKED: Mutex<Option<HashMap<String, Handle<Image>>>> = Mutex::new(None);
/// More than this many kept: all made again as asked for.
const KEPT: usize = 512;

/// `d` baked at the screen's pixels with the glow, kept for its next drawing.
pub fn picture(d: &Drawing, images: &mut Assets<Image>) -> Handle<Image> {
    let scale = px_per_unit();
    let margin = vector_hud::glow_reach();
    let key = format!("{:?}|{}", d, scale.to_bits());
    let mut kept = BAKED.lock().unwrap_or_else(|e| e.into_inner());
    let map = kept.get_or_insert_with(HashMap::new);
    if map.len() > KEPT {
        map.clear();
    }
    map.entry(key).or_insert_with(|| images.add(vector_hud::image(&d.shape(margin), scale))).clone()
}

/// Draw `d` with its top left at `x, y` (interface units, from the parent's top left),
/// tinted `color`: its picture over a box the glow's reach larger on every side.
pub fn spawn(p: &mut ChildSpawnerCommands, d: &Drawing, x: f32, y: f32, color: Color, images: &mut Assets<Image>) -> Entity {
    let m = vector_hud::glow_reach();
    let image = picture(d, images);
    p.spawn((at(x - m, y - m, d.size.x + 2.0 * m, d.size.y + 2.0 * m), ImageNode { color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }, Pickable::IGNORE)).id()
}

/// A box's backing: plain black at the Box Background row's darkness, sharp-edged, over the
/// box only.
pub fn backing(p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32) -> Entity {
    p.spawn((at(x, y, w, h), BackgroundColor(box_shade()), Pickable::IGNORE)).id()
}

/// A scroll bar `w` wide and `h` high: a triangle pointing up at its top and down at its
/// foot, the titles' arrow turned ([`ARROW`], as wide as the bar allows), and the marker, a
/// thin bar ([`LINE`] wide, in the bar's middle) `marker` (top, height) inside the track
/// between the arrows; `None`: the arrows alone.
pub fn scrollbar(w: f32, h: f32, marker: Option<(f32, f32)>) -> Drawing {
    let (aw, ah) = scroll_arrow(w);
    let x = (w - aw) / 2.0;
    let mut d = Drawing::new(w, h).arrow(x, 0.0, aw, ah, Dir::Up).arrow(x, h - ah, aw, ah, Dir::Down);
    if let Some((top, height)) = marker {
        let mx = (w - LINE) / 2.0;
        d = d.block(mx, ah + top, mx + LINE, ah + top + height.max(LINE));
    }
    d
}

/// A scroll bar's arrow (width, height) in a bar `w` wide: the titles' arrow turned, its
/// long side at most three quarters of the bar.
pub fn scroll_arrow(w: f32) -> (f32, f32) {
    let long = ARROW.y.min(w * 0.75);
    (long, long * ARROW.x / ARROW.y)
}

/// The container menu's transfer arrow in a box `w` by `h` (the picture's 64 by 128),
/// pointing `dir` (Left or Right): a chevron of two bars and two bars across at its back,
/// the game's arrow's look in lines [observed: `interface\shared\arrow\right.dds`, the
/// owner's screenshot, 2026-11-27].
pub fn transfer_arrow(w: f32, h: f32, dir: Dir) -> Drawing {
    let t = 2.0 * LINE;
    let (tip, back) = (w * 0.85, w * 0.3);
    let my = h / 2.0;
    let arm = h * 0.45;
    // Pointing right; turned for left.
    let pts: [[(f32, f32); 4]; 2] = [
        [(back, my - arm), (back + t, my - arm), (tip + t / 2.0, my), (tip - t / 2.0, my)],
        [(tip - t / 2.0, my), (tip + t / 2.0, my), (back + t, my + arm), (back, my + arm)],
    ];
    let bars = [(0.0, my - arm * 0.45), (0.0, my + arm * 0.45)];
    let flip = |(x, y): (f32, f32)| if dir == Dir::Left { (w - x, y) } else { (x, y) };
    let mut d = Drawing::new(w, h);
    for p in pts {
        d = d.poly(&p.map(flip));
    }
    for (x0, y) in bars {
        let (a, b) = (flip((x0, y)), flip((back + t * 0.5 + (arm * 0.45) * (tip - back) / arm, y)));
        d = d.block(a.0.min(b.0), y - t / 2.0, a.0.max(b.0), y + t / 2.0);
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frames_corners_meet_on_the_pixel() {
        // At 1.125 pixels a unit (50% on a 4K view), a frame 200.3 by 90.7 units: each line's
        // far edge and the side that ends there take the same pixel.
        let d = Drawing::new(210.0, 100.0).frame(4.0, 4.0, 204.3, 94.7);
        let shape = d.shape(0.0);
        let mask = vector_hud::ink(&shape, 1.125);
        let lit = |x: usize, y: usize| mask.at(x, y) >= 0.5;
        // The right side's last lit column is the top and bottom lines' last lit column.
        let last_col = |y: usize| (0..mask.width).rev().find(|&x| lit(x, y)).unwrap();
        let top_y = (0..mask.height).find(|&y| lit(mask.width / 2, y)).unwrap();
        let bottom_y = (0..mask.height).rev().find(|&y| lit(mask.width / 2, y)).unwrap();
        let mid_y = mask.height / 2;
        assert_eq!(last_col(top_y), last_col(mid_y), "the top line ends on the right side");
        assert_eq!(last_col(bottom_y), last_col(mid_y), "the bottom line ends on the right side");
        // Every side is as thick (whole pixels).
        let thick_h = (0..mask.height).filter(|&y| lit(mask.width / 2, y)).count();
        let thick_v = (0..mask.width).filter(|&x| lit(x, mid_y)).count();
        assert_eq!(thick_h, thick_v, "top + bottom as thick as left + right");
    }

    #[test]
    fn the_scroll_bars_arrows_are_the_titles_turned() {
        let (w, h) = scroll_arrow(32.0);
        assert!((w / h - 2.0).abs() < 1e-4 && w <= 24.0 + 1e-4, "{w} {h}");
        let d = scrollbar(32.0, 365.0, Some((10.0, 50.0)));
        assert_eq!(d.parts.len(), 3);
        assert_eq!(scrollbar(32.0, 365.0, None).parts.len(), 2);
    }

    #[test]
    fn the_transfer_arrow_points_both_ways_as_mirrors() {
        let r = transfer_arrow(64.0, 128.0, Dir::Right);
        let l = transfer_arrow(64.0, 128.0, Dir::Left);
        let ink = |d: &Drawing| vector_hud::ink(&d.shape(0.0), 2.0);
        let (mr, ml) = (ink(&r), ink(&l));
        for y in 0..mr.height {
            for x in 0..mr.width {
                assert!((mr.at(x, y) - ml.at(mr.width - 1 - x, y)).abs() < 0.2, "{x} {y}");
            }
        }
    }
}
