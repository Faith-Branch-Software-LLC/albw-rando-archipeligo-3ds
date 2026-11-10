//! Draws the view that `ui_core::menu` produces.

use ui_core::display::Display;
use ui_core::menu::{
    pill_spans, step_lefts, Layout, Pills, View, PILL_HEIGHT, PILL_TOP, PILL_WIDTH, ROW_HEIGHT, ROW_LEFT, ROW_TOP,
    ROW_WIDTH, STEP_WIDTH, VALUE_CENTER, VALUE_LEFT,
};

use std::ffi::c_void;

use crate::art::Skin;
use crate::draw::{self, Font, CENTER, LEFT, RIGHT};
use crate::icons::Icons;
use crate::model_view::Preview;

/// Text scales for the sizes picked on the Display page. citro2d draws any font with 30 pixel glyph
/// cells at scale 1. Rows, hints and the top screen text share one size so the screens look even.
#[derive(Clone, Copy)]
struct Sizes {
    text: f32,
    title: f32,
}

impl Sizes {
    fn new(display: Display) -> Self {
        Self { text: f32::from(display.text) / 30.0, title: f32::from(display.title) / 30.0 }
    }

    /// The smallest the top screen text gets when a page has a lot to say.
    fn body_smallest(self) -> f32 {
        self.text * 0.85
    }
}

/// The icon column's width. Every row on a page moves over if any row on it shows an icon.
const ICON_SPACE: f32 = 29.0;
const ICON_SIZE: f32 = 26.0;

/// How many pixels at each end of row art keep their size when the art stretches.
const PLATE_BORDER: f32 = 8.0;
/// Pills are plain colors with rounded corners.
const PILL_RADIUS: f32 = 5.0;

/// The three row shapes, each with its own art.
#[derive(Clone, Copy)]
enum Plate {
    Bar,
    Card,
    Line,
}

/// Draws a row's background: its art, or plain shapes without art. The focused row glows.
fn plate(skin: &Skin, kind: Plate, focused: bool, y: f32, height: f32) {
    if focused {
        draw::glow(ROW_LEFT, y, ROW_WIDTH, height, draw::GOLD);
    }
    let (own, own_focused) = match kind {
        Plate::Bar => (skin.bar, skin.bar_focused),
        Plate::Card => (skin.card, skin.card_focused),
        Plate::Line => (skin.line, skin.line_focused),
    };
    // Shapes without their own art stretch the row bar.
    let art = if focused { own_focused.or(skin.bar_focused) } else { own.or(skin.bar) };
    match art {
        Some(art) => art.draw(ROW_LEFT, y, ROW_WIDTH, height, PLATE_BORDER),
        None => {
            draw::rect(ROW_LEFT, y, ROW_WIDTH, height, if focused { draw::CARD_ACTIVE } else { draw::CARD });
            if focused {
                draw::rect(ROW_LEFT, y, 3.0, height, draw::GOLD);
            }
        }
    }
}

/// A pill's background: gold when picked, dark otherwise.
fn pill_plate(picked: bool, left: f32, top: f32, width: f32) {
    draw::rounded_rect(left, top, width, PILL_HEIGHT, PILL_RADIUS, if picked { draw::GOLD } else { draw::WELL });
}

/// The rows with their places on the page, the focused one last so its glow lies over its
/// neighbors.
fn in_draw_order(view: &View) -> impl Iterator<Item = (usize, &ui_core::menu::RowView)> {
    let rows = view.rows.iter().enumerate();
    rows.clone().filter(|(_, row)| !row.focused).chain(rows.filter(|(_, row)| row.focused))
}

/// The dark box in the top screen art that holds the text.
const BOX_LEFT: f32 = 38.0;
const BOX_RIGHT: f32 = 362.0;
const BOX_TOP: f32 = 114.0;
const BOX_BOTTOM: f32 = 228.0;
/// The page name sits further in than the text below it.
const TITLE_LEFT: f32 = 56.0;

