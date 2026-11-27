//! The dialogue menu, drawn as the original draws it (docs/spec-notes/dialogue-menu.md).
//!
//! `crate::dialogue` says what the menu holds ([`DialogueMenu`]: the actor's name, the
//! line being said or the choices offered, the selected choice); this module sets it out
//! by the game's own interface file and draws it with the game's fonts and pictures, read
//! from the owner's archives at run time. Lengths are in the interface's units, 960 to
//! the height of the view.
//!
//! - [`layout`] places everything (pure, tested without the game's files).
//! - While the actor speaks: the line, without a frame, on a dark band. While the player
//!   chooses: the choices between two brackets on the band, the selected one framed, a
//!   scroll bar at the left when not all of them fit.
//! - The actor's name stands at the top right throughout.
//! - While a Goodbye choice is offered, "[Tab] Goodbye" stands at the left of the band,
//!   level with the bottom bracket, in the band's own text on a backing in its shade (list 5
//!   item 62 asked for "TAB) Goodbye" at the left of the dialogue box; the words are now in the
//!   bracket style of the tile menus' key hints, proposed to the maintainer in #1517; the
//!   interface file has no such hint [observed: dialog_menu.xml]): Tab picks that choice
//!   (`crate::dialogue`). The settings page's "Goodbye hint" switch ([`GoodbyeHint`])
//!   hides the words and what is made for them (docs/spec-notes/dialogue-menu.md §4);
//!   Tab works as before.
//! - The menu is rebuilt when what it shows changes; the pointer chooses the choice it
//!   is on (`crate::dialogue` reads the clicks).

use crate::dialogue::{ChoiceButton, ListId};
use crate::interface::{amber, at, bc_supported, plain_font, set_line, Face, Picture, Pictures};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use crate::ui_pointer::Button;
use bevy::window::PrimaryWindow;
use fnv_font::{Font, Line};
use nvf_assets::GameData;

// ---- the interface file's numbers [documented: menus\dialog\dialog_menu.xml, §2] ------

/// The dark band's width, centred on the screen.
const BAND_WIDTH: f32 = 1080.0;
/// The line starts this far into the band and wraps this much narrower than it.
const TEXT_IN: f32 = 60.0;
/// The band reaches this far above the line's top (less 5) and is this much taller.
const TEXT_BAND_UP: f32 = 45.0;
const TEXT_BAND_MORE: f32 = 80.0;
/// The list: this far into the band and this much narrower; at least this high.
const LIST_IN: f32 = 20.0;
const LIST_LESS: f32 = 70.0;
const MIN_LIST: f32 = 110.0;
/// The brackets: this far right of the list, the top one this far above it, the bottom
/// one this far below it; their upright ends are this long.
const BRACKET_IN: f32 = 15.0;
const BRACKET_ABOVE: f32 = 20.0;
const BRACKET_BELOW: f32 = 10.0;
const BRACKET_ENDS: f32 = 40.0;
/// The band reaches this far above the top bracket's line and below the bottom one's.
const LIST_BAND_UP: f32 = 30.0;
const LIST_BAND_DOWN: f32 = 30.0;
/// The scroll bar's width at the list's left; the choices start right of it and end
/// this far short of the list's right.
const SCROLL_WIDTH: f32 = 32.0;
const CHOICE_SHORT: f32 = 5.0;
/// A choice's text: this far into its row and down from its top; wrapped this much
/// narrower than the row. The row is the text's height and this much more.
const CHOICE_TEXT: Vec2 = Vec2::new(25.0, 15.0);
const CHOICE_WRAP_LESS: f32 = 40.0;
const CHOICE_PAD: f32 = 20.0;
/// The arrows of the scroll bar.
const ARROW: Vec2 = Vec2::new(32.0, 16.0);
const MARKER_WIDTH: f32 = 6.0;
const MARKER_X: f32 = 12.0;
/// The speaker's name and the Goodbye hint stand this far over the box's top line (their
/// letters' foot), the name from the box's left end, the hint to its right end [design:
/// owner 2026-11-27, "align the NPC name to the start of the box and a small space above
/// it", "align the tab good bye prompt to the top end of the box - same spacing as the NPC
/// name"].
const OVER_BOX: f32 = 12.0;
/// The thickness of every line [documented: menus\globals.xml, on a PC].
const THICK: f32 = 2.0;

// ---- measured on the owner's screenshots [observed, §3] ------------------------------

/// The height of a line of font 6, and how far its letters stand under its top.
const LINE: f32 = 38.0;
const TEXT_BASE: f32 = 23.3;
/// The choices stand this much higher in the list than the file puts them.
const LIFT: f32 = 7.2;
/// The tallest the list grows before it scrolls [hypothesis, §3].
const MAX_LIST: f32 = 290.0;
/// Where the arrows' middles stand from the list's top and bottom, and the length of the
/// marker between them.
const UP_MIDDLE: f32 = 31.0;
const DOWN_MIDDLE: f32 = 35.0;
const TRACK_TOP: f32 = 60.0;
const TRACK_BOTTOM: f32 = 52.0;
const MARKER: f32 = 40.0;
/// Between a check's tag and the line.
pub const TAG_GAP: &str = "  ";

// ---- NVForever's own [design; the place asked for in owner list 5 item 62] -------------------------------------

/// The hint shown while a Goodbye choice is offered. The request (list 5 item 62) was
/// "TAB) Goodbye"; these words are the bracket style of the tile menus' key hints, proposed
/// to the maintainer in #1517.
pub const GOODBYE_HINT: &str = "[Tab] Goodbye";

