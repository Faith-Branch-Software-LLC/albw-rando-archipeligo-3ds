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
/// The body font, converted at 18 pixel glyph cells to match the menus' 18 pixel text.
const BODY_FONT: (&str, &[u8]) = ("body.bcfnt", include_bytes!("../assets/body.bcfnt"));

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

/// Loads the fonts. Returns a note for the log.
pub fn load_fonts() -> String {
    format!("{} {}", load_font(draw::Font::Title, TITLE_FONT), load_font(draw::Font::Body, BODY_FONT))
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

/// Bottom screen art. Each piece is optional: anything missing is drawn with plain shapes. Pills
/// are always plain colors with rounded corners.
#[derive(Default)]
pub struct Skin {
    /// The whole bottom screen, 320 by 240, behind everything.
    pub background: Option<draw::Art>,
    /// Menu rows, 304 by 34.
    pub bar: Option<draw::Art>,
    pub bar_focused: Option<draw::Art>,
    /// Option cards with pills, 304 by 56. Without them, the bars stretch to fit.
    pub card: Option<draw::Art>,
    pub card_focused: Option<draw::Art>,
    /// Two-line rows, as in spoiler logs and credits, 304 by 44. Without them, the bars stretch.
    pub line: Option<draw::Art>,
    pub line_focused: Option<draw::Art>,
    /// Option cards with two lines of pills, 304 by 80.
    pub card_tall: Option<draw::Art>,
    pub card_tall_focused: Option<draw::Art>,
    /// The box the spinning model shows in on the Models and Voices page, 128 by 106.
    pub model_box: Option<draw::Art>,
    /// The triforce beside the title on the randomizing screen, 128 by 112, drawn at half size.
    pub triforce: Option<draw::Art>,
}

const TRIFORCE: (&str, &[u8]) = ("triforce.png", include_bytes!("../assets/triforce.png"));
const BACKGROUND: (&str, &[u8]) = ("bottom.png", include_bytes!("../assets/bottom.png"));
const BAR: (&str, &[u8]) = ("bar.png", include_bytes!("../assets/bar.png"));
const BAR_FOCUSED: (&str, &[u8]) = ("bar-focused.png", include_bytes!("../assets/bar-focused.png"));
const CARD: (&str, &[u8]) = ("card.png", include_bytes!("../assets/card.png"));
const CARD_FOCUSED: (&str, &[u8]) = ("card-focused.png", include_bytes!("../assets/card-focused.png"));
const LINE: (&str, &[u8]) = ("line.png", include_bytes!("../assets/line.png"));
const LINE_FOCUSED: (&str, &[u8]) = ("line-focused.png", include_bytes!("../assets/line-focused.png"));
const CARD_TALL: (&str, &[u8]) = ("card-tall.png", include_bytes!("../assets/card-tall.png"));
const CARD_TALL_FOCUSED: (&str, &[u8]) = ("card-tall-focused.png", include_bytes!("../assets/card-tall-focused.png"));
const MODEL_BOX: (&str, &[u8]) = ("model-box.png", include_bytes!("../assets/model-box.png"));

fn load_art(file: (&str, &'static [u8]), notes: &mut Vec<String>) -> Option<draw::Art> {
    let (sd, built_in, source) = read(file);
    let data = if sd.is_empty() { built_in } else { &sd };
    if data.is_empty() {
        return None;
    }
    let loaded = picture::decode(data).map_err(|err| err.to_string()).and_then(|texture| {
        draw::Art::load(&texture).ok_or_else(|| "not enough GPU memory".to_owned())
    });
    match loaded {
        Ok(art) => {
            notes.push(format!("loaded {source}"));
            Some(art)
        }
        Err(err) => {
            notes.push(format!("{source} could not load: {err}"));
            None
        }
    }
}

/// Loads the bottom screen art. Returns it and a note for the log.
pub fn load_skin() -> (Skin, String) {
    let mut notes = Vec::new();
    let skin = Skin {
        background: load_art(BACKGROUND, &mut notes),
        bar: load_art(BAR, &mut notes),
        bar_focused: load_art(BAR_FOCUSED, &mut notes),
        card: load_art(CARD, &mut notes),
        card_focused: load_art(CARD_FOCUSED, &mut notes),
        line: load_art(LINE, &mut notes),
        line_focused: load_art(LINE_FOCUSED, &mut notes),
        card_tall: load_art(CARD_TALL, &mut notes),
        card_tall_focused: load_art(CARD_TALL_FOCUSED, &mut notes),
        model_box: load_art(MODEL_BOX, &mut notes),
        triforce: load_art(TRIFORCE, &mut notes),
    };
    (skin, format!("Bottom screen art: {}.", notes.join(", ")))
}