/// Writes a line to the run log the first time it comes up, so a per-frame check logs only once.
fn note_once(line: &str) {
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let Ok(mut seen) = SEEN.lock() else { return };
    if !seen.iter().any(|old| old == line) {
        seen.push(line.to_owned());
        crate::job::log(line);
    }
}

/// `top_art`, if loaded, replaces the plain top screen.
/// `preview`, if given, is a model and its turn in radians, drawn below the rows.
pub fn draw(view: &View, icons: &Icons, top_art: Option<*mut c_void>, skin: &Skin, display: Display, preview: Option<(&Preview, f32)>) {
    let sizes = Sizes::new(display);
    draw::frame(
        || match top_art {
            Some(art) => draw_top_art(view, art, sizes),
            None => draw_top(view, sizes),
        },
        || {
            if let Some(background) = skin.background {
                background.draw(0.0, 0.0, 320.0, 240.0, 0.0);
            }
            draw_bottom(view, icons, skin, sizes);
            if let Some((model, angle)) = preview {
                // Below the rows and above the hint.
                let top = view.layout.top(view.rows.len()) + 4.0;
                let bottom = 204.0;
                model.draw(160.0, (top + bottom) / 2.0, bottom - top, angle, 320);
            }
        },
    );
}

/// The art carries the app's name, so only the page title and text go in its box.
fn draw_top_art(view: &View, art: *mut c_void, sizes: Sizes) {
    draw::image(art, 0.0, 0.0, 400.0);
    draw::text_fit(Font::Title, &view.title, TITLE_LEFT, BOX_TOP, sizes.title, BOX_RIGHT - TITLE_LEFT, draw::GOLD, LEFT);
    let body_top = BOX_TOP + draw::line_height(sizes.title) + 2.0;
    // Long text gets smaller, a step at a time, until it fits the box.
    let mut scale = sizes.text;
    let mut lines = draw::wrap(Font::Body, &view.body, scale, BOX_RIGHT - BOX_LEFT);
    while scale > sizes.body_smallest() && lines.len() as f32 * draw::line_height(scale) > BOX_BOTTOM - body_top {
        scale -= 0.03;
        lines = draw::wrap(Font::Body, &view.body, scale, BOX_RIGHT - BOX_LEFT);
    }
    let step = draw::line_height(scale).min((BOX_BOTTOM - body_top) / lines.len().max(1) as f32);
    for (index, line) in lines.iter().enumerate() {
        draw::text(line, BOX_LEFT, body_top + index as f32 * step, scale, draw::WHITE, LEFT);
    }
}

fn draw_top(view: &View, sizes: Sizes) {
    let accent = draw::GOLD;
    draw::triforce(200.0, 10.0, 16.0, accent);
    draw::text_in(Font::Title, &view.title, 200.0, 38.0, sizes.title, draw::WHITE, CENTER);
    draw::text(&view.subtitle, 200.0, 66.0, sizes.text, draw::MUTED, CENTER);
    draw::rect(24.0, 90.0, 352.0, 1.0, draw::DIM);
    let lines = draw::wrap(Font::Body, &view.body, sizes.text, 352.0);
    let step = draw::line_height(sizes.text);
    for (index, line) in lines.iter().take(((240.0 - 98.0) / step) as usize).enumerate() {
        draw::text(line, 24.0, 98.0 + index as f32 * step, sizes.text, draw::WHITE, LEFT);
    }
}

/// Text scale that fits a line in a pill's height, at most the chosen text size.
fn pill_scale(sizes: Sizes) -> f32 {
    let per_scale = draw::line_height(1.0);
    if per_scale > 0.0 { sizes.text.min((PILL_HEIGHT - 2.0) / per_scale) } else { sizes.text }
}

/// Draws text centered in a box, made smaller if it is too wide.
fn centered(content: &str, x: f32, y: f32, w: f32, h: f32, scale: f32, color: u32) {
    let scale = {
        let width = draw::text_width(Font::Body, content, scale);
        if width > w - 6.0 && width > 0.0 { scale * (w - 6.0) / width } else { scale }
    };
    draw::text(content, x + w / 2.0, y + (h - draw::line_height(scale)) / 2.0, scale, color, CENTER);
}

