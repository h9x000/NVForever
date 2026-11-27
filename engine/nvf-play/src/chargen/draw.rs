//! What the character-creation menus are drawn with: the game's two menu fonts, the
//! interface's pictures and the pieces the menu files build from (lines, lines fading
//! at the corners, the highlight box, text set left, right or centred). Lengths are in
//! the interface's units (960 to the view's height), from the parent node's top left.

use crate::interface::{amber, at, plain_font, set_line, u, Face, Picture, Pictures};
use bevy::prelude::*;
use fnv_font::{Font, Line};
use nvf_assets::GameData;
use crate::vector_menu::Drawing;

/// Font 1 (glowing), font 2, the body text, and font 6, the titles [documented:
/// Fallout_default.ini, docs/spec-notes/fonts.md].
const GLOW_FONT: &str = "textures\\fonts\\glow_monofonto_large.fnt";
const BODY_FONT: &str = "textures\\fonts\\monofonto_large.fnt";
const TITLE_FONT: &str = "textures\\fonts\\glow_monofonto_vl_dialogs.fnt";

/// Which of the menus' fonts a text is set in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Glow,
    Body,
    Title,
}

/// Every line is 2 thick [documented: menus\globals.xml, on a PC].
pub const THICK: f32 = 2.0;
/// The short lines at a bracket's corners [documented: bottom_bracket.xml, the
/// headline's vertical fade lines; the 40 is message-menu.md §2's].
pub const CORNER: f32 = 40.0;
/// How far under a text's top its letters stand: the body font and the title font
/// (message-menu.md §3).
pub const BODY_BASE: f32 = 15.0;
pub const TITLE_BASE: f32 = 21.5;
/// A body text's line height (the font's 31) and the pitch of a wrapped text's lines
/// (message-menu.md §3).
pub const BODY_H: f32 = 31.0;
pub const LINE_PITCH: f32 = 35.0;

/// The fonts and pictures, read when the first menu opens.
pub struct Kit {
    pub glow: Face,
    pub body: Face,
    pub title: Face,
    pub pictures: Pictures,
}

impl Kit {
    pub fn load(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Kit {
        Kit {
            glow: crate::interface::screen_face(GLOW_FONT, plain_font(31.0, 12.0), data, images, bc),
            body: crate::interface::screen_face(BODY_FONT, plain_font(31.0, 12.0), data, images, bc),
            title: crate::interface::screen_face(TITLE_FONT, plain_font(34.0, 14.0), data, images, bc),
            pictures: Pictures::load(data),
        }
    }
}

/// Where a text's letters start for its anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Left,
    Right,
    Centre,
}

/// The first line of a text in a font (all of it when it has no line breaks).
pub fn first_line(font: &Font, s: &str) -> Line {
    font.lines(s, None).into_iter().next().unwrap_or_default()
}

/// A rectangle's four sides, `thick` wide, as one box with a border. Four lines of their own
/// each took their place and their length to the pixel on their own, so at some sizes a side
/// came out a pixel short of the line it should meet or a pixel past its end [observed: the
/// owner's shots of the creator's buttons at 50% menu scale on a 4K screen, 2026-11-27]; a
/// border is laid out with its box and always closes.
pub fn outline(p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, thick: f32, color: Color) {
    p.spawn((Node { border: UiRect::all(u(thick)), ..at(x, y, w, h) }, BorderColor::all(color)));
}

// ---- the lines with Sharp on (`crate::vector_menu`) -----------------------------------------
//
// With the Menu Font row at Sharp the menus' lines are the kit's drawings [design: owner
// 2026-11-27, "make sure all corner lines connect", "make the lines end sharply instead of
// the gradual opacity change", "same line thickness"]: `crate::vector_menu::LINE` thick, the
// fading corner lines drawn to their end and ending square, one drawing a connected frame so
// its corners meet on the pixel, with the letters' dark glow. Original keeps the pictures.

/// The bottom bracket in a box `w` by [`CORNER`]: the line along its foot and a side up
/// from each end.
pub fn bottom_bracket_drawing(w: f32) -> Drawing {
    Drawing::new(w, CORNER).hline_up(0.0, w, CORNER).vline(0.0, 0.0, CORNER).vline_left(w, 0.0, CORNER)
}