/// [`GOODBYE_HINT`] for the device in use: with a controller its B, which leaves as Tab
/// does ("[B] Goodbye", `crate::pad::menu_buttons`) [design, #1662].
pub fn goodbye_hint() -> String {
    let at = crate::pad::at_menu(fnv_game::game::menus::DIALOGUE);
    format!("{} Goodbye", crate::pad::hint_bracketed(&[KeyCode::Tab], at))
}
/// The settings page's "Goodbye hint" switch (key `dialogue-goodbye-hint`): on by
/// default [owner, list 5 item 62]; off hides the words, their backing and the narrow screens' strip; a tall list moves back down, Tab still says goodbye.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoodbyeHint(pub bool);

impl Default for GoodbyeHint {
    fn default() -> Self {
        GoodbyeHint(true)
    }
}

/// Whether the Tab hint stands on `view`, for both views: `setting` is the player's switch for
/// it (#1517), and a Goodbye choice has to be offered [owner, list 5 item 62].
pub fn hint_shown(view: &MenuView, setting: bool) -> bool {
    setting && view.goodbye && !view.choices.is_empty()
}

/// A choice as the menu shows it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChoiceRow {
    /// The tag of a skill or perk check and the line.
    pub label: String,
    /// Said before: drawn dim.
    pub darkened: bool,
    /// A check the player falls short of: its frame is red.
    pub short: bool,
}

/// What the menu shows: the actor's name, and the line being said or the choices.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MenuView {
    pub speaker: String,
    pub line: Option<String>,
    pub choices: Vec<ChoiceRow>,
    /// One of the choices is the Goodbye one: Tab says it.
    pub goodbye: bool,
}

/// The menu to draw (set by `crate::dialogue`), its selected choice and the first choice
/// shown ([`window`]).
#[derive(Resource, Default)]
pub struct DialogueMenu {
    pub view: Option<MenuView>,
    pub selected: usize,
    first: usize,
    /// The screen of the conversation this stands for, which its buttons carry.
    pub list: Option<ListId>,
}

// ---- layout --------------------------------------------------------------------------

/// A line of text: where it starts and the line its letters stand on.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub line: Line,
    pub x: f32,
    pub base: f32,
}

/// A rectangle: left, top, width, height.
pub type Area = [f32; 4];

