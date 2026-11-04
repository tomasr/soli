//! Card drawing. Cards are built from primitives; the only asset is the
//! embedded UI font.

use macroquad::prelude::*;
use soli_core::{Card, Suit};
use std::f32::consts::PI;
use std::sync::OnceLock;

static FONT: OnceLock<Font> = OnceLock::new();

/// Loads the embedded font. Must run once, inside the macroquad context, before
/// anything draws text.
pub fn init_font() {
    let font = load_ttf_font_from_bytes(include_bytes!("../assets/fonts/Inter-SemiBold.ttf"))
        .expect("embedded font is valid");
    let _ = FONT.set(font);
}

fn font() -> &'static Font {
    FONT.get().expect("render::init_font was called")
}

/// Glyphs are rasterised per pixel size. Coordinates are logical pixels, so
/// rasterise at the physical size and scale back down to keep text crisp on
/// high-DPI displays. Returns (font_size, font_scale).
fn font_px(size: f32) -> (u16, f32) {
    let dpi = screen_dpi_scale();
    let px = (size * dpi).round().max(1.0);
    (px as u16, size / px)
}

pub fn measure(text: &str, size: f32) -> TextDimensions {
    let (px, scale) = font_px(size);
    measure_text(text, Some(font()), px, scale)
}

/// Draws `text` with its baseline at `y`.
pub fn text(text: &str, x: f32, y: f32, size: f32, color: Color) {
    text_rotated(text, x, y, size, color, 0.0);
}

fn text_rotated(text: &str, x: f32, y: f32, size: f32, color: Color, rotation: f32) {
    let (px, scale) = font_px(size);
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font: Some(font()),
            font_size: px,
            font_scale: scale,
            rotation,
            color,
            ..Default::default()
        },
    );
}

pub const TABLE: Color = Color::new(0.10, 0.40, 0.22, 1.0);
const CARD_FACE: Color = Color::new(0.99, 0.99, 0.97, 1.0);
const CARD_EDGE: Color = Color::new(0.25, 0.25, 0.25, 1.0);
const RED: Color = Color::new(0.80, 0.08, 0.14, 1.0);
const BLACK_INK: Color = Color::new(0.08, 0.08, 0.10, 1.0);
const BACK: Color = Color::new(0.14, 0.24, 0.55, 1.0);
const BACK_PATTERN: Color = Color::new(0.30, 0.42, 0.75, 1.0);
const SLOT: Color = Color::new(0.08, 0.33, 0.18, 1.0);
const SLOT_MARK: Color = Color::new(0.30, 0.52, 0.38, 1.0);
pub const HIGHLIGHT: Color = Color::new(1.0, 0.85, 0.2, 0.9);

pub fn rounded_rect(r: Rect, radius: f32, color: Color) {
    let rad = radius.min(r.w / 2.0).min(r.h / 2.0);
    draw_rectangle(r.x + rad, r.y, r.w - 2.0 * rad, r.h, color);
    draw_rectangle(r.x, r.y + rad, r.w, r.h - 2.0 * rad, color);
    for (cx, cy) in [
        (r.x + rad, r.y + rad),
        (r.x + r.w - rad, r.y + rad),
        (r.x + rad, r.y + r.h - rad),
        (r.x + r.w - rad, r.y + r.h - rad),
    ] {
        draw_circle(cx, cy, rad, color);
    }
}

fn corner_radius(r: Rect) -> f32 {
    r.w * 0.08
}

/// Empty pile placeholder.
pub fn draw_slot(r: Rect, label: Option<&str>) {
    rounded_rect(r, corner_radius(r), SLOT);
    if let Some(text) = label {
        let size = r.w * 0.4;
        let m = measure(text, size);
        self::text(
            text,
            r.x + (r.w - m.width) / 2.0,
            r.y + (r.h + m.offset_y) / 2.0,
            size,
            SLOT_MARK,
        );
    }
}

/// Ring shown on an empty stock when clicking it will recycle the waste.
pub fn draw_recycle_mark(r: Rect) {
    let c = r.center();
    draw_circle_lines(c.x, c.y, r.w * 0.22, (r.w * 0.05).max(1.5), SLOT_MARK);
}

pub fn draw_outline(r: Rect, color: Color) {
    let pad = r.w * 0.04;
    rounded_rect(
        Rect::new(r.x - pad, r.y - pad, r.w + 2.0 * pad, r.h + 2.0 * pad),
        corner_radius(r) + pad,
        color,
    );
}

pub fn draw_card(card: Card, r: Rect) {
    let radius = corner_radius(r);
    let edge = (r.w * 0.012).max(1.0);
    rounded_rect(r, radius, CARD_EDGE);
    let inner = Rect::new(r.x + edge, r.y + edge, r.w - 2.0 * edge, r.h - 2.0 * edge);
    if card.face_up {
        rounded_rect(inner, radius - edge, CARD_FACE);
        draw_face(card, inner);
    } else {
        rounded_rect(inner, radius - edge, CARD_FACE);
        draw_back(inner);
    }
}

