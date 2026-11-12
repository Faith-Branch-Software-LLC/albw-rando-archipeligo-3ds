//! Draws the view that `ui_core::menu` produces.

use ui_core::menu::{
    pill_boxes, step_lefts, Layout, Pills, View, PILL_HEIGHT, PILL_TOP, PILL_WIDTH, ROW_HEIGHT, ROW_LEFT, ROW_TOP,
    ROW_WIDTH, STEP_WIDTH, VALUE_CENTER, VALUE_LEFT, HINT_TOP, VIEW_BOTTOM,
};

use std::ffi::c_void;

use crate::art::Skin;
use crate::draw::{self, Font, CENTER, LEFT, RIGHT};
use crate::icons::Icons;
use crate::model_view::Preview;

/// Text sizes in pixels. citro2d draws any font with 30 pixel glyph cells at scale 1, so a size in
/// pixels is drawn at that size over 30. Rows, hints and the top screen text share one size so the
/// screens look even. The body font is converted at 18 pixels, so text at 18 is sharpest.
const TEXT_SIZE: f32 = 18.0;
const TITLE_SIZE: f32 = 26.0;
/// The height of the hint line's button badges.
const BADGE_SIZE: f32 = 21.0;

/// Text scales for `TEXT_SIZE` and `TITLE_SIZE`.
#[derive(Clone, Copy)]
pub(crate) struct Sizes {
    pub(crate) text: f32,
    pub(crate) title: f32,
}

