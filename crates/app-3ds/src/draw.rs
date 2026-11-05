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

pub fn text(content: &str, x: f32, y: f32, scale: f32, color: u32, align: i32) {
    let Ok(content) = CString::new(content) else { return };
    // SAFETY: Only called inside `frame`, and the pointer is live for the call.
    unsafe { c2d::shim_text_aligned(content.as_ptr(), x, y, scale, color, align) };
}

pub fn rect(x: f32, y: f32, w: f32, h: f32, color: u32) {
    // SAFETY: Only called inside `frame`.
    unsafe { c2d::shim_rect(x, y, w, h, color) };
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