fn draw_pills(pills: &Pills, top: f32, scale: f32) {
    match pills {
        Pills::Choices { names, selected } => {
            for (index, (left, width)) in pill_spans(names.len()).enumerate() {
                let picked = index == *selected;
                pill_plate(picked, left, top, width);
                centered(&names[index], left, top, width, PILL_HEIGHT, scale, if picked { draw::INK } else { draw::MUTED });
            }
        }
        Pills::Number { value, min, max } => {
            for (left, sign) in step_lefts().into_iter().zip(["-", "+"]) {
                pill_plate(false, left, top, STEP_WIDTH);
                centered(sign, left, top, STEP_WIDTH, PILL_HEIGHT, scale, draw::GOLD);
            }
            let left = step_lefts()[0] + STEP_WIDTH + 4.0;
            let width = PILL_WIDTH - 2.0 * (STEP_WIDTH + 4.0);
            let fraction = if max > min { (value - min) as f32 / (max - min) as f32 } else { 0.0 };
            pill_plate(false, left, top, width);
            // Inside the rounded ends, so the fill never pokes past the corners.
            let inner = width - 2.0 * PILL_RADIUS;
            draw::rect(left + PILL_RADIUS, top + PILL_HEIGHT - 3.0, inner * fraction, 3.0, draw::GOLD);
            centered(&value.to_string(), left, top, width, PILL_HEIGHT - 2.0, scale, draw::WHITE);
        }
    }
}

/// Rules pages: each setting is a card with its name on top and its choices below.
fn draw_cards(view: &View, icons: &Icons, skin: &Skin, sizes: Sizes) {
    let layout = view.layout;
    let icon_size = 18.0;
    let pill_text = pill_scale(sizes);
    let name_scale = (PILL_TOP - 8.0) / draw::line_height(1.0).max(1.0);
    let name_scale = sizes.text.min(name_scale);
    for (slot, row) in in_draw_order(view) {
        let y = layout.top(slot);
        plate(skin, Plate::Card, row.focused, y, layout.height);
        let mut name_left = ROW_LEFT + 10.0;
        if let Some((name, image)) = row.icon.and_then(|name| Some((name, icons.get(name)?))) {
            draw::icon(image, name_left, y + 4.0, icon_size, ui_core::icons::extra_turns(name));
            name_left += icon_size + 4.0;
        }
        let name_top = y + (PILL_TOP - draw::line_height(name_scale)) / 2.0;
        let color = if row.focused { draw::WHITE } else { draw::MUTED };
        draw::text_fit(Font::Body, &row.label, name_left, name_top, name_scale, ROW_LEFT + ROW_WIDTH - 8.0 - name_left, color, LEFT);
        if let Some(pills) = &row.pills {
            draw_pills(pills, y + PILL_TOP, pill_text);
        }
    }
    draw_scrollbar(view);
    draw::text_fit(Font::Body, &view.hint, 160.0, 212.0, sizes.text, 304.0, draw::MUTED, CENTER);
}

fn draw_scrollbar(view: &View) {
    let layout = view.layout;
    if view.total_rows > layout.visible {
        let track_height = layout.visible as f32 * layout.pitch() - layout.gap;
        let thumb = track_height * layout.visible as f32 / view.total_rows as f32;
        let offset = track_height * view.scroll as f32 / view.total_rows as f32;
        draw::rect(314.0, ROW_TOP, 3.0, track_height, draw::TRACK);
        draw::rect(314.0, ROW_TOP + offset, 3.0, thumb, draw::GOLD);
    }
}

