//! Drawing helpers and the placeholder look. Every shape is drawn in code until real art exists.

use std::ffi::CString;

use citro2d_sys as c2d;

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((a as u32) << 24) | ((b as u32) << 16) | ((g as u32) << 8) | r as u32
}

pub const SKY_TOP: u32 = rgba(0x10, 0x18, 0x2C, 255);
pub const SKY_BOTTOM: u32 = rgba(0x1C, 0x2C, 0x4A, 255);
pub const CARD: u32 = rgba(0x24, 0x31, 0x4D, 255);
pub const CARD_ACTIVE: u32 = rgba(0x2F, 0x41, 0x66, 255);
pub const TRACK: u32 = rgba(0x0C, 0x12, 0x22, 255);
pub const GOLD: u32 = rgba(0xF2, 0xC6, 0x4B, 255);
pub const GREEN: u32 = rgba(0x4F, 0xC2, 0x7A, 255);
pub const RED: u32 = rgba(0xE0, 0x5A, 0x5A, 255);
pub const WHITE: u32 = rgba(0xF4, 0xF6, 0xFB, 255);
pub const MUTED: u32 = rgba(0x98, 0xA6, 0xC2, 255);
pub const DIM: u32 = rgba(0x4E, 0x5C, 0x7A, 255);
/// Unpicked pills, and the well behind a number.
pub const WELL: u32 = rgba(0x18, 0x22, 0x3A, 255);
/// Text on a gold pill.
pub const INK: u32 = rgba(0x1A, 0x1F, 0x2E, 255);
const SHINE: u32 = rgba(0xFF, 0xFF, 0xFF, 70);

pub const LEFT: i32 = 0;
pub const CENTER: i32 = 1;
pub const RIGHT: i32 = 2;

pub fn start() {
    // SAFETY: Gfx is alive, and this is the only citro2d user.
    assert!(unsafe { c2d::shim_init() }, "failed to start citro2d");
}

pub fn stop() {
    // SAFETY: Called once, after the last frame.
    unsafe { c2d::shim_exit() };
}

/// Draws one frame. `top` and `bottom` draw each screen over its background.
pub fn frame(top: impl FnOnce(), bottom: impl FnOnce()) {
    // SAFETY: citro2d is running, and the main thread is its only user.
    unsafe {
        c2d::shim_frame_begin();
        c2d::shim_screen_begin(0, SKY_TOP);
        c2d::shim_rect_gradient(0.0, 0.0, 400.0, 240.0, SKY_TOP, SKY_BOTTOM);
    }
    top();
    // SAFETY: As above.
    unsafe {
        c2d::shim_screen_begin(1, SKY_TOP);
        c2d::shim_rect_gradient(0.0, 0.0, 320.0, 240.0, SKY_BOTTOM, SKY_TOP);
    }
    bottom();
    // SAFETY: As above.
    unsafe { c2d::shim_frame_end() };
}

/// Font slots, filled from the SD card by `load_font`. An empty slot uses the system font.
#[derive(Clone, Copy)]
pub enum Font {
    Body = 0,
    Title = 1,
}

/// Loads a .bcfnt font from memory into a slot. Returns false if it could not load.
pub fn load_font_from(font: Font, data: &[u8]) -> bool {
    // SAFETY: citro2d is running, and it copies the data before returning.
    unsafe { c2d::shim_font_load_mem(font as i32, data.as_ptr().cast(), data.len()) }
}

/// Draws text in the body font.
pub fn text(content: &str, x: f32, y: f32, scale: f32, color: u32, align: i32) {
    text_in(Font::Body, content, x, y, scale, color, align);
}

/// The width `text_in` would draw the text at.
pub fn text_width(font: Font, content: &str, scale: f32) -> f32 {
    let Ok(content) = CString::new(content) else { return 0.0 };
    // SAFETY: Only called inside `frame`, and the pointer is live for the call.
    unsafe { c2d::shim_text_width(content.as_ptr(), scale, font as i32) }
}

/// The height of one line of text at a scale.
pub fn line_height(scale: f32) -> f32 {
    // SAFETY: citro2d is running.
    unsafe { c2d::shim_line_height(scale) }
}

