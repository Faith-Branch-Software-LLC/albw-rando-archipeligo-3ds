//! Milestone 1 smoke test: citro2d draws on both screens and reads buttons and touch.

use std::ffi::CString;

use citro2d_sys as c2d;
use ctru::prelude::*;

const BACKGROUND: u32 = abgr(0x1E, 0x24, 0x30);
const ROW: u32 = abgr(0x33, 0x3D, 0x50);
const ROW_FOCUSED: u32 = abgr(0x3F, 0x8F, 0x6B);
const WHITE: u32 = abgr(0xFF, 0xFF, 0xFF);

const ROWS: [&str; 3] = ["D-pad moves focus", "Touch selects a row", "START exits"];
const ROW_X: f32 = 20.0;
const ROW_W: f32 = 280.0;
const ROW_H: f32 = 48.0;
const ROW_GAP: f32 = 12.0;
const ROW_TOP: f32 = 30.0;

const fn abgr(r: u8, g: u8, b: u8) -> u32 {
    0xFF00_0000 | ((b as u32) << 16) | ((g as u32) << 8) | r as u32
}

fn row_y(index: usize) -> f32 {
    ROW_TOP + index as f32 * (ROW_H + ROW_GAP)
}

fn row_at(x: f32, y: f32) -> Option<usize> {
    (0..ROWS.len()).find(|&i| {
        x >= ROW_X && x < ROW_X + ROW_W && y >= row_y(i) && y < row_y(i) + ROW_H
    })
}

fn main() {
    let _gfx = Gfx::new().expect("failed to start GFX");
    let mut hid = Hid::new().expect("failed to start HID");
    let apt = Apt::new().expect("failed to start APT");

    // SAFETY: Gfx is alive, and this is the only citro2d user.
    assert!(unsafe { c2d::shim_init() }, "failed to start citro2d");

    let title = CString::new("ALBW Randomizer").unwrap();
    let subtitle = CString::new("Toolchain smoke test").unwrap();
    let labels: Vec<CString> = ROWS.iter().map(|s| CString::new(*s).unwrap()).collect();
    let mut focus = 0usize;

    while apt.main_loop() {
        hid.scan_input();
        let down = hid.keys_down();
        if down.contains(KeyPad::START) {
            break;
        }
        if down.contains(KeyPad::DPAD_DOWN) {
            focus = (focus + 1) % ROWS.len();
        }
        if down.contains(KeyPad::DPAD_UP) {
            focus = (focus + ROWS.len() - 1) % ROWS.len();
        }
        if down.contains(KeyPad::TOUCH) {
            let (x, y) = hid.touch_position();
            if let Some(row) = row_at(x as f32, y as f32) {
                focus = row;
            }
        }

        // SAFETY: citro2d is initialized, and each pointer is a live CString.
        unsafe {
            c2d::shim_frame_begin();

            c2d::shim_screen_begin(0, BACKGROUND);
            c2d::shim_text(title.as_ptr(), 24.0, 80.0, 1.0, WHITE);
            c2d::shim_text(subtitle.as_ptr(), 24.0, 120.0, 0.6, WHITE);

            c2d::shim_screen_begin(1, BACKGROUND);
            for (i, label) in labels.iter().enumerate() {
                let color = if i == focus { ROW_FOCUSED } else { ROW };
                c2d::shim_rect(ROW_X, row_y(i), ROW_W, ROW_H, color);
                c2d::shim_text(label.as_ptr(), ROW_X + 12.0, row_y(i) + 14.0, 0.6, WHITE);
            }

            c2d::shim_frame_end();
        }
    }

    // SAFETY: Called once, after the last frame.
    unsafe { c2d::shim_exit() };
}