/// Spoiler entries: the place in small text over what it holds.
fn draw_lines(view: &View, skin: &Skin, sizes: Sizes) {
    let layout = view.layout;
    let per_scale = draw::line_height(1.0).max(1.0);
    let small = (sizes.text * 0.85).min(18.0 / per_scale);
    let large = sizes.text.min(22.0 / per_scale);
    let room = ROW_WIDTH - 20.0;
    for (slot, row) in in_draw_order(view) {
        let y = layout.top(slot);
        plate(skin, Plate::Line, row.focused, y, layout.height);
        let left = ROW_LEFT + 10.0;
        match &row.value {
            Some(value) => {
                draw::text_fit(Font::Body, &row.label, left, y + 3.0, small, room, draw::MUTED, LEFT);
                let color = if row.focused { draw::GOLD } else { draw::WHITE };
                draw::text_fit(Font::Body, value, left, y + layout.height - 3.0 - draw::line_height(large), large, room, color, LEFT);
            }
            None => {
                let top = y + (layout.height - draw::line_height(large)) / 2.0;
                draw::text_fit(Font::Body, &row.label, left, top, large, room, draw::MUTED, LEFT);
            }
        }
    }
    draw_scrollbar(view);
    draw::text_fit(Font::Body, &view.hint, 160.0, 212.0, sizes.text, 304.0, draw::MUTED, CENTER);
}

fn draw_bottom(view: &View, icons: &Icons, skin: &Skin, sizes: Sizes) {
    if view.layout == Layout::CARDS {
        return draw_cards(view, icons, skin, sizes);
    }
    if view.layout == Layout::LINES {
        return draw_lines(view, skin, sizes);
    }
    let shift = if view.rows.iter().any(|row| row.icon.is_some_and(|name| icons.get(name).is_some())) { ICON_SPACE } else { 0.0 };
    let label_left = ROW_LEFT + 10.0 + shift;
    let label_room = |has_value: bool| if has_value { VALUE_LEFT - 4.0 - label_left } else { ROW_LEFT + ROW_WIDTH - 8.0 - label_left };
    let adjustable_room = 2.0 * (ROW_LEFT + ROW_WIDTH - 18.0 - VALUE_CENTER);
    let fixed_room = ROW_LEFT + ROW_WIDTH - 8.0 - VALUE_LEFT;
    // One size for every row on the page, small enough for the longest label or value to fit.
    let fit = |text: &str, room: f32| {
        let width = draw::text_width(Font::Body, text, sizes.text);
        if width > room { sizes.text * room / width } else { sizes.text }
    };
    let scale = view
        .rows
        .iter()
        .map(|row| {
            let label = fit(&row.label, label_room(row.value.is_some()));
            let value = row.value.as_deref().map_or(sizes.text, |value| fit(value, if row.adjustable { adjustable_room } else { fixed_room }));
            label.min(value)
        })
        .fold(sizes.text, f32::min);
    if scale < sizes.text {
        note_once(&format!("Rows on \"{}\" drawn at {scale:.3} instead of {:.3} to fit.", view.title, sizes.text));
    }
    let text_y = (ROW_HEIGHT - draw::line_height(scale)) / 2.0;
    for (slot, row) in in_draw_order(view) {
        let y = Layout::ROWS.top(slot);
        plate(skin, Plate::Bar, row.focused, y, ROW_HEIGHT);
        if let Some((name, image)) = row.icon.and_then(|name| Some((name, icons.get(name)?))) {
            let turns = ui_core::icons::extra_turns(name);
            draw::icon(image, ROW_LEFT + 6.0, y + (ROW_HEIGHT - ICON_SIZE) / 2.0, ICON_SIZE, turns);
        }
        draw::text(&row.label, label_left, y + text_y, scale, draw::WHITE, LEFT);
        if let Some(value) = &row.value {
            if row.adjustable {
                draw::text(value, VALUE_CENTER, y + text_y, scale, draw::GOLD, CENTER);
                if row.focused {
                    draw::text("<", VALUE_LEFT, y + text_y, scale, draw::GOLD, LEFT);
                    draw::text(">", ROW_LEFT + ROW_WIDTH - 6.0, y + text_y, scale, draw::GOLD, RIGHT);
                }
            } else {
                draw::text(value, ROW_LEFT + ROW_WIDTH - 8.0, y + text_y, scale, draw::MUTED, RIGHT);
            }
        }
    }
    draw_scrollbar(view);
    draw::text_fit(Font::Body, &view.hint, 160.0, 212.0, sizes.text, 304.0, draw::MUTED, CENTER);
}
