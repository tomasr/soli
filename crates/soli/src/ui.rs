//! Generic chrome: panels, text rows and transient messages.

use crate::render;
use macroquad::prelude::*;

const PANEL: Color = Color::new(0.06, 0.07, 0.08, 1.0);
const SCRIM: Color = Color::new(0.0, 0.0, 0.0, 0.35);
pub const DIM: Color = Color::new(1.0, 1.0, 1.0, 0.6);
pub const ACCENT: Color = Color::new(1.0, 0.85, 0.35, 1.0);

/// Dims the table and draws a dark rounded panel centred on screen.
pub fn panel(w: f32, h: f32) -> Rect {
    let (sw, sh) = (screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, sw, sh, SCRIM);
    let r = Rect::new(
        (sw - w.min(sw)) / 2.0,
        (sh - h.min(sh)) / 2.0,
        w.min(sw),
        h.min(sh),
    );
    render::rounded_rect(r, 14.0, PANEL);
    r
}

pub fn centered_text(text: &str, cx: f32, baseline: f32, size: f32, color: Color) {
    let m = render::measure(text, size);
    render::text(text, cx - m.width / 2.0, baseline, size, color);
}

pub fn right_text(text: &str, right: f32, baseline: f32, size: f32, color: Color) {
    let m = render::measure(text, size);
    render::text(text, right - m.width, baseline, size, color);
}

/// Short message shown above the status bar, fading out over its last part.
pub fn toast(text: &str, age: f64, bottom: f32) {
    const SHOW: f64 = 1.6;
    const FADE: f64 = 0.4;
    if age > SHOW {
        return;
    }
    let alpha = (((SHOW - age) / FADE).min(1.0)) as f32;
    let size = 16.0;
    let m = render::measure(text, size);
    let (w, h) = (m.width + 28.0, 32.0);
    let r = Rect::new((screen_width() - w) / 2.0, bottom - h - 12.0, w, h);
    render::rounded_rect(r, 8.0, Color::new(0.0, 0.0, 0.0, 0.7 * alpha));
    let baseline = r.y + (h + m.offset_y) / 2.0;
    render::text(
        text,
        r.x + 14.0,
        baseline,
        size,
        Color::new(1.0, 1.0, 1.0, alpha),
    );
}

pub fn format_time(secs: u32) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}