impl Sizes {
    pub(crate) fn new() -> Self {
        Self { text: TEXT_SIZE / 30.0, title: TITLE_SIZE / 30.0 }
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
/// The model preview's box ends a few pixels above the hint bar, which starts at y 199.
const MODEL_BOX_BOTTOM: f32 = 194.0;
/// The box is centered, and wide enough for the model as it turns.
const MODEL_BOX_WIDTH: f32 = 128.0;
const MODEL_BOX_LEFT: f32 = 160.0 - MODEL_BOX_WIDTH / 2.0;
/// Room between the model and the box's edges.
const MODEL_MARGIN: f32 = 10.0;
/// Pills are plain colors with rounded corners.
const PILL_RADIUS: f32 = 5.0;

/// The three row shapes, each with its own art.
#[derive(Clone, Copy)]
enum Plate {
    Bar,
    Card,
    /// A card with two lines of pills.
    TallCard,
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
        // Without tall card art, the usual card art stretches.
        Plate::TallCard => (skin.card_tall.or(skin.card), skin.card_tall_focused.or(skin.card_focused)),
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
pub fn draw(view: &View, icons: &Icons, top_art: Option<*mut c_void>, skin: &Skin, preview: Option<(&Preview, f32)>) {
    let sizes = Sizes::new();
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
                // A box below the rows and above the hint bar, clear of the focused row's glow.
                let top = view.rows.last().map_or(ROW_TOP, |row| row.top + row.height) + 6.0;
                let height = MODEL_BOX_BOTTOM - top;
                match skin.model_box.or(skin.line) {
                    Some(art) => art.draw(MODEL_BOX_LEFT, top, MODEL_BOX_WIDTH, height, PLATE_BORDER),
                    None => draw::rect(MODEL_BOX_LEFT, top, MODEL_BOX_WIDTH, height, draw::CARD),
                }
                let room = height - 2.0 * MODEL_MARGIN;
                model.draw(160.0, top + height / 2.0, room, angle, 320);
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

/// The hint line's text color, light yellow so it stands out on the bar.
const HINT_COLOR: u32 = draw::rgba(0xFF, 0xEE, 0x9C, 255);
/// The middle of the bar at the bottom of the background art (y 199 to 239).
const HINT_MIDDLE: f32 = 219.0;
const HINT_GAP: f32 = 12.0;

/// A control named in the hint line, drawn the way it looks on the console.
enum Control<'a> {
    /// A, B, X or Y: a letter in a circle.
    Round(&'a str),
    /// L/R or START: a word in a rounded box.
    Wide(&'a str),
    /// The D-pad, with the directions in use lit.
    Pad { across: bool },
}

impl Control<'_> {
    fn of(key: &str) -> Option<Control<'_>> {
        match key {
            "A" | "B" | "X" | "Y" => Some(Control::Round(key)),
            "L/R" | "START" | "Start" | "SELECT" => Some(Control::Wide(key)),
            "Left/Right" => Some(Control::Pad { across: true }),
            "Up/Down" => Some(Control::Pad { across: false }),
            _ => None,
        }
    }

    /// `badge` is how tall the badge is, and how wide a round one is.
    fn width(&self, badge: f32, key_scale: f32) -> f32 {
        match self {
            Control::Round(_) | Control::Pad { .. } => badge,
            Control::Wide(key) => draw::text_width(Font::Body, key, key_scale) + badge * 0.6,
        }
    }

    fn draw(&self, left: f32, badge: f32, key_scale: f32) {
        let top = HINT_MIDDLE - badge / 2.0;
        match self {
            Control::Round(key) => {
                draw::circle(left + badge / 2.0, HINT_MIDDLE, badge / 2.0, HINT_COLOR);
                centered(key, left, top, badge, badge, key_scale, draw::INK);
            }
            Control::Wide(key) => {
                let width = self.width(badge, key_scale);
                draw::rounded_rect(left, top, width, badge, badge / 2.0, HINT_COLOR);
                centered(key, left, top, width, badge, key_scale, draw::INK);
            }
            Control::Pad { across } => {
                // A plus sign, with the arms in use lit and the others dim.
                let arm = (badge * 5.0 / 16.0).round();
                let (lit, unlit) = (HINT_COLOR, draw::DIM);
                let (horizontal, vertical) = if *across { (lit, unlit) } else { (unlit, lit) };
                draw::rect(left + arm + 0.5, top, badge - 2.0 * arm - 1.0, badge, vertical);
                draw::rect(left, top + arm + 0.5, badge, badge - 2.0 * arm - 1.0, horizontal);
                draw::rect(left + arm + 0.5, top + arm + 0.5, badge - 2.0 * arm - 1.0, badge - 2.0 * arm - 1.0, lit);
            }
        }
    }
}

/// Covers rows that scrolled under the hint bar with the bar's own background.
fn cover_hint_bar(skin: &Skin) {
    let height = 240.0 - HINT_TOP;
    match skin.background {
        Some(background) => background.part(0.0, HINT_TOP, 320.0, height, 0.0, HINT_TOP, 320.0, height),
        // The art's bar: navy between gold lines.
        None => {
            draw::rect(0.0, HINT_TOP, 320.0, height, draw::GOLD_EDGE);
            draw::rect(0.0, HINT_TOP + 2.0, 320.0, height - 4.0, draw::NAVY_DEEP);
        }
    }
}

/// The hint line, such as "A Select   B Back": each control drawn as a button, then what it does.
pub(crate) fn draw_hint(hint: &str, skin: &Skin, sizes: Sizes) {
    cover_hint_bar(skin);
    let per_scale = draw::line_height(1.0).max(1.0);
    let badge = BADGE_SIZE;
    let key_scale = sizes.text.min((badge - 3.0) / per_scale);
    let parts: Vec<(Option<Control>, &str)> = hint
        .split("   ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| match part.split_once(' ') {
            Some((key, label)) => match Control::of(key) {
                Some(control) => (Some(control), label),
                None => (None, part),
            },
            None => (None, part),
        })
        .collect();
    let width_at = |scale: f32| {
        parts
            .iter()
            .map(|(control, label)| control.as_ref().map_or(0.0, |control| control.width(badge, key_scale) + 4.0) + draw::text_width(Font::Body, label, scale))
            .sum::<f32>()
            + HINT_GAP * parts.len().saturating_sub(1) as f32
    };
    let room = 304.0;
    let full = width_at(sizes.text);
    let scale = if full > room && full > 0.0 { sizes.text * room / full } else { sizes.text };
    let mut left = 160.0 - width_at(scale).min(room) / 2.0;
    let text_top = HINT_MIDDLE - draw::line_height(scale) / 2.0;
    for (control, label) in &parts {
        if let Some(control) = control {
            control.draw(left, badge, key_scale);
            left += control.width(badge, key_scale) + 4.0;
        }
        draw::text(label, left, text_top, scale, HINT_COLOR, LEFT);
        left += draw::text_width(Font::Body, label, scale) + HINT_GAP;
    }
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
            for (index, (left, line_top, width)) in pill_boxes(names.len()).into_iter().enumerate() {
                let picked = index == *selected;
                let top = top + line_top;
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
    for (_, row) in in_draw_order(view) {
        let y = row.top;
        let kind = if row.height > layout.height { Plate::TallCard } else { Plate::Card };
        plate(skin, kind, row.focused, y, row.height);
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
    draw_hint(&view.hint, skin, sizes);
}

fn draw_scrollbar(view: &View) {
    let track_height = VIEW_BOTTOM - ROW_TOP;
    if view.content_height > track_height + 0.5 {
        let thumb = track_height * track_height / view.content_height;
        let offset = track_height * view.scroll / view.content_height;
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
    for (_, row) in in_draw_order(view) {
        let y = row.top;
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
    draw_hint(&view.hint, skin, sizes);
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
    for (_, row) in in_draw_order(view) {
        let y = row.top;
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
    draw_hint(&view.hint, skin, sizes);
}
