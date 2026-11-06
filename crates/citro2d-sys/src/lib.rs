//! Minimal bindings to the C shim over citro2d.
#![no_std]

// Forces libctru to link after citro2d and citro3d.
use ctru_sys as _;

use core::ffi::{c_char, c_int, c_void};

unsafe extern "C" {
    pub fn shim_init() -> bool;
    pub fn shim_exit();
    pub fn shim_frame_begin();
    pub fn shim_frame_end();
    pub fn shim_screen_begin(screen: c_int, clear_color: u32);
    pub fn shim_rect(x: f32, y: f32, w: f32, h: f32, color: u32);
    pub fn shim_rect_gradient(x: f32, y: f32, w: f32, h: f32, top: u32, bottom: u32);
    pub fn shim_circle(x: f32, y: f32, radius: f32, color: u32);
    pub fn shim_triangle(x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32, color: u32);
    pub fn shim_text_aligned(utf8: *const c_char, x: f32, y: f32, scale: f32, color: u32, align: c_int, font: c_int);
    pub fn shim_text_width(utf8: *const c_char, scale: f32, font: c_int) -> f32;
    pub fn shim_line_height(scale: f32) -> f32;
    pub fn shim_font_load_mem(slot: c_int, data: *const c_void, size: usize) -> bool;
    pub fn shim_image_load_game(
        format: c_int,
        texture_width: c_int,
        texture_height: c_int,
        width: c_int,
        height: c_int,
        transposed: bool,
        pixels: *const u8,
        size: usize,
    ) -> *mut c_void;
    pub fn shim_image_load_rgba8(texture_width: c_int, texture_height: c_int, width: c_int, height: c_int, pixels: *const u8) -> *mut c_void;
    pub fn shim_image_draw(image: *mut c_void, x: f32, y: f32, size: f32);
    pub fn shim_image_draw_centered(image: *mut c_void, x: f32, y: f32, size: f32, extra_turns: c_int);
    pub fn shim_text(utf8: *const c_char, x: f32, y: f32, scale: f32, color: u32);
}