/// The top bracket in a box `w` by `h`: the line along its top and a side `h` down from
/// each end.
pub fn top_bracket_drawing(w: f32, h: f32) -> Drawing {
    Drawing::new(w, h).hline(0.0, w, 0.0).vline(0.0, 0.0, h).vline_left(w, 0.0, h)
}

/// A rectangle's four sides, inside it.
pub fn box_drawing(w: f32, h: f32) -> Drawing {
    Drawing::new(w, h).frame(0.0, 0.0, w, h)
}

/// A headline in a box `w` by `corner`: the line along the top, open from `gap.0` to
/// `gap.1` (the title; none when the title runs past the end), and a side `corner` long
/// down from each end.
pub fn headline_drawing(w: f32, gap: (f32, f32), corner: f32) -> Drawing {
    let d = Drawing::new(w, corner).hline(0.0, gap.0, 0.0).vline(0.0, 0.0, corner).vline_left(w, 0.0, corner);
    if gap.1 < w {
        d.hline(gap.1, w, 0.0)
    } else {
        d
    }
}

/// Lines across `w`, their tops at `ys`, each with a side `corner` long down from its right
/// end (the information boxes' lines), in a box as high as the last one's side reaches.
pub fn corner_lines_drawing(w: f32, ys: &[f32], corner: f32) -> Drawing {
    let h = ys.iter().fold(0.0f32, |m, &y| m.max(y + corner));
    ys.iter().fold(Drawing::new(w, h), |d, &y| d.hline(0.0, w, y).vline_left(w, y, y + corner))
}

/// The baseline that puts a line's capitals (their ink: the game font's pictures hold a glow
/// round the letter, as much over it as under the line) with their middle on `mid`, for a
/// face drawn `scale` times its size [design; observed: the middle of the pictures of H, D,
/// E, O and 0 is that of their ink within half a unit in fonts 1, 2, 6 and 8].
pub fn cap_base(face: &Face, mid: f32, scale: f32) -> f32 {
    mid + crate::quick_loot::cap(face) * scale / 2.0
}

/// The other way round: the middle of the capitals of a line standing on `base`.
pub fn cap_mid(face: &Face, base: f32, scale: f32) -> f32 {
    base - crate::quick_loot::cap(face) * scale / 2.0
}

/// Where a line's ink runs from its start, left and right, in the face's units: from the
/// first picture's left edge to the last one's right (the glow round a game font's letters
/// is as wide on both sides, so their middle is the ink's), not its advances, which end with
/// the room after the last letter. A sharp line is drawn `shift` right of its place
/// (`crate::interface::VectorFace`) and its pictures are its advances.
pub fn ink_span(face: &Face, line: &Line) -> (f32, f32) {
    if let Some(v) = &face.vector {
        return (v.shift, v.shift + line.width);
    }
    line.glyphs
        .iter()
        .filter_map(|placed| {
            let g = face.font.glyph(placed.code);
            g.drawn().then_some((placed.x + g.before, placed.x + g.before + g.width))
        })
        .reduce(|(l, r), (a, b)| (l.min(a), r.max(b)))
        .unwrap_or((0.0, line.width))
}

/// Where a line drawn `scale` times its size starts so that its ink's middle is on `mid`.
pub fn ink_centre_x(face: &Face, line: &Line, mid: f32, scale: f32) -> f32 {
    let (l, r) = ink_span(face, line);
    mid - (l + r) / 2.0 * scale
}

pub struct Ctx<'a> {
    pub kit: &'a mut Kit,
    pub data: Option<&'a GameData>,
    pub images: &'a mut Assets<Image>,
    pub bc: bool,
    pub glyphs: &'a crate::pad::PadGlyphs,
}

