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

const CARD_FACE: Color = Color::new(0.99, 0.99, 0.97, 1.0);
const CARD_EDGE: Color = Color::new(0.25, 0.25, 0.25, 1.0);
const RED: Color = Color::new(0.80, 0.08, 0.14, 1.0);
const BLACK_INK: Color = Color::new(0.08, 0.08, 0.10, 1.0);
pub const HIGHLIGHT: Color = Color::new(1.0, 0.85, 0.2, 0.9);
pub const HINT: Color = Color::new(0.35, 0.85, 1.0, 1.0);

pub struct Table {
    pub name: &'static str,
    pub felt: Color,
}

impl Table {
    fn slot(&self) -> Color {
        mix(self.felt, BLACK, 0.2)
    }

    fn mark(&self) -> Color {
        mix(self.felt, WHITE, 0.22)
    }
}

pub const TABLES: [Table; 4] = [
    Table {
        name: "Green",
        felt: Color::new(0.10, 0.40, 0.22, 1.0),
    },
    Table {
        name: "Blue",
        felt: Color::new(0.09, 0.24, 0.42, 1.0),
    },
    Table {
        name: "Burgundy",
        felt: Color::new(0.38, 0.09, 0.14, 1.0),
    },
    Table {
        name: "Slate",
        felt: Color::new(0.21, 0.24, 0.27, 1.0),
    },
];

pub enum Pattern {
    Lattice,
    Stripes,
    Dots,
}

pub struct Back {
    pub name: &'static str,
    base: Color,
    ink: Color,
    pattern: Pattern,
}

pub const BACKS: [Back; 5] = [
    Back {
        name: "Blue lattice",
        base: Color::new(0.14, 0.24, 0.55, 1.0),
        ink: Color::new(0.30, 0.42, 0.75, 1.0),
        pattern: Pattern::Lattice,
    },
    Back {
        name: "Red lattice",
        base: Color::new(0.62, 0.10, 0.14, 1.0),
        ink: Color::new(0.80, 0.30, 0.32, 1.0),
        pattern: Pattern::Lattice,
    },
    Back {
        name: "Green stripes",
        base: Color::new(0.10, 0.38, 0.26, 1.0),
        ink: Color::new(0.22, 0.52, 0.38, 1.0),
        pattern: Pattern::Stripes,
    },
    Back {
        name: "Plum dots",
        base: Color::new(0.34, 0.16, 0.42, 1.0),
        ink: Color::new(0.55, 0.36, 0.64, 1.0),
        pattern: Pattern::Dots,
    },
    Back {
        name: "Charcoal stripes",
        base: Color::new(0.18, 0.19, 0.21, 1.0),
        ink: Color::new(0.34, 0.35, 0.38, 1.0),
        pattern: Pattern::Stripes,
    },
];

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

/// Filled rounded rectangle, drawn as a triangle fan so no pixel is covered
/// twice (overlapping pieces would show darker seams with translucent colours).
pub fn rounded_rect(r: Rect, radius: f32, color: Color) {
    const SEGMENTS: usize = 6;
    let rad = radius.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    let center = r.center();
    // Corner centres, clockwise from top-right, with the angle each arc starts at.
    let corners = [
        (vec2(r.x + r.w - rad, r.y + rad), -PI / 2.0),
        (vec2(r.x + r.w - rad, r.y + r.h - rad), 0.0),
        (vec2(r.x + rad, r.y + r.h - rad), PI / 2.0),
        (vec2(r.x + rad, r.y + rad), PI),
    ];
    let mut prev: Option<Vec2> = None;
    let mut first = Vec2::ZERO;
    for (c, start) in corners {
        for i in 0..=SEGMENTS {
            let a = start + (i as f32 / SEGMENTS as f32) * (PI / 2.0);
            let p = c + vec2(a.cos(), a.sin()) * rad;
            match prev {
                Some(q) => draw_triangle(center, q, p, color),
                None => first = p,
            }
            prev = Some(p);
        }
    }
    if let Some(q) = prev {
        draw_triangle(center, q, first, color);
    }
}

fn corner_radius(r: Rect) -> f32 {
    r.w * 0.08
}