fn draw_back(r: Rect) {
    let pad = r.w * 0.07;
    let b = Rect::new(r.x + pad, r.y + pad, r.w - 2.0 * pad, r.h - 2.0 * pad);
    rounded_rect(b, r.w * 0.05, BACK);
    // Diamond lattice.
    let step = b.w / 6.0;
    let thick = (r.w * 0.015).max(1.0);
    let mut d = -b.h;
    while d < b.w {
        clipped_line(b, vec2(b.x + d, b.y), vec2(b.x + d + b.h, b.y + b.h), thick);
        clipped_line(b, vec2(b.x + d + b.h, b.y), vec2(b.x + d, b.y + b.h), thick);
        d += step;
    }
}

/// Draws the part of a line segment that falls inside `r`.
fn clipped_line(r: Rect, a: Vec2, b: Vec2, thick: f32) {
    // Liang-Barsky clipping.
    let d = b - a;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [
        (-d.x, a.x - r.x),
        (d.x, r.x + r.w - a.x),
        (-d.y, a.y - r.y),
        (d.y, r.y + r.h - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    if t0 < t1 {
        let (s, e) = (a + d * t0, a + d * t1);
        draw_line(s.x, s.y, e.x, e.y, thick, BACK_PATTERN);
    }
}

fn ink(card: Card) -> Color {
    match card.color() {
        soli_core::Color::Red => RED,
        soli_core::Color::Black => BLACK_INK,
    }
}

fn draw_face(card: Card, r: Rect) {
    let color = ink(card);
    let label = card.rank_label();
    let font_size = r.w * 0.24;
    let m = measure(label, font_size);
    let pad = r.w * 0.08;

    // Top-left index: rank with a small suit beneath it.
    let text_x = r.x + pad;
    let text_y = r.y + pad + m.offset_y;
    text(label, text_x, text_y, font_size, color);
    let small = r.w * 0.16;
    let index_cx = text_x + m.width / 2.0;
    draw_suit(card.suit, vec2(index_cx, text_y + small * 0.8), small, color);

    // Bottom-right index, rotated 180 degrees.
    let br = vec2(r.x + r.w - pad, r.y + r.h - pad - m.offset_y);
    text_rotated(label, br.x, br.y, font_size, color, PI);
    draw_suit_flipped(card.suit, vec2(br.x - m.width / 2.0, br.y - small * 0.8), small, color);

    let center = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
    if card.rank > 10 {
        // Court cards: framed letter with a suit on either side.
        let frame = Rect::new(r.x + r.w * 0.25, r.y + r.h * 0.2, r.w * 0.5, r.h * 0.6);
        draw_rectangle_lines(frame.x, frame.y, frame.w, frame.h, (r.w * 0.02).max(1.0), color);
        let big = r.w * 0.36;
        let bm = measure(label, big);
        text(label, center.x - bm.width / 2.0, center.y + bm.offset_y / 2.0 - r.h * 0.06, big, color);
        draw_suit(card.suit, vec2(center.x, center.y + r.h * 0.15), r.w * 0.2, color);
    } else {
        draw_suit(card.suit, center + vec2(0.0, r.h * 0.04), r.w * 0.42, color);
    }
}

pub fn draw_suit(suit: Suit, c: Vec2, size: f32, color: Color) {
    draw_shape(suit, c, size, color, false);
}

fn draw_suit_flipped(suit: Suit, c: Vec2, size: f32, color: Color) {
    draw_shape(suit, c, size, color, true);
}

/// Draws a suit symbol centred on `c` with overall height `s`, optionally
/// upside down.
fn draw_shape(suit: Suit, c: Vec2, s: f32, color: Color, flip: bool) {
    let fy = if flip { -1.0 } else { 1.0 };
    let p = |x: f32, y: f32| vec2(c.x + x * s, c.y + y * s * fy);
    let circle = |x: f32, y: f32, r: f32| {
        let at = p(x, y);
        draw_circle(at.x, at.y, r * s, color);
    };
    match suit {
        Suit::Hearts => {
            circle(-0.25, -0.18, 0.26);
            circle(0.25, -0.18, 0.26);
            draw_triangle(p(-0.49, -0.08), p(0.49, -0.08), p(0.0, 0.48), color);
        }
        Suit::Spades => {
            circle(-0.23, 0.1, 0.25);
            circle(0.23, 0.1, 0.25);
            draw_triangle(p(-0.47, 0.02), p(0.47, 0.02), p(0.0, -0.48), color);
            draw_triangle(p(-0.05, 0.1), p(-0.17, 0.5), p(0.17, 0.5), color);
            draw_triangle(p(-0.05, 0.1), p(0.05, 0.1), p(0.17, 0.5), color);
        }
        Suit::Diamonds => {
            draw_triangle(p(0.0, -0.5), p(0.36, 0.0), p(-0.36, 0.0), color);
            draw_triangle(p(0.0, 0.5), p(0.36, 0.0), p(-0.36, 0.0), color);
        }
        Suit::Clubs => {
            circle(0.0, -0.24, 0.21);
            circle(-0.24, 0.05, 0.21);
            circle(0.24, 0.05, 0.21);
            circle(0.0, 0.0, 0.15);
            draw_triangle(p(-0.05, 0.0), p(-0.16, 0.5), p(0.16, 0.5), color);
            draw_triangle(p(-0.05, 0.0), p(0.05, 0.0), p(0.16, 0.5), color);
        }
    }
}