impl Ctx<'_> {
    pub fn picture(&mut self, path: &str) -> Option<Picture> {
        self.kit.pictures.picture(path, self.data, self.images, self.bc)
    }

    /// A picture tinted `color`, stretched over `node`, with `extra` components.
    pub fn image(&mut self, p: &mut ChildSpawnerCommands, path: &str, node: Node, color: Color, extra: impl Bundle) -> Option<Entity> {
        let (image, rect) = self.picture(path)?;
        Some(p.spawn((node, ImageNode { rect, color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }, extra)).id())
    }

    /// A straight line from the atlas's solid piece (or a plain fill).
    pub fn line(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if self.image(p, "solid.dds", at(x, y, w, h), color, ()).is_none() {
            p.spawn((at(x, y, w, h), BackgroundColor(color)));
        }
    }

    /// A line `len` long that fades out downwards (or upwards, `up`), its top at `y`.
    pub fn fade(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, len: f32, up: bool, color: Color) {
        let name = if up { "fade_to_top.dds" } else { "fade_to_bottom.dds" };
        if self.image(p, name, at(x, y, THICK, len), color, ()).is_none() {
            p.spawn((at(x, y, THICK, len), BackgroundColor(color.with_alpha(0.5))));
        }
    }

    /// Whether the lines are drawn in vector (the Menu Font row says Sharp,
    /// `crate::vector_menu::on`).
    pub fn sharp(&self) -> bool {
        crate::vector_menu::on()
    }

    /// A vector drawing with its top left at `x, y`, tinted `color` (`crate::vector_menu::spawn`).
    pub fn drawing(&mut self, p: &mut ChildSpawnerCommands, d: &Drawing, x: f32, y: f32, color: Color) -> Entity {
        crate::vector_menu::spawn(p, d, x, y, color, self.images)
    }

    /// The bottom bracket (bottom_bracket.xml): a line `w` long at `y` with a short line
    /// fading up at each end; Sharp: [`bottom_bracket_drawing`], its foot where the line's is.
    pub fn bottom_bracket(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, color: Color) {
        if self.sharp() {
            self.drawing(p, &bottom_bracket_drawing(w), x, y + THICK - CORNER, color);
            return;
        }
        self.line(p, x, y, w, THICK, color);
        for xx in [x, x + w - THICK] {
            self.fade(p, xx, y - CORNER + THICK, CORNER, true, color);
        }
    }

    /// The top bracket (top_bracket.xml): a line `w` long at `y` with a line fading down
    /// `h` at each end; Sharp: [`top_bracket_drawing`].
    pub fn top_bracket(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if self.sharp() {
            self.drawing(p, &top_bracket_drawing(w, h), x, y, color);
            return;
        }
        self.line(p, x, y, w, THICK, color);
        for xx in [x, x + w - THICK] {
            self.fade(p, xx, y, h, false, color);
        }
    }

    /// The menus' headline over a box `w` wide at `x, y`: a line from the left end to
    /// `gap.0` and from `gap.1` to the right end (the title between), and a line `corner`
    /// long fading down from each end; Sharp: [`headline_drawing`].
    #[allow(clippy::too_many_arguments)]
    pub fn headline(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, gap: (f32, f32), corner: f32, color: Color) {
        if self.sharp() {
            self.drawing(p, &headline_drawing(w, gap, corner), x, y, color);
            return;
        }
        self.line(p, x, y, gap.0, THICK, color);
        self.fade(p, x, y, corner, false, color);
        self.line(p, x + gap.1, y, w - gap.1, THICK, color);
        self.fade(p, x + w - THICK, y, corner, false, color);
    }

    /// The information boxes' lines at `x, y + ys[k]`: each 3 short of `w` with a line
    /// `corner` long fading down from the box's right end [documented: the menu files'
    /// widths]; Sharp: [`corner_lines_drawing`], each line meeting its side.
    #[allow(clippy::too_many_arguments)]
    pub fn corner_lines(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, ys: &[f32], corner: f32, color: Color) {
        if self.sharp() {
            self.drawing(p, &corner_lines_drawing(w, ys, corner), x, y, color);
            return;
        }
        for yy in ys {
            self.line(p, x, y + yy, w - 3.0, THICK, color);
            self.fade(p, x + w - THICK, y + yy, corner, false, color);
        }
    }

    /// A list's scroll bar `w` by `h` at `x, y` with its marker (`top`, height from the
    /// bar's top, as `crate::chargen::lists::marker` gives it), Sharp only: the menus' bar
    /// (`crate::vector_menu::scrollbar`), the marker moved into its track.
    #[allow(clippy::too_many_arguments)]
    pub fn sharp_scrollbar(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, marker: Option<(f32, f32)>, color: Color) {
        let (_, arrow_h) = crate::vector_menu::scroll_arrow(w);
        let d = crate::vector_menu::scrollbar(w, h, marker.map(|(top, len)| (top - arrow_h, len)));
        self.drawing(p, &d, x, y, color);
    }

    /// The highlight box (box.xml through list_box.xml): an outline at brightness 200
    /// and a fill of alpha 40 [documented: `_line_alpha` 255 of `_line_brightness` 200,
    /// `_fill_alpha` 40].
    pub fn highlight(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, alpha: f32) {
        let fill = amber(128.0).with_alpha((40.0 / 255.0f32).powf(2.2) * alpha);
        p.spawn((at(x, y, w, h), BackgroundColor(fill)));
        let line = amber(200.0).with_alpha(alpha);
        self.outline(p, x, y, w, h, line);
    }

    /// A rectangle's four sides; Sharp: [`box_drawing`].
    pub fn outline(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if self.sharp() {
            self.drawing(p, &box_drawing(w, h), x, y, color);
            return;
        }
        outline(p, x, y, w, h, THICK, color);
    }

    pub fn face(&self, which: Which) -> &Face {
        match which {
            Which::Glow => &self.kit.glow,
            Which::Body => &self.kit.body,
            Which::Title => &self.kit.title,
        }
    }

    /// One line of text whose top is at `top`, placed by `align` about `x`. Its width.
    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, p: &mut ChildSpawnerCommands, which: Which, s: &str, x: f32, top: f32, align: Align, color: Color) -> f32 {
        let face = self.face(which);
        let width = crate::pad::hint_width(face, s, self.glyphs);
        let x = match align {
            Align::Left => x,
            Align::Right => x - width,
            Align::Centre => x - width / 2.0,
        };
        crate::pad::set_hint(p, face, s, x, top + if which == Which::Title { TITLE_BASE } else { BODY_BASE }, color, self.glyphs);
        width
    }

    /// A body text wrapped at `wrap` (and at its own line breaks), its first line's top at
    /// `top`. The number of lines.
    pub fn wrapped(&mut self, p: &mut ChildSpawnerCommands, s: &str, x: f32, top: f32, wrap: f32, color: Color) -> usize {
        let lines = self.kit.body.font.lines(s, Some(wrap));
        for (k, line) in lines.iter().enumerate() {
            set_line(p, &self.kit.body, line, x, top + BODY_BASE + LINE_PITCH * k as f32, color);
        }
        lines.len()
    }

    pub fn width(&self, which: Which, s: &str) -> f32 {
        first_line(&self.face(which).font, s).width
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A face of 31 whose letters are pictures 10 wide, `ascent` over the line and `height`
    /// high, with `before` and `after` room round them.
    fn face(ascent: f32, height: f32, before: f32, after: f32) -> Face {
        let mut font = plain_font(31.0, 12.0);
        for c in b'A'..=b'Z' {
            font.glyphs[usize::from(c)] = fnv_font::Glyph { uv: [0.0, 0.0, 1.0, 1.0], width: 10.0, height, before, after, ascent };
        }
        Face { font, image: None, vector: None }
    }

    #[test]
    fn a_line_is_centred_by_its_ink() {
        // The game font's way: 2 of glow round 22 of ink, the last picture 3 past its advance.
        let raster = face(24.0, 26.0, 0.0, -3.0);
        let line = first_line(&raster.font, "DONE");
        assert_eq!(line.width, 4.0 * 7.0);
        assert_eq!(ink_span(&raster, &line), (0.0, 31.0));
        assert_eq!(ink_centre_x(&raster, &line, 100.0, 1.0), 100.0 - 15.5);
        assert_eq!(ink_centre_x(&raster, &line, 100.0, 0.5), 100.0 - 7.75);
        // Capitals 22 high, glow or not: their middle 11 over the line.
        assert_eq!(cap_base(&raster, 50.0, 1.0), 61.0);
        assert_eq!(cap_mid(&raster, 61.0, 1.0), 50.0);
        let mut sharp = face(22.0, 22.0, 0.0, 0.0);
        assert_eq!(cap_base(&sharp, 50.0, 0.5), 55.5);
        // A sharp line is drawn `shift` right: its ink runs from there.
        sharp.vector = Some(crate::interface::VectorFace { font: Handle::default(), em: 31.0, weight: 400, base: 24.0, caps: false, shift: 3.0 });
        let line = first_line(&sharp.font, "DONE");
        assert_eq!(ink_span(&sharp, &line), (3.0, 43.0));
        // Nothing drawn (the figures have no pictures here): the advances.
        assert_eq!(ink_span(&raster, &first_line(&raster.font, "12")), (0.0, 24.0));
    }

    /// A drawing's lines as rectangles.
    pub(crate) fn rects(d: &Drawing) -> Vec<Rect> {
        d.parts
            .iter()
            .map(|p| match &p.form {
                crate::vector_hud::Form::Rect(r) => *r,
                _ => panic!("a line is a rectangle"),
            })
            .collect()
    }

    /// Whether two rectangles touch or overlap (a joined corner).
    pub(crate) fn meet(a: Rect, b: Rect) -> bool {
        a.min.x <= b.max.x && b.min.x <= a.max.x && a.min.y <= b.max.y && b.min.y <= a.max.y
    }

    #[test]
    fn the_sharp_brackets_corners_meet_and_their_lines_are_the_kits() {
        use crate::vector_menu::LINE;
        let thick = |r: &Rect| (r.width().min(r.height()) - LINE).abs() < 1e-4;
        // Bottom bracket: the foot along the box's bottom, the sides from it up.
        let b = rects(&bottom_bracket_drawing(1024.0));
        assert_eq!(b[0], Rect::new(0.0, CORNER - LINE, 1024.0, CORNER));
        assert!(b.iter().all(thick) && meet(b[0], b[1]) && meet(b[0], b[2]));
        assert_eq!((b[1].min.x, b[2].max.x), (0.0, 1024.0));
        // Top bracket: the sides as long as asked.
        let t = rects(&top_bracket_drawing(600.0, 50.0));
        assert!(t.iter().all(thick) && meet(t[0], t[1]) && meet(t[0], t[2]));
        assert_eq!((t[1].height(), t[2].max.x), (50.0, 600.0));
        // A box: four sides, each meeting the next.
        let o = rects(&box_drawing(100.0, 48.0));
        assert_eq!(o.len(), 4);
        assert!(o.iter().all(thick) && meet(o[0], o[2]) && meet(o[0], o[3]) && meet(o[1], o[2]) && meet(o[1], o[3]));
    }

    #[test]
    fn a_sharp_headline_leaves_the_titles_room_and_joins_its_corners() {
        let h = rects(&headline_drawing(1024.0, (50.0, 300.0), 60.0));
        assert_eq!(h.len(), 4);
        assert_eq!((h[0].min.x, h[0].max.x), (0.0, 50.0));
        assert_eq!((h[3].min.x, h[3].max.x), (300.0, 1024.0));
        assert!(meet(h[0], h[1]) && meet(h[3], h[2]));
        assert_eq!((h[1].height(), h[2].height()), (60.0, 60.0));
        // A title past the right end: no line after it.
        assert_eq!(headline_drawing(1024.0, (50.0, 1100.0), 60.0).parts.len(), 3);
        // The information lines: each across the whole width, meeting its side.
        let c = rects(&corner_lines_drawing(540.0, &[0.0, 333.0], 60.0));
        assert_eq!(c.len(), 4);
        assert!(meet(c[0], c[1]) && meet(c[2], c[3]));
        assert_eq!((c[0].max.x, c[1].max.x, c[3].min.y), (540.0, 540.0, 333.0));
    }

    #[test]
    fn an_outline_is_one_bordered_box() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        world.commands().entity(root).with_children(|p| outline(p, 10.0, 20.0, 100.0, 48.0, THICK, Color::WHITE));
        world.flush();
        let mut q = world.query::<(&Node, &BorderColor)>();
        let all: Vec<_> = q.iter(&world).collect();
        assert_eq!(all.len(), 1, "one node, not four lines");
        assert_eq!(all[0].0.border, UiRect::all(u(THICK)));
        assert_eq!(*all[0].1, BorderColor::all(Color::WHITE));
    }
}
