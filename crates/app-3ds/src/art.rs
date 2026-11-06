//! Loads the menu art and fonts. Each is built into the app, and a file of the same name in
//! `/3ds/albw-rando/art/` on the SD card replaces it, so art can change without a rebuild.

use std::ffi::c_void;
use std::fs;

use ui_core::picture;

use crate::draw;

const FOLDER: &str = "sdmc:/3ds/albw-rando/art";

/// The top screen picture for the menus, 400 by 240.
const TOP: (&str, &[u8]) = ("top.png", include_bytes!("../assets/top.png"));
/// Fonts in the 3DS .bcfnt format. devkitPro's `mkbcfnt` makes them from .ttf or .otf files.
const TITLE_FONT: (&str, &[u8]) = ("title.bcfnt", include_bytes!("../assets/title.bcfnt"));
/// The body font comes in two conversions, with 14 and 18 pixel glyph cells. Text is sharpest
/// drawn at or a little below a font's cell size, so each text size uses the nearer one.
const BODY_FONT: (&str, &[u8]) = ("body.bcfnt", include_bytes!("../assets/body.bcfnt"));
const BODY_FONT_LARGE: (&str, &[u8]) = ("body-large.bcfnt", include_bytes!("../assets/body-large.bcfnt"));
/// Text sizes above this use the large body font.
const SMALL_BODY_FONT_UP_TO: u8 = 14;

/// Where the Display page's sizes are saved.
pub const DISPLAY: &str = "sdmc:/3ds/albw-rando/display.txt";

/// The saved text sizes, or the defaults.
pub fn load_display() -> ui_core::display::Display {
    ui_core::display::Display::parse(&fs::read_to_string(DISPLAY).unwrap_or_default())
}

fn body_font(text_size: u8) -> (&'static str, &'static [u8]) {
    if text_size <= SMALL_BODY_FONT_UP_TO {
        BODY_FONT
    } else {
        BODY_FONT_LARGE
    }
}

/// Swaps the body font if a new text size wants the other conversion. Call it between frames.
pub fn resize_body_font(old_size: u8, new_size: u8) {
    if body_font(old_size).0 != body_font(new_size).0 {
        load_font(draw::Font::Body, body_font(new_size));
    }
}

fn load_font(font: draw::Font, file: (&str, &'static [u8])) -> String {
    let (sd, built_in, source) = read(file);
    let data = if sd.is_empty() { built_in } else { &sd };
    if draw::load_font_from(font, data) {
        format!("Font: loaded {source}.")
    } else {
        format!("Font: {source} could not load, using the system font.")
    }
}

/// The SD card's copy of a file if there is one, else the built-in one. The note says which.
fn read(file: (&str, &'static [u8])) -> (Vec<u8>, &'static [u8], String) {
    let path = format!("{FOLDER}/{}", file.0);
    match fs::read(&path) {
        Ok(data) => (data, &[], format!("{} from the SD card", file.0)),
        Err(_) => (Vec::new(), file.1, format!("built-in {}", file.0)),
    }
}

/// Loads the fonts for a text size. Returns a note for the log.
pub fn load_fonts(text_size: u8) -> String {
    format!("{} {}", load_font(draw::Font::Title, TITLE_FONT), load_font(draw::Font::Body, body_font(text_size)))
}

/// Loads the top screen picture. Returns it, if it loaded, and a note for the log.
pub fn load_top() -> (Option<*mut c_void>, String) {
    let (sd, built_in, source) = read(TOP);
    let data = if sd.is_empty() { built_in } else { &sd };
    let texture = match picture::decode(data) {
        Ok(texture) => texture,
        Err(err) => return (None, format!("Top screen art: {source} could not load: {err}")),
    };
    let image = draw::load_picture(&texture);
    if image.is_null() {
        return (None, "Top screen art could not load: not enough GPU memory.".to_owned());
    }
    (Some(image), format!("Top screen art: loaded {source}, {}x{}.", texture.width, texture.height))
}
