//! Draws the view that `ui_core::menu` produces.

use ui_core::menu::{Page, View, ROW_GAP, ROW_HEIGHT, ROW_LEFT, ROW_TOP, ROW_WIDTH, VALUE_CENTER, VALUE_LEFT, VISIBLE_ROWS};

use crate::draw::{self, CENTER, LEFT, RIGHT};

pub fn draw(view: &View, page: Page) {
    draw::frame(|| draw_top(view, page), || draw_bottom(view));
}

fn draw_top(view: &View, page: Page) {
    let accent = if page == Page::OldWarning { draw::RED } else { draw::GOLD };
    draw::triforce(200.0, 10.0, 16.0, accent);
    draw::text(&view.title, 200.0, 40.0, 0.75, draw::WHITE, CENTER);
    draw::text(&view.subtitle, 200.0, 66.0, 0.47, draw::MUTED, CENTER);
    draw::rect(24.0, 90.0, 352.0, 1.0, draw::DIM);
    for (index, line) in view.body.iter().take(7).enumerate() {
        draw::text(line, 24.0, 98.0 + index as f32 * 19.0, 0.48, draw::WHITE, LEFT);
    }
}

fn draw_bottom(view: &View) {
    for (slot, row) in view.rows.iter().enumerate() {
        let y = ROW_TOP + slot as f32 * (ROW_HEIGHT + ROW_GAP);
        draw::rect(ROW_LEFT, y, ROW_WIDTH, ROW_HEIGHT, if row.focused { draw::CARD_ACTIVE } else { draw::CARD });
        if row.focused {
            draw::rect(ROW_LEFT, y, 3.0, ROW_HEIGHT, draw::GOLD);
        }
        draw::text(&row.label, ROW_LEFT + 10.0, y + 6.0, 0.47, draw::WHITE, LEFT);
        if let Some(value) = &row.value {
            if row.adjustable {
                draw::text(value, VALUE_CENTER, y + 6.0, 0.47, draw::GOLD, CENTER);
                if row.focused {
                    draw::text("<", VALUE_LEFT, y + 6.0, 0.47, draw::GOLD, LEFT);
                    draw::text(">", ROW_LEFT + ROW_WIDTH - 6.0, y + 6.0, 0.47, draw::GOLD, RIGHT);
                }
            } else {
                draw::text(value, ROW_LEFT + ROW_WIDTH - 8.0, y + 6.0, 0.47, draw::MUTED, RIGHT);
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
    draw::text(&view.hint, 160.0, 208.0, 0.42, draw::MUTED, CENTER);
}