#[derive(Debug, Clone, PartialEq)]
pub struct ChoicePlace {
    pub index: usize,
    pub area: Area,
    pub lines: Vec<Placed>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScrollBar {
    pub up: Area,
    pub down: Area,
    pub marker: Area,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListPlace {
    pub area: Area,
    /// The left end, width and height of the brackets' lines.
    pub top_line: f32,
    pub bottom_line: f32,
    pub bracket_x: f32,
    pub bracket_width: f32,
    pub choices: Vec<ChoicePlace>,
    pub scroll: Option<ScrollBar>,
}

/// The menu set out on a screen `width` units wide (960 high).
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub name: Option<Placed>,
    pub band: Option<Area>,
    pub text: Vec<Placed>,
    pub list: Option<ListPlace>,
    /// "[Tab] Goodbye", while a Goodbye choice is offered and the switch is on.
    pub hint: Option<Placed>,
    /// Its backing in the band's shade, when it stands at the band's side.
    pub hint_back: Option<Area>,
    /// The first choice shown.
    pub first: usize,
}

/// The height of a text of `lines` lines in font 6.
fn text_height(lines: usize) -> f32 {
    LINE * lines as f32
}

/// Where a block `height` high starts: its middle where the Subtitle Position row puts the
/// subtitles (`crate::interface::subtitle_top`, a part of the view's height up from its foot),
/// the room `extra` under it (the Goodbye hint's) and all of it on the view.
fn centred(height: f32, extra: f32) -> f32 {
    centred_at(crate::interface::subtitle_place(), height, extra, crate::interface::menu_scale())
}

/// The band's reach past the block it holds, above and below (the list's: its brackets and
/// the band past them; the line's band reaches less).
const BAND_ABOVE: f32 = BRACKET_ABOVE + LIST_BAND_UP;
const BAND_BELOW: f32 = BRACKET_BELOW + LIST_BAND_DOWN;

/// [`centred`] at the Subtitle Position row's `place` and the menus' `scale`: the band and
/// the room `extra` under it all on the view.
fn centred_at(place: f32, height: f32, extra: f32, scale: f32) -> f32 {
    let anchor = crate::interface::BOTTOM_MIDDLE.y;
    // The highest and lowest it may stand: the band's top at the view's top, its foot with
    // the room under it at the view's foot.
    let highest = crate::interface::subtitle_top_at(1.0, height, anchor, scale) + BAND_ABOVE;
    let lowest = crate::interface::subtitle_top_at(0.0, height + extra + BAND_BELOW, anchor, scale);
    crate::interface::subtitle_top_at(place, height, anchor, scale).min(lowest).max(highest)
}

/// One past the last of the whole choices from `from` that fit in `room`, at least one.
fn fitting(from: usize, heights: &[f32], room: f32) -> usize {
    let mut sum = 0.0;
    let mut end = from;
    while end < heights.len() && (end == from || sum + heights[end] <= room) {
        sum += heights[end];
        end += 1;
    }
    end
}

/// The room the list gives its choices: the whole choices that fit from the top in
/// [`MAX_LIST`], at least one. The same at every scroll of the list, so the band keeps its
/// height and place as the selection goes down it; the room a later window leaves under
/// its last whole choice stays empty [owner's playtest 2026-11-05: the band grew and moved
/// when the selection scrolled onto a wrapped choice, as the choices from the new first one
/// packed differently; design: whole choices only, no choice cut at the foot].
pub fn list_room(heights: &[f32]) -> f32 {
    heights[..fitting(0, heights, MAX_LIST)].iter().sum()
}

/// The first choice shown so that `selected` is among the shown ones, moving as little
/// as possible: as many whole choices as fit the list's room ([`list_room`]), at least one.
pub fn window(first: usize, selected: usize, heights: &[f32]) -> (usize, usize) {
    let n = heights.len();
    if n == 0 {
        return (0, 0);
    }
    let selected = selected.min(n - 1);
    let room = list_room(heights);
    let fits = |from: usize| fitting(from, heights, room);
    let mut first = first.min(selected);
    while fits(first) <= selected {
        first += 1;
    }
    // Show as much as fits: no room left empty under the last choice when earlier ones
    // could be shown.
    while first > 0 && fits(first - 1) == n && fits(first) == n {
        first -= 1;
    }
    (first, fits(first))
}

const LIST_WIDTH: f32 = BAND_WIDTH - LIST_LESS;
const CHOICE_WIDTH: f32 = LIST_WIDTH - SCROLL_WIDTH - CHOICE_SHORT;

/// Each choice's lines.
fn choice_lines(view: &MenuView, body: &Font) -> Vec<Vec<Line>> {
    view.choices.iter().map(|c| body.lines(c.label.trim(), Some(CHOICE_WIDTH - CHOICE_WRAP_LESS))).collect()
}

/// Each choice's row height: its text's and the room under it.
fn choice_heights(lines: &[Vec<Line>]) -> Vec<f32> {
    lines.iter().map(|l| text_height(l.len().max(1)) + CHOICE_PAD).collect()
}

/// `hint`: whether the "Goodbye hint" switch is on.
#[allow(clippy::too_many_arguments)]
pub fn layout(view: &MenuView, first: usize, selected: usize, width: f32, hint: bool, body: &Font, name_font: &Font, glyphs: &crate::pad::PadGlyphs) -> Layout {
    let band_x = (width - BAND_WIDTH) / 2.0;
    // The name over the box's (or the line's) left end ([`OVER_BOX`]), its letters' start on it.
    let name_line = Some(view.speaker.trim()).filter(|s| !s.is_empty()).map(|s| name_font.lines(s, None).remove(0));
    let name_at = |line: Line, left: f32, top: f32| Placed { x: left - lead(&line, name_font), base: top - OVER_BOX, line };
    let mut out = Layout { name: None, band: None, text: Vec::new(), list: None, hint: None, hint_back: None, first: 0 };
    if view.choices.is_empty() {
        let Some(text) = view.line.as_deref().map(str::trim).filter(|t| !t.is_empty()) else { return out };
        let lines = body.lines(text, Some(BAND_WIDTH - 2.0 * TEXT_IN));
        let height = text_height(lines.len());
        let top = centred(height, 0.0);
        out.band = Some([band_x, top - TEXT_BAND_UP, BAND_WIDTH, height + TEXT_BAND_MORE]);
        // Each line centred on the band, as the HUD's subtitles are [design: owner 2026-11-27,
        // "make sure subtitles are always centered"]; the name over the first line's capitals
        // from the widest line's start.
        let middle = band_x + BAND_WIDTH / 2.0;
        out.text = lines.into_iter().enumerate().map(|(i, line)| Placed { x: middle - line.width / 2.0, base: top + TEXT_BASE + LINE * i as f32, line }).collect();
        let left = out.text.iter().map(|t| t.x).fold(f32::MAX, f32::min);
        let cap_top = out.text.first().map_or(top, |t| t.base - rise(body));
        out.name = name_line.map(|l| name_at(l, left, cap_top));
        return out;
    }

    let list_x = band_x + LIST_IN;
    let choice_x = list_x + SCROLL_WIDTH;
    let lines = choice_lines(view, body);
    let heights = choice_heights(&lines);
    let (first, end) = window(first, selected, &heights);
    let height = list_room(&heights).max(MIN_LIST);
    // The hint's words; they stand over the box (no room under it).
    let hint_text = hint_shown(view, hint).then(goodbye_hint);
    let hint_line = hint_text.as_deref().map(|t| body.lines(t, None).remove(0));
    let words = hint_text.as_deref().map_or(0.0, |t| crate::pad::hint_width_font(body, t, glyphs));
    let room = 0.0;
    let top = centred(height, room);
    let mut y = top - LIFT;
    let mut choices = Vec::new();
    for (i, ls) in lines.iter().enumerate().take(end).skip(first) {
        let placed = ls
            .iter()
            .enumerate()
            .map(|(j, l)| Placed { line: l.clone(), x: choice_x + CHOICE_TEXT.x, base: y + CHOICE_TEXT.y + TEXT_BASE + LINE * j as f32 })
            .collect();
        choices.push(ChoicePlace { index: i, area: [choice_x, y, CHOICE_WIDTH, heights[i]], lines: placed });
        y += heights[i];
    }
    let bottom = top + height;
    let scroll = (end - first < view.choices.len()).then(|| {
        let (track_top, track_bottom) = (top + TRACK_TOP, bottom - TRACK_BOTTOM);
        let hidden = view.choices.len() - (end - first);
        let along = (track_bottom - track_top - MARKER).max(0.0) * first as f32 / hidden as f32;
        ScrollBar {
            up: [list_x, top + UP_MIDDLE - ARROW.y / 2.0, ARROW.x, ARROW.y],
            down: [list_x, bottom - DOWN_MIDDLE - ARROW.y / 2.0, ARROW.x, ARROW.y],
            marker: [list_x + MARKER_X, track_top + along, MARKER_WIDTH, MARKER],
        }
    });
    let top_line = top - BRACKET_ABOVE;
    let bottom_line = bottom + BRACKET_BELOW;
    out.band = Some([band_x, top_line - LIST_BAND_UP, BAND_WIDTH, bottom_line - top_line + LIST_BAND_UP + LIST_BAND_DOWN + room]);
    out.list = Some(ListPlace { area: [list_x, top, LIST_WIDTH, height], top_line, bottom_line, bracket_x: list_x + BRACKET_IN, bracket_width: LIST_WIDTH, choices, scroll });
    out.first = first;
    let (box_x, box_w) = (list_x + BRACKET_IN, LIST_WIDTH);
    out.name = name_line.map(|l| name_at(l, box_x, top_line));
    // The hint over the box's right end, on the name's line.
    if let Some(line) = hint_line {
        out.hint = Some(Placed { x: box_x + box_w - words, base: top_line - OVER_BOX, line });
    }
    out
}

/// How far a line's first picture starts after its place (the game font's room before it);
/// nothing for a font without pictures.
fn lead(line: &Line, font: &Font) -> f32 {
    line.glyphs.first().map_or(0.0, |p| font.bitmap_glyph(p).before)
}

/// How far a font's capitals rise over their line: its 'H', else three fifths of its size.
fn rise(font: &Font) -> f32 {
    match font.glyph(b'H').ascent {
        a if a > 0.0 => a,
        _ => font.size * 0.6,
    }
}

// ---- the game's fonts and pictures ---------------------------------------------------

/// Font 6 (the line and the choices) and font 7 (the name) [documented: the interface
/// file names them by number; Fallout_default.ini `sFontFile_6`, `_7`].
const BODY_FONT: &str = "textures\\fonts\\glow_monofonto_vl_dialogs.fnt";
const NAME_FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";

#[derive(Clone)]
pub(crate) struct Kit {
    pub(crate) body: Face,
    pub(crate) name: Face,
    fade_down: Option<Picture>,
    fade_up: Option<Picture>,
    arrow_up: Option<Picture>,
    arrow_down: Option<Picture>,
    marker: Option<Picture>,
}

pub(crate) fn load_kit(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Kit {
    let mut pictures = Pictures::load(data);
    let mut picture = |path: &str| pictures.picture(path, data, images, bc);
    let fade_down = picture("interface\\shared\\line\\fade_to_bottom.dds");
    let fade_up = picture("interface\\shared\\line\\fade_to_top.dds");
    let arrow_up = picture("interface\\shared\\scrollbar\\arrow_up.dds");
    let arrow_down = picture("interface\\shared\\scrollbar\\arrow_down.dds");
    let marker = picture("interface\\shared\\scrollbar\\vert_marker.dds");
    Kit {
        body: crate::interface::screen_face(BODY_FONT, plain_font(34.0, 17.0), data, images, bc),
        name: crate::interface::screen_face(NAME_FONT, plain_font(36.0, 16.0), data, images, bc),
        fade_down,
        fade_up,
        arrow_up,
        arrow_down,
        marker,
    }
}

// ---- drawing ---------------------------------------------------------------------------

pub struct DialogueMenuPlugin;

impl Plugin for DialogueMenuPlugin {
    fn build(&self, app: &mut App) {
        crate::dialogue_touch::install(app);
        app.init_resource::<DialogueMenu>().init_resource::<GoodbyeHint>().add_systems(Update, draw.in_set(crate::PlaySet::Ui).in_set(DrawMenu));
        crate::settings::add_setting(
            app,
            crate::settings::Setting {
                key: "dialogue-goodbye-hint",
                group: crate::pause::GROUP_MENUS,
                label: "Goodbye hint",
                note: "On: \"[Tab] Goodbye\" stands at the left of the dialogue box while a Goodbye choice is offered. Off: hidden; Tab still says goodbye.",
                switches: "",
                get: |w| w.resource::<GoodbyeHint>().0,
                set: |w, on| w.resource_mut::<GoodbyeHint>().0 = on,
                slider: None,
            },
        );
    }
}

/// The drawing of the menu, for what fills [`DialogueMenu`] to run before.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DrawMenu;

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
pub(crate) struct TouchNativeRoot;

/// The frame of a choice, shown while it is the selected one.
#[derive(Component)]
struct Frame(usize);

/// The line of the red frame of a check the player falls short of [observed: §3].
fn short_line() -> Color {
    Color::srgb_u8(100, 18, 16)
}

fn image(p: Option<&Picture>, color: Color) -> Option<ImageNode> {
    p.map(|(image, rect)| ImageNode { rect: *rect, color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) })
}

#[allow(clippy::too_many_arguments)]
fn spawn_menu(commands: &mut Commands, view: &MenuView, l: &Layout, kit: &Kit, selected: usize, on_screen: Option<&ListId>, glyphs: &crate::pad::PadGlyphs, images: &mut Assets<Image>) {
    spawn_menu_on(commands, view, l, kit, selected, on_screen, glyphs, false, None, None, Some(images));
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_touch_menu(commands: &mut Commands, view: &MenuView, l: &Layout, kit: &Kit, selected: usize, on_screen: Option<&ListId>, glyphs: &crate::pad::PadGlyphs, confirm: Option<(&str,f32)>, camera: Entity) {
    spawn_menu_on(commands, view, l, kit, selected, on_screen, glyphs, true, confirm, Some(camera), None);
}

#[allow(clippy::too_many_arguments)]
fn spawn_menu_on(commands: &mut Commands, view: &MenuView, l: &Layout, kit: &Kit, selected: usize, on_screen: Option<&ListId>, glyphs: &crate::pad::PadGlyphs, touch: bool, confirm: Option<(&str,f32)>, camera: Option<Entity>, mut images: Option<&mut Assets<Image>>) {
    // The lines and marks in vector while the Menu Font row says Sharp (`crate::vector_menu`;
    // the touch screen's own drawing keeps the pictures).
    let vector = crate::vector_menu::on() && images.is_some();
    let screen = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    // At the UI scale the menu keeps to the view's bottom, in its middle (#1631).
    let screen = if touch { screen } else { crate::interface::framed_root(screen, crate::interface::BOTTOM_MIDDLE) };
    let bright = amber(255.0);
    let mut root = commands.spawn((screen, GlobalZIndex(if touch {2400} else {90})));
    if touch {root.insert((TouchNativeRoot,crate::dialogue_touch::PaletteRoot));} else {root.insert(MenuRoot);}
    if let Some(camera)=camera {root.insert(UiTargetCamera(camera));}
    root.with_children(|s| {
        // A backing only behind the choices, inside their brackets' lines, plain and
        // sharp-edged as Quick Loot's at the Box Background row's darkness; none behind the
        // line being said (subtitles), nor the Goodbye hint [design: owner 2026-11-27, "the
        // conversation box goes outside the line limit - fix it and remove the box for the
        // tab goodbye prompt", "remove the box from subtitles - box should be only behind the
        // interaction boxes that have lines around their edges"].
        // (Drawn with the brackets below: in vector baked with their lines, so it ends where
        // they end.)
        // The sharp lines' start shift (`VectorFace::shift`) taken back where words are set by
        // their ink: the name's start on the box's, the hint's end on it, the spoken line centred.
        let shift = |f: &Face| f.vector.as_ref().map_or(0.0, |v| v.shift);
        if let Some(n) = &l.name {
            set_line(s, &kit.name, &n.line, n.x - shift(&kit.name), n.base, bright);
        }
        if let Some(hint) = &l.hint {
            crate::pad::set_hint(s, &kit.body, &goodbye_hint(), hint.x - shift(&kit.body), hint.base, bright, glyphs);
        }
        for t in &l.text {
            set_line(s, &kit.body, &t.line, t.x - shift(&kit.body), t.base, bright);
        }
        if touch && view.choices.is_empty() {
            if let Some([x,y,w,h]) = l.band {
                s.spawn((Button, crate::dialogue_touch::native_skip(on_screen.cloned()), at(x,y,w,h)));
            }
        }
        let Some(list) = &l.list else { return };
        // The brackets: a line with an upright at each end that fades out, down from the
        // top one and up from the bottom one.
        let (bx, bw) = (list.bracket_x, list.bracket_width);
        if let (true, Some(images)) = (vector, images.as_deref_mut()) {
            // One drawing: the two brackets' lines and their uprights, square-ended and
            // joined, with the scroll bar's arrows and marker (`crate::vector_menu`).
            let (x0, y0) = (bx, list.top_line);
            let h = list.bottom_line + THICK - y0;
            let d = crate::vector_menu::Drawing::new(bw, h).bracket_down(0.0, bw, 0.0).bracket_up(0.0, bw, h).fill(0.0, 0.0, bw, h);
            crate::vector_menu::spawn(s, &d, x0, y0, bright, images);
            // The scroll bar a drawing of its own: it stands left of the brackets, outside
            // their drawing's box, which cut its arrows [owner 2026-11-27, "part of the arrow is
            // cut out"].
            if let Some(bar) = &list.scroll {
                let [ux, uy, uw, _] = bar.up;
                let [_, dy, _, dh] = bar.down;
                let [_, my, _, mh] = bar.marker;
                let sb = crate::vector_menu::scrollbar(uw, dy + dh - uy, Some((my - uy - crate::vector_menu::scroll_arrow(uw).1, mh)));
                crate::vector_menu::spawn(s, &sb, ux, uy, bright, images);
            }
        } else {
        crate::vector_menu::backing(s, bx, list.top_line, bw, list.bottom_line + THICK - list.top_line);
        for (y, fade, ends_y) in [(list.top_line, &kit.fade_down, list.top_line), (list.bottom_line, &kit.fade_up, list.bottom_line + THICK - BRACKET_ENDS)] {
            s.spawn((at(bx, y, bw, THICK), BackgroundColor(bright)));
            for x in [bx, bx + bw - THICK] {
                if let Some(i) = image(fade.as_ref(), bright) {
                    s.spawn((at(x, ends_y, THICK, BRACKET_ENDS), i));
                }
            }
        }
        if let Some(bar) = &list.scroll {
            for (area, p) in [(bar.up, &kit.arrow_up), (bar.down, &kit.arrow_down), (bar.marker, &kit.marker)] {
                let [x, y, w, h] = area;
                match image(p.as_ref(), bright) {
                    Some(i) => s.spawn((at(x, y, w, h), i)),
                    None => s.spawn((at(x + w / 2.0 - THICK, y, 2.0 * THICK, h), BackgroundColor(bright))),
                };
            }
        }
        }
        if let Some((caption,minimum)) = confirm {
            let [x,_,w,_] = list.area;
            let y = list.bottom_line + 15.0;
            let height = minimum.max(kit.body.font.size * 1.6);
            s.spawn((Button,crate::dialogue_touch::native_confirm(on_screen.cloned()),
                at(x,y,w,height),BackgroundColor(amber(128.0).with_alpha(0.1))))
                .with_children(|p| {
                    for (index,line) in kit.body.font.lines(caption,Some(w-40.0)).iter().enumerate() {
                        set_line(p,&kit.body,line,20.0,crate::interface::line_base(&kit.body)
                            + index as f32 * kit.body.font.size * 1.2,bright);
                    }
                });
        }
        // The choices, cut at the brackets' lines.
        let clip_top = list.top_line + THICK;
        let mut window = at(0.0, clip_top, 0.0, list.bottom_line - clip_top);
        (window.width, window.overflow) = (Val::Percent(100.0),
            if touch { Overflow::scroll_y() } else { Overflow::clip() });
        let mut clip = s.spawn(window);
        if touch {clip.insert(crate::dialogue_touch::native_viewport());}
        clip.with_children(|w| {
            for c in &list.choices {
                let [x, y, cw, ch] = c.area;
                let row = &view.choices[c.index];
                let mut choice = w.spawn((Button, ChoiceButton(c.index, on_screen.cloned()), at(x, y - clip_top, cw, ch)));
                if touch {choice.insert(crate::dialogue_touch::native_choice(c.index,on_screen.cloned(),row.short));}
                choice.with_children(|r| {
                    // [documented: the list's highlight box] One unit short of the row; a
                    // faint fill and a line around it, red for a check the player falls
                    // short of [observed].
                    let h = ch - 1.0;
                    let (line, fill) = if row.short { (short_line(), Color::srgba_u8(62, 0, 0, 82)) } else { (amber(200.0), amber(128.0).with_alpha(0.1)) };
                    let shown = if c.index == selected { Visibility::Inherited } else { Visibility::Hidden };
                    // Its lines whole pixels thick, each from its own edge, so all four match
                    // (`crate::message_menu::box_lines`); in vector with their glow while Sharp.
                    let mut frame = r.spawn((Frame(c.index), at(0.0, 0.0, cw, h), shown, BackgroundColor(fill)));
                    match (vector, images.as_deref_mut()) {
                        (true, Some(images)) => {
                            let d = crate::vector_menu::Drawing::new(cw, h).frame(0.0, 0.0, cw, h);
                            frame.with_children(|f| {
                                crate::vector_menu::spawn(f, &d, 0.0, 0.0, line, images);
                            });
                        }
                        _ => {
                            frame.with_children(|f| crate::message_menu::box_lines(f, line));
                        }
                    }
                    // Said before: as bright as the interface's disabled text (128 of 255).
                    let color = if row.darkened { amber(128.0) } else { bright };
                    for t in &c.lines {
                        set_line(r, &kit.body, &t.line, t.x - x, t.base - y, color);
                    }
                });
            }
        });
    });
}


/// What the drawn menu was made from: the menu's view, the first choice shown, the width
/// in units, the "Goodbye hint" switch, the button labels and the current font revision.
type MenuKey = (MenuView, usize, u32, bool, crate::pad::Labels, u32, [u32; 3]);

fn menu_key(view: Option<&MenuView>, creating: bool, first: usize, width: f32, hint: bool) -> Option<MenuKey> {
    // The letters (the Menu Font row), the box's darkness and the subtitles' place too.
    let look = [crate::vector_font::letters(), (crate::interface::box_backing() * 100.0).round() as u32, (crate::interface::subtitle_place() * 1000.0).round() as u32];
    view.filter(|_| !creating).map(|v| (v.clone(), first, width.round() as u32, hint, crate::pad::labels(), crate::cjk_font::revision(), look))
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    mut menu: ResMut<DialogueMenu>,
    data: Option<Res<GameData>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    roots: Query<Entity, With<MenuRoot>>,
    mut frames: Query<(&Frame, &mut Visibility)>,
    mut kit: Local<crate::interface::Kept<Kit>>,
    mut shown: Local<Option<MenuKey>>,
    creator: Option<Res<crate::creator::CreatorMenu>>,
    tiles: Option<Res<crate::tiles::dialog_menu::DialogTiles>>,
    mut buttons: Query<&mut ChoiceButton>,
    hint: Res<GoodbyeHint>,
    glyphs: Res<crate::pad::PadGlyphs>,
    touch: Res<crate::dialogue_touch::TouchDialogue>,
) {
    if touch.enabled {
        for e in &roots { commands.entity(e).despawn(); }
        *shown = None;
        return;
    }
    // Drawn from its file: all of the menu, none of it here (`crate::tiles::dialog_menu`).
    if tiles.is_some_and(|t| t.by_tiles()) {
        for e in &roots {
            commands.entity(e).despawn();
        }
        *shown = None;
        return;
    }
    let width = windows.single().map_or(960.0 * 16.0 / 9.0, |w| 960.0 * w.width() / w.height().max(1.0));
    let selected = menu.selected;
    let kit = kit.get_or_make(|| load_kit(data.as_deref(), &mut images, bc_supported(formats.as_deref())));
    // Where the list starts follows the selection: the menu is rebuilt when it scrolls.
    let first = match &menu.view {
        Some(v) if !v.choices.is_empty() => window(menu.first, selected, &choice_heights(&choice_lines(v, &kit.body.font))).0,
        _ => 0,
    };
    if menu.first != first {
        menu.first = first;
    }
    // Nothing over the character creator: the opening opens it at the end of a line of
    // Doc Mitchell's, still in the conversation [owner 2026-10-17].
    let creating = creator.is_some_and(|c| c.is_open());
    let key = menu_key(menu.view.as_ref(), creating, first, width, hint.0);
    if key != *shown {
        for e in &roots {
            commands.entity(e).despawn();
        }
        if let Some((view, first, _, hint_on, _, _, _)) = &key {
            let l = layout(view, *first, selected, width, *hint_on, &kit.body.font, &kit.name.font, &glyphs);
            // One line when the hint appears, or when the switch holds it back.
            let was = shown.as_ref().and_then(|s| s.0.goodbye.then_some(s.3));
            match (view.goodbye.then_some(*hint_on), was) {
                (Some(true), was) if was != Some(true) => info!("dialogue menu: {} shown at the left of the choices", goodbye_hint()),
                (Some(false), was) if was != Some(false) => info!("dialogue menu: {} hidden by the Goodbye hint setting", goodbye_hint()),
                _ => {}
            }
            spawn_menu(&mut commands, view, &l, kit, selected, menu.list.as_ref(), &glyphs, &mut images);
        }
        *shown = key;
        return;
    }
    for (frame, mut v) in &mut frames {
        v.set_if_neq(if frame.0 == selected { Visibility::Inherited } else { Visibility::Hidden });
    }
    // The same words for the next screen (no rebuild): what stands is that screen's now.
    for mut b in &mut buttons {
        if b.1 != menu.list {
            b.1.clone_from(&menu.list);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character 17 on, spaces 14; sizes as the game's fonts 6 and 7.
    fn font(size: f32) -> Font {
        let mut f = plain_font(size, 17.0);
        f.glyphs[b' ' as usize].after = 14.0;
        f
    }

    fn choices(labels: &[&str]) -> MenuView {
        MenuView { speaker: "Trudy".into(), line: None, choices: labels.iter().map(|l| ChoiceRow { label: l.to_string(), ..default() }).collect(), goodbye: false }
    }

    fn lay(v: &MenuView, width: f32, hint: bool) -> Layout {
        layout(v, 0, 0, width, hint, &font(34.0), &font(36.0), &no_glyphs())
    }

    /// No glyph pictures: the test's hints name no PlayStation button, so the width is the
    /// text's.
    fn no_glyphs() -> crate::pad::PadGlyphs {
        crate::pad::PadGlyphs::default()
    }

    fn goodbye(labels: &[&str]) -> MenuView {
        MenuView { goodbye: true, ..choices(labels) }
    }

    #[test]
    fn the_name_and_the_hint_stand_over_the_box_at_its_ends() {
        let v = goodbye(&["What happened here?", "Goodbye."]);
        for width in [1280.0, 1707.0] {
            let l = lay(&v, width, true);
            let list = l.list.as_ref().unwrap();
            let (name, hint) = (l.name.as_ref().unwrap(), l.hint.as_ref().unwrap());
            // Both OVER_BOX over the top line, the name from the box's left end, the hint's
            // words ending on its right end; no backing for the hint.
            assert_eq!((name.base, hint.base), (list.top_line - OVER_BOX, list.top_line - OVER_BOX));
            assert_eq!(name.x, list.bracket_x);
            let words = crate::pad::hint_width_font(&font(34.0), &goodbye_hint(), &no_glyphs());
            assert!((hint.x + words - (list.bracket_x + list.bracket_width)).abs() < 1e-3);
            assert!(l.hint_back.is_none());
            // The switch off: the words go, nothing else moves.
            let off = lay(&v, width, false);
            assert!(off.hint.is_none());
            assert_eq!((off.list.as_ref(), off.name.as_ref()), (l.list.as_ref(), l.name.as_ref()));
        }
    }

    #[test]
    fn the_spoken_line_is_centred_with_the_name_over_it() {
        let text = format!("{} {}", "a".repeat(55), "b".repeat(5));
        let v = MenuView { speaker: "Bruce Isaac".into(), line: Some(text), choices: Vec::new(), goodbye: false };
        let width = 960.0 * 16.0 / 9.0;
        let l = layout(&v, 0, 0, width, true, &font(34.0), &font(36.0), &no_glyphs());
        let middle = width / 2.0;
        for t in &l.text {
            assert!((t.x + t.line.width / 2.0 - middle).abs() < 1e-3, "every line centred");
        }
        let name = l.name.unwrap();
        let left = l.text.iter().map(|t| t.x).fold(f32::MAX, f32::min);
        assert_eq!(name.x, left);
        assert!(name.base < l.text[0].base - rise(&font(34.0)), "over the first line's capitals");
    }

    #[test]
    fn a_flip_of_the_switch_rebuilds_the_menu() {
        let mut v = choices(&["Hello.", "Goodbye."]);
        v.goodbye = true;
        let on = menu_key(Some(&v), false, 0, 1707.0, true);
        assert_eq!(on, menu_key(Some(&v), false, 0, 1707.2, true));
        assert_ne!(on, menu_key(Some(&v), false, 0, 1707.0, false), "the drawing's cache sees the switch");
        assert_eq!(menu_key(Some(&v), true, 0, 1707.0, true), None, "nothing over the character creator");
    }

    #[test]
    fn the_goodbye_hint_is_a_setting_on_by_default() {
        let mut app = App::new();
        app.add_plugins(DialogueMenuPlugin);
        let world = app.world_mut();
        assert!(world.resource::<GoodbyeHint>().0);
        let settings = world.resource::<crate::settings::Settings>();
        let s = settings.list()[settings.index("dialogue-goodbye-hint").expect("listed")];
        assert_eq!((s.group, s.label), (crate::pause::GROUP_MENUS, "Goodbye hint"));
        assert!((s.get)(world));
        (s.set)(world, false);
        assert!(!world.resource::<GoodbyeHint>().0 && !(s.get)(world));
    }

    #[test]
    fn the_subtitle_position_puts_the_block_anywhere_on_the_view() {
        use crate::interface::subtitle_top_at as top;
        // At 100%: a quarter up puts the middle 720 down; the middle at 480; the ends keep 15
        // from the view's edges.
        assert_eq!(top(0.25, 76.0, 1.0, 1.0), 720.0 - 38.0);
        assert_eq!(top(0.5, 76.0, 1.0, 1.0), 480.0 - 38.0);
        assert_eq!(top(0.0, 76.0, 1.0, 1.0), 960.0 - 15.0 - 76.0);
        assert_eq!(top(1.0, 76.0, 1.0, 1.0), 15.0);
        // At 50%, the menu's frame is the view's lower half: the view's middle is its top, and
        // its top half above it.
        assert_eq!(top(0.5, 76.0, 1.0, 0.5), -38.0);
        assert_eq!(top(1.0, 76.0, 1.0, 0.5), -960.0 + 15.0);
    }

    #[test]
    fn the_line_is_centred_on_its_band() {
        // Two lines on a 16:9 screen, as the owner's screenshot of Bruce Isaac.
        let text = format!("{} {}", "a".repeat(55), "b".repeat(5));
        let v = MenuView { speaker: "Bruce Isaac".into(), line: Some(text), choices: Vec::new(), goodbye: false };
        let width = 960.0 * 16.0 / 9.0;
        let l = layout(&v, 0, 0, width, true, &font(34.0), &font(36.0), &no_glyphs());
        assert_eq!(l.text.len(), 2);
        // The text's top at 730 (its middle a fifth up the view, the Subtitle Position row's
        // default), its first line standing 23.3 under it, the next 38 lower.
        assert!((l.text[0].base - 753.3).abs() < 1e-3);
        assert!((l.text[1].base - 791.3).abs() < 1e-3);
        // The band 1080 wide and centred; each line centred on it.
        let band = l.band.unwrap();
        assert!((band[0] - (width - 1080.0) / 2.0).abs() < 1e-3);
        assert!((l.text[0].x + l.text[0].line.width / 2.0 - width / 2.0).abs() < 1e-3);
        assert_eq!((band[1], band[3]), (685.0, 156.0));
        assert!(l.list.is_none());
    }

    #[test]
    fn the_choices_stand_between_the_brackets() {
        // Three choices on a 4:3 screen, two of two lines and one of one, as the owner's
        // screenshot of Trudy; a fourth that does not fit.
        let long = "abcde ".repeat(12);
        let v = choices(&[&long, &long, "Do you know the robot?", &long]);
        let l = layout(&v, 0, 1, 1280.0, true, &font(34.0), &font(36.0), &no_glyphs());
        let list = l.list.unwrap();
        assert_eq!(list.choices.len(), 3);
        // The list is 250 high, its top at 643 (its middle a fifth up the view, the Subtitle
        // Position row's default) and the brackets' lines 20 above and 10 below it.
        assert_eq!(list.area, [120.0, 643.0, 1010.0, 250.0]);
        assert_eq!((list.top_line, list.bottom_line, list.bracket_x), (623.0, 903.0, 135.0));
        // The rows: 96, 96 and 58 high, starting 7.2 above the list's top, from the right
        // of the scroll bar.
        let tops: Vec<f32> = list.choices.iter().map(|c| c.area[1]).collect();
        assert!((tops[0] - 635.8).abs() < 1e-3 && (tops[1] - 731.8).abs() < 1e-3 && (tops[2] - 827.8).abs() < 1e-3);
        assert_eq!(list.choices[0].area[0], 152.0);
        assert_eq!(list.choices[0].area[2], 973.0);
        let first = &list.choices[0].lines;
        assert!((first[0].base - (635.8 + 15.0 + 23.3)).abs() < 1e-3);
        assert_eq!(first[0].x, 177.0);
        // The scroll bar shows because the fourth choice does not fit.
        let bar = list.scroll.unwrap();
        assert_eq!(bar.up[0], 120.0);
        assert!((bar.up[1] + 8.0 - 674.0).abs() < 1e-3);
        // The band: 30 above and below the brackets.
        assert_eq!(l.band.unwrap(), [100.0, 593.0, 1080.0, 340.0]);
    }

    #[test]
    fn a_short_list_keeps_its_least_height() {
        let l = layout(&choices(&["Goodbye."]), 0, 0, 1280.0, true, &font(34.0), &font(36.0), &no_glyphs());
        let list = l.list.unwrap();
        assert_eq!(list.area[3], 110.0);
        assert!(list.scroll.is_none());
    }

    #[test]
    fn the_window_follows_the_selection() {
        let h = [58.0; 8];
        // Five rows of 58 fit in 290.
        assert_eq!(window(0, 0, &h), (0, 5));
        assert_eq!(window(0, 4, &h), (0, 5));
        assert_eq!(window(0, 5, &h), (1, 6));
        assert_eq!(window(3, 7, &h), (3, 8));
        // Going up moves the window only when the selection leaves it.
        assert_eq!(window(3, 4, &h), (3, 8));
        assert_eq!(window(3, 2, &h), (2, 7));
        // A window that could show more above when the list got shorter.
        assert_eq!(window(6, 7, &h[..7]), (2, 7));
        // One very long choice is shown alone.
        assert_eq!(window(0, 0, &[400.0, 58.0]), (0, 1));
        assert_eq!(window(0, 0, &[]), (0, 0));
    }

    #[test]
    fn the_list_keeps_its_height_as_the_selection_scrolls() {
        // Sunny Smiles's six choices at 1280 x 720: two of two lines among them. The room
        // is the first three (250); from the second choice only three fit it again, not the
        // four that 290 would take, so the band neither grows nor moves.
        let h = [96.0, 58.0, 96.0, 58.0, 58.0, 58.0];
        assert_eq!(list_room(&h), 250.0);
        assert_eq!(window(0, 2, &h), (0, 3));
        assert_eq!(window(0, 3, &h), (1, 4));
        assert_eq!(window(1, 4, &h), (2, 5));
        assert_eq!(window(2, 5, &h), (3, 6));
        // Back up: the window moves only when the selection leaves it.
        assert_eq!(window(3, 3, &h), (3, 6));
        assert_eq!(window(3, 2, &h), (2, 5));
        // In the layout the list and the band are the same at both scrolls.
        let two = "abcde ".repeat(12);
        let v = choices(&[&two, "Do you know the robot?", &two, "Short.", "Short.", "Goodbye."]);
        let a = layout(&v, 0, 1, 1280.0, true, &font(34.0), &font(36.0), &no_glyphs());
        let b = layout(&v, 0, 3, 1280.0, true, &font(34.0), &font(36.0), &no_glyphs());
        let (la, lb) = (a.list.unwrap(), b.list.unwrap());
        assert_eq!(la.area, lb.area);
        assert_eq!(a.band, b.band);
        assert_eq!((la.choices.len(), lb.choices.len()), (3, 3));
        assert_eq!((la.choices[0].index, lb.choices[0].index), (0, 1));
        // The room under the last whole choice stays empty: the third choice shown from the
        // second ends 38 above the list's foot.
        let last = &lb.choices[2];
        assert!((la.area[1] + la.area[3]) - (last.area[1] + last.area[3]) > 30.0);
    }
}
