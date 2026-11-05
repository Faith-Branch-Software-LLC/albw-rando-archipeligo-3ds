//! Minimal bindings to the C shim over citro2d.
#![no_std]

// Forces libctru to link after citro2d and citro3d.
use ctru_sys as _;

use core::ffi::{c_char, c_int};

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
    pub fn shim_text_aligned(utf8: *const c_char, x: f32, y: f32, scale: f32, color: u32, align: c_int);
    pub fn shim_text(utf8: *const c_char, x: f32, y: f32, scale: f32, color: u32);
}
