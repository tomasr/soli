//! Generic chrome: panels, text rows, keycaps and transient messages.

use crate::render::{self, MIST, OCHRE, PAPER, TEXT, alpha, rgb};
use macroquad::prelude::*;

const PANEL: Color = rgb(0x13, 0x18, 0x16);
const PANEL_EDGE: Color = alpha(PAPER, 0.08);
const SCRIM: Color = Color::new(0.03, 0.04, 0.035, 0.6);
pub const DIM: Color = MIST;
pub const ACCENT: Color = OCHRE;
pub const FG: Color = TEXT;

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
    render::rounded_rect(r, 18.0, PANEL_EDGE);
    render::rounded_rect(
        Rect::new(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0),
        17.0,
        PANEL,
    );
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

/// Baseline that vertically centres text of `size` on `cy`.
pub fn baseline_for(cy: f32, size: f32) -> f32 {
    cy + render::measure("M", size).offset_y / 2.0
}

const KEY_SIZE: f32 = 12.0;
const KEY_H: f32 = 22.0;

fn keycap_width(key: &str) -> f32 {
    (render::measure(key, KEY_SIZE).width + 12.0).max(KEY_H)
}

/// Draws a key label in a small rounded box with its left edge at `x`,
/// centred on `cy`. Returns its width.
pub fn keycap(key: &str, x: f32, cy: f32, tint: Color) -> f32 {
    let w = keycap_width(key);
    let r = Rect::new(x, cy - KEY_H / 2.0, w, KEY_H);
    render::rounded_rect(r, 6.0, alpha(tint, 0.08));
    render::rounded_rect_lines(r, 6.0, 1.0, alpha(tint, 0.3));
    centered_text(
        key,
        r.center().x,
        baseline_for(cy, KEY_SIZE),
        KEY_SIZE,
        tint,
    );
    w
}

const HINT_SIZE: f32 = 14.0;
const HINT_KEY_GAP: f32 = 8.0;
const HINT_GAP: f32 = 20.0;

/// Width of a row of "key  label" hints.
pub fn key_hints_width(hints: &[(&str, &str)]) -> f32 {
    let items: f32 = hints
        .iter()
        .map(|(k, l)| keycap_width(k) + HINT_KEY_GAP + render::measure(l, HINT_SIZE).width)
        .sum();
    items + HINT_GAP * hints.len().saturating_sub(1) as f32
}

/// Draws a row of "key  label" hints starting at `x`, centred on `cy`.
pub fn key_hints(hints: &[(&str, &str)], x: f32, cy: f32) {
    let baseline = baseline_for(cy, HINT_SIZE);
    let mut x = x;
    for (key, label) in hints {
        x += keycap(key, x, cy, FG) + HINT_KEY_GAP;
        render::text(label, x, baseline, HINT_SIZE, DIM);
        x += render::measure(label, HINT_SIZE).width + HINT_GAP;
    }
}

/// Short message shown above the status bar, fading out over its last part.
pub fn toast(text: &str, age: f64, bottom: f32) {
    const SHOW: f64 = 1.6;
    const FADE: f64 = 0.4;
    if age > SHOW {
        return;
    }
    let a = (((SHOW - age) / FADE).min(1.0)) as f32;
    let size = 15.0;
    let m = render::measure(text, size);
    let (w, h) = (m.width + 32.0, 34.0);
    let r = Rect::new((screen_width() - w) / 2.0, bottom - h - 14.0, w, h);
    render::rounded_rect(r, 10.0, alpha(PANEL, 0.95 * a));
    let baseline = r.y + (h + m.offset_y) / 2.0;
    render::text(text, r.x + 16.0, baseline, size, alpha(FG, a));
}

pub fn format_time(secs: u32) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}