/// Draws text, made smaller if needed so it is no wider than `max_width`.
pub fn text_fit(font: Font, content: &str, x: f32, y: f32, scale: f32, max_width: f32, color: u32, align: i32) {
    let width = text_width(font, content, scale);
    let scale = if width > max_width && width > 0.0 { scale * max_width / width } else { scale };
    text_in(font, content, x, y, scale, color, align);
}

/// Splits text into lines no wider than `max_width`, at spaces. Line breaks already in the text stay.
pub fn wrap(font: Font, lines: &[String], scale: f32, max_width: f32) -> Vec<String> {
    let mut out = Vec::new();
    for source in lines {
        let mut line = String::new();
        for word in source.split_whitespace() {
            let candidate = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
            if !line.is_empty() && text_width(font, &candidate, scale) > max_width {
                out.push(std::mem::replace(&mut line, word.to_owned()));
            } else {
                line = candidate;
            }
        }
        // Keeps blank lines and indented lines as they were.
        if source.starts_with(' ') && !line.is_empty() {
            line = format!("{}{line}", &source[..source.len() - source.trim_start().len()]);
        }
        out.push(line);
    }
    out
}

pub fn text_in(font: Font, content: &str, x: f32, y: f32, scale: f32, color: u32, align: i32) {
    let Ok(content) = CString::new(content) else { return };
    // SAFETY: Only called inside `frame`, and the pointer is live for the call.
    unsafe { c2d::shim_text_aligned(content.as_ptr(), x, y, scale, color, align, font as i32) };
}

/// Draws a loaded image. `image` comes from `load_image` and is never freed.
pub fn image(image: *mut core::ffi::c_void, x: f32, y: f32, size: f32) {
    // SAFETY: Only called inside `frame`, and loaded images live until the app exits.
    unsafe { c2d::shim_image_draw(image, x, y, size) };
}

/// Draws a loaded image scaled to fit a `size` square, centered in it, turned `extra_turns`
/// quarter turns counterclockwise.
pub fn icon(image: *mut core::ffi::c_void, x: f32, y: f32, size: f32, extra_turns: u8) {
    // SAFETY: Only called inside `frame`, and loaded images live until the app exits.
    unsafe { c2d::shim_image_draw_centered(image, x, y, size, extra_turns.into()) };
}

/// Makes a texture from a game image. Returns null if the GPU memory is full.
pub fn load_image(flim: &ui_core::flim::Flim) -> *mut core::ffi::c_void {
    let (texture_width, texture_height) = (i32::from(flim.texture_width), i32::from(flim.texture_height));
    // SAFETY: `parse` checked that `pixels` fills the texture, and the shim copies it.
    unsafe {
        c2d::shim_image_load_game(
            flim.gpu_format.into(),
            texture_width,
            texture_height,
            flim.width.into(),
            flim.height.into(),
            flim.transposed,
            flim.pixels.as_ptr(),
            flim.pixels.len(),
        )
    }
}

/// Makes a texture from a decoded picture. Returns null if the GPU memory is full.
pub fn load_picture(picture: &ui_core::picture::Texture) -> *mut core::ffi::c_void {
    let (texture_width, texture_height) = (i32::from(picture.texture_width), i32::from(picture.texture_height));
    // SAFETY: `decode` made `pixels` fill the texture, and the shim copies it.
    unsafe { c2d::shim_image_load_rgba8(texture_width, texture_height, picture.width.into(), picture.height.into(), picture.pixels.as_ptr()) }
}

/// A loaded picture and its size in pixels, for art drawn at other sizes.
#[derive(Clone, Copy)]
pub struct Art {
    image: *mut core::ffi::c_void,
    pub width: f32,
    pub height: f32,
}

impl Art {
    /// Loads a decoded picture. Returns None if the GPU memory is full.
    pub fn load(picture: &ui_core::picture::Texture) -> Option<Self> {
        let image = load_picture(picture);
        (!image.is_null()).then(|| Self { image, width: picture.width.into(), height: picture.height.into() })
    }

    fn part(self, sx: f32, sy: f32, sw: f32, sh: f32, x: f32, y: f32, w: f32, h: f32) {
        // SAFETY: Only called inside `frame`, and loaded images live until the app exits.
        unsafe { c2d::shim_image_draw_part(self.image, sx, sy, sw, sh, x, y, w, h) };
    }

