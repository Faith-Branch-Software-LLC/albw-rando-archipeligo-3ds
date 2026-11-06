//! Draws the view that `ui_core::menu` produces.

use ui_core::display::Display;
use ui_core::menu::{View, ROW_GAP, ROW_HEIGHT, ROW_LEFT, ROW_TOP, ROW_WIDTH, VALUE_CENTER, VALUE_LEFT, VISIBLE_ROWS};

use std::ffi::c_void;

use crate::draw::{self, Font, CENTER, LEFT, RIGHT};
use crate::icons::Icons;

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
pub fn draw(view: &View, icons: &Icons, top_art: Option<*mut c_void>, display: Display) {
    let sizes = Sizes::new(display);
    draw::frame(
        || match top_art {
            Some(art) => draw_top_art(view, art, sizes),
            None => draw_top(view, sizes),
        },
        || draw_bottom(view, icons, sizes),
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

fn draw_bottom(view: &View, icons: &Icons, sizes: Sizes) {
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
    for (slot, row) in view.rows.iter().enumerate() {
        let y = ROW_TOP + slot as f32 * (ROW_HEIGHT + ROW_GAP);
        draw::rect(ROW_LEFT, y, ROW_WIDTH, ROW_HEIGHT, if row.focused { draw::CARD_ACTIVE } else { draw::CARD });
        if row.focused {
            draw::rect(ROW_LEFT, y, 3.0, ROW_HEIGHT, draw::GOLD);
        }
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
    if view.total_rows > VISIBLE_ROWS {
        let track_height = VISIBLE_ROWS as f32 * (ROW_HEIGHT + ROW_GAP) - ROW_GAP;
        let thumb = track_height * VISIBLE_ROWS as f32 / view.total_rows as f32;
        let offset = track_height * view.scroll as f32 / view.total_rows as f32;
        draw::rect(314.0, ROW_TOP, 3.0, track_height, draw::TRACK);
        draw::rect(314.0, ROW_TOP + offset, 3.0, thumb, draw::GOLD);
    }
    draw::text_fit(Font::Body, &view.hint, 160.0, 212.0, sizes.text, 304.0, draw::MUTED, CENTER);
}