/// Empty pile placeholder.
pub fn draw_slot(r: Rect, label: Option<&str>, table: &Table) {
    rounded_rect(r, corner_radius(r), table.slot());
    if let Some(text) = label {
        let size = r.w * 0.4;
        let m = measure(text, size);
        self::text(
            text,
            r.x + (r.w - m.width) / 2.0,
            r.y + (r.h + m.offset_y) / 2.0,
            size,
            table.mark(),
        );
    }
}

/// Ring shown on an empty stock when clicking it will recycle the waste.
pub fn draw_recycle_mark(r: Rect, table: &Table) {
    let c = r.center();
    draw_circle_lines(c.x, c.y, r.w * 0.22, (r.w * 0.05).max(1.5), table.mark());
}

pub fn draw_outline(r: Rect, color: Color) {
    let pad = r.w * 0.04;
    rounded_rect(
        Rect::new(r.x - pad, r.y - pad, r.w + 2.0 * pad, r.h + 2.0 * pad),
        corner_radius(r) + pad,
        color,
    );
}

pub fn draw_card(card: Card, r: Rect, back: &Back) {
    let radius = corner_radius(r);
    let edge = (r.w * 0.012).max(1.0);
    rounded_rect(r, radius, CARD_EDGE);
    let inner = Rect::new(r.x + edge, r.y + edge, r.w - 2.0 * edge, r.h - 2.0 * edge);
    rounded_rect(inner, radius - edge, CARD_FACE);
    if card.face_up {
        draw_face(card, inner);
    } else {
        draw_back(inner, back);
    }
}

fn draw_back(r: Rect, back: &Back) {
    let pad = r.w * 0.07;
    let b = Rect::new(r.x + pad, r.y + pad, r.w - 2.0 * pad, r.h - 2.0 * pad);
    rounded_rect(b, r.w * 0.05, back.base);
    let thick = (r.w * 0.015).max(1.0);
    let ink = back.ink;
    match back.pattern {
        Pattern::Lattice => {
            let step = b.w / 6.0;
            let mut d = -b.h;
            while d < b.w {
                clipped_line(
                    b,
                    vec2(b.x + d, b.y),
                    vec2(b.x + d + b.h, b.y + b.h),
                    thick,
                    ink,
                );
                clipped_line(
                    b,
                    vec2(b.x + d + b.h, b.y),
                    vec2(b.x + d, b.y + b.h),
                    thick,
                    ink,
                );
                d += step;
            }
        }
        Pattern::Stripes => {
            let step = b.w / 7.0;
            let mut d = -b.h;
            while d < b.w {
                clipped_line(
                    b,
                    vec2(b.x + d, b.y),
                    vec2(b.x + d + b.h, b.y + b.h),
                    thick * 3.0,
                    ink,
                );
                d += step;
            }
        }
        Pattern::Dots => {
            let step = b.w / 6.0;
            let radius = step * 0.22;
            let rows = (b.h / step).floor().max(1.0) as i32;
            let y0 = b.y + (b.h - (rows - 1) as f32 * step) / 2.0;
            for row in 0..rows {
                // Alternate rows are offset by half a step.
                let shift = if row % 2 == 0 { 0.5 } else { 1.0 };
                let mut x = b.x + step * (shift - 0.25);
                while x < b.x + b.w - radius {
                    draw_circle(x, y0 + row as f32 * step, radius, ink);
                    x += step;
                }
            }
        }
    }
}

/// Draws the part of a line segment that falls inside `r`.
fn clipped_line(r: Rect, a: Vec2, b: Vec2, thick: f32, color: Color) {
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
        draw_line(s.x, s.y, e.x, e.y, thick, color);
    }
}

fn ink(card: Card) -> Color {
    match card.color() {
        soli_core::Color::Red => RED,
        soli_core::Color::Black => BLACK_INK,
    }
}