    /// Draws the art filling w by h. Art already that size is drawn as is. Otherwise the corners,
    /// `border` pixels on a side, keep their size, the edges stretch one way, and the middle both.
    pub fn draw(self, x: f32, y: f32, w: f32, h: f32, border: f32) {
        if (self.width - w).abs() < 0.5 && (self.height - h).abs() < 0.5 {
            return self.part(0.0, 0.0, self.width, self.height, x, y, w, h);
        }
        let bx = border.min(self.width / 2.0).min(w / 2.0);
        let by = border.min(self.height / 2.0).min(h / 2.0);
        let source = [(0.0, bx), (bx, self.width - 2.0 * bx), (self.width - bx, bx)];
        let target = [(x, bx), (x + bx, w - 2.0 * bx), (x + w - bx, bx)];
        let source_rows = [(0.0, by), (by, self.height - 2.0 * by), (self.height - by, by)];
        let target_rows = [(y, by), (y + by, h - 2.0 * by), (y + h - by, by)];
        for ((sy, sh), (ty, th)) in source_rows.into_iter().zip(target_rows) {
            for ((sx, sw), (tx, tw)) in source.into_iter().zip(target) {
                self.part(sx, sy, sw, sh, tx, ty, tw, th);
            }
        }
    }
}

/// A soft glow around a box: layers of the color, fainter as they spread out.
pub fn glow(x: f32, y: f32, w: f32, h: f32, color: u32) {
    let rgb = color & 0x00FF_FFFF;
    for (spread, alpha) in [(5.0, 18u32), (4.0, 24), (3.0, 34), (2.0, 48), (1.0, 70)] {
        let layer = rgb | (alpha << 24);
        // Corners stay inside the spread, so the glow reads as rounded.
        rect(x - spread + 1.0, y - spread, w + 2.0 * spread - 2.0, h + 2.0 * spread, layer);
        rect(x - spread, y - spread + 1.0, 1.0, h + 2.0 * spread - 2.0, layer);
        rect(x + w + spread - 1.0, y - spread + 1.0, 1.0, h + 2.0 * spread - 2.0, layer);
    }
}

pub fn rect(x: f32, y: f32, w: f32, h: f32, color: u32) {
    // SAFETY: Only called inside `frame`.
    unsafe { c2d::shim_rect(x, y, w, h, color) };
}

/// A rectangle with rounded corners. The color should be opaque: the pieces overlap at the corners.
pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32, color: u32) {
    let r = radius.min(w / 2.0).min(h / 2.0);
    rect(x + r, y, w - 2.0 * r, h, color);
    rect(x, y + r, r, h - 2.0 * r, color);
    rect(x + w - r, y + r, r, h - 2.0 * r, color);
    for (cx, cy) in [(x + r, y + r), (x + w - r, y + r), (x + r, y + h - r), (x + w - r, y + h - r)] {
        circle(cx, cy, r, color);
    }
}

pub fn circle(x: f32, y: f32, radius: f32, color: u32) {
    // SAFETY: Only called inside `frame`.
    unsafe { c2d::shim_circle(x, y, radius, color) };
}

fn pill(x: f32, y: f32, w: f32, h: f32, color: u32) {
    let radius = h / 2.0;
    circle(x + radius, y + radius, radius, color);
    circle(x + w - radius, y + radius, radius, color);
    rect(x + radius, y, (w - h).max(0.0), h, color);
}

/// A rounded bar. A band of light travels along the filled part while work is running.
pub fn bar(x: f32, y: f32, w: f32, h: f32, fraction: f32, color: u32, tick: u32, animate: bool) {
    pill(x, y, w, h, TRACK);
    let fill = (w * fraction.clamp(0.0, 1.0)).max(h);
    pill(x, y, fill, h, color);
    let inner = fill - h;
    if animate && inner > 4.0 {
        let band = 36.0;
        let head = (tick as f32 * 2.5) % (inner + band);
        let start = (head - band).max(0.0);
        let end = head.min(inner);
        rect(x + h / 2.0 + start, y, end - start, h, SHINE);
    }
}

pub fn triforce(center_x: f32, top: f32, side: f32, color: u32) {
    let height = side * 0.866;
    let piece = |x: f32, y: f32| {
        // SAFETY: Only called inside `frame`.
        unsafe { c2d::shim_triangle(x, y, x - side / 2.0, y + height, x + side / 2.0, y + height, color) };
    };
    piece(center_x, top);
    piece(center_x - side / 2.0, top + height);
    piece(center_x + side / 2.0, top + height);
}