/// Pip positions for 2..=10 on a grid where x is the column (-1, 0, 1) and y
/// runs from -1 (top row) to 1 (bottom row). Pips below the middle are drawn
/// upside down, as on printed cards.
#[rustfmt::skip]
fn pip_layout(rank: u8) -> &'static [(f32, f32)] {
    const T: f32 = 1.0 / 3.0;
    match rank {
        2 => &[(0.0, -1.0), (0.0, 1.0)],
        3 => &[(0.0, -1.0), (0.0, 0.0), (0.0, 1.0)],
        4 => &[(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)],
        5 => &[(-1.0, -1.0), (1.0, -1.0), (0.0, 0.0), (-1.0, 1.0), (1.0, 1.0)],
        6 => &[(-1.0, -1.0), (1.0, -1.0), (-1.0, 0.0), (1.0, 0.0), (-1.0, 1.0), (1.0, 1.0)],
        7 => &[(-1.0, -1.0), (1.0, -1.0), (0.0, -0.5),
               (-1.0, 0.0), (1.0, 0.0),
               (-1.0, 1.0), (1.0, 1.0)],
        8 => &[(-1.0, -1.0), (1.0, -1.0), (0.0, -0.5),
               (-1.0, 0.0), (1.0, 0.0),
               (0.0, 0.5), (-1.0, 1.0), (1.0, 1.0)],
        9 => &[(-1.0, -1.0), (1.0, -1.0), (-1.0, -T), (1.0, -T),
               (0.0, 0.0),
               (-1.0, T), (1.0, T), (-1.0, 1.0), (1.0, 1.0)],
        10 => &[(-1.0, -1.0), (1.0, -1.0), (0.0, -2.0 * T), (-1.0, -T), (1.0, -T),
                (-1.0, T), (1.0, T), (0.0, 2.0 * T), (-1.0, 1.0), (1.0, 1.0)],
        _ => &[],
    }
}

fn draw_face(card: Card, r: Rect) {
    let color = ink(card);
    let label = card.rank_label();
    let font_size = r.w * 0.2;
    let m = measure(label, font_size);
    let pad = r.w * 0.05;
    let small = r.w * 0.12;

    // Corner indices: rank with a small suit beneath it, repeated upside down
    // in the opposite corner. Centred on a fixed column so "10" lines up.
    let column = r.w * 0.08;
    let index_cx = r.x + pad + column;
    let text_y = r.y + pad + m.offset_y;
    text(label, index_cx - m.width / 2.0, text_y, font_size, color);
    draw_suit(
        card.suit,
        vec2(index_cx, text_y + small * 0.85),
        small,
        color,
    );

    let br_cx = r.x + r.w - pad - column;
    let br_y = r.y + r.h - pad - m.offset_y;
    text_rotated(label, br_cx + m.width / 2.0, br_y, font_size, color, PI);
    draw_suit_flipped(card.suit, vec2(br_cx, br_y - small * 0.85), small, color);

    let center = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
    match card.rank {
        1 => draw_suit(card.suit, center, r.w * 0.45, color),
        2..=10 => {
            let (dx, dy) = (r.w * 0.17, r.h * 0.29);
            let size = r.w * 0.15;
            for &(px, py) in pip_layout(card.rank) {
                let at = center + vec2(px * dx, py * dy);
                if py > 0.0 {
                    draw_suit_flipped(card.suit, at, size, color);
                } else {
                    draw_suit(card.suit, at, size, color);
                }
            }
        }
        _ => {
            // Court cards: a tinted, framed panel with the rank and suit.
            let frame = Rect::new(r.x + r.w * 0.24, r.y + r.h * 0.17, r.w * 0.52, r.h * 0.66);
            let tint = Color::new(color.r, color.g, color.b, 0.08);
            draw_rectangle(frame.x, frame.y, frame.w, frame.h, tint);
            draw_rectangle_lines(
                frame.x,
                frame.y,
                frame.w,
                frame.h,
                (r.w * 0.02).max(1.0),
                color,
            );
            let big = r.w * 0.36;
            let bm = measure(label, big);
            let baseline = center.y + bm.offset_y / 2.0 - r.h * 0.07;
            text(label, center.x - bm.width / 2.0, baseline, big, color);
            draw_suit(
                card.suit,
                vec2(center.x, center.y + r.h * 0.15),
                r.w * 0.2,
                color,
            );
        }
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
