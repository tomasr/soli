//! Card drawing and the palette. Cards are built from primitives; the only
//! asset is the embedded UI font.

use crate::art::{self, Animal};
use macroquad::prelude::*;
use soli_core::{Card, Suit};
use std::f32::consts::PI;
use std::sync::OnceLock;

static FONT: OnceLock<Font> = OnceLock::new();

/// Loads the embedded font. Must run once, inside the macroquad context, before
/// anything draws text.
pub fn init_font() {
    let font = load_ttf_font_from_bytes(include_bytes!("../assets/fonts/InterTight-SemiBold.ttf"))
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
    let (px, scale) = font_px(size);
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font: Some(font()),
            font_size: px,
            font_scale: scale,
            color,
            ..Default::default()
        },
    );
}

/// Builds an opaque colour from 8-bit channels.
pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}

/// Same colour with a different alpha.
pub const fn alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a)
}

// The palette: a quiet, dark table so the cards carry the colour.
/// Card faces and the light end of every contrast pair.
pub const PAPER: Color = rgb(0xF2, 0xEE, 0xE6);
/// Spades and clubs.
pub const INK: Color = rgb(0x1D, 0x1F, 0x1E);
/// Hearts and diamonds.
pub const BRICK: Color = rgb(0xA8, 0x43, 0x2F);
/// Hints, selection and records.
pub const OCHRE: Color = rgb(0xD9, 0xA4, 0x41);
/// Primary text on the table.
pub const TEXT: Color = rgb(0xEC, 0xE8, 0xDF);
/// Secondary text on the table.
pub const MIST: Color = rgb(0x9D, 0xA5, 0x9E);

pub const HIGHLIGHT: Color = alpha(PAPER, 0.55);
pub const HINT: Color = OCHRE;
const CARD_SHADOW: Color = Color::new(0.0, 0.0, 0.0, 0.3);
const SLOT_LINE: Color = alpha(PAPER, 0.16);
const SLOT_MARK: Color = alpha(PAPER, 0.22);

pub struct Table {
    pub name: &'static str,
    pub felt: Color,
}

pub const TABLES: [Table; 3] = [
    Table {
        name: "Moss",
        felt: rgb(0x1E, 0x2A, 0x23),
    },
    Table {
        name: "Slate",
        felt: rgb(0x1C, 0x22, 0x28),
    },
    Table {
        name: "Umber",
        felt: rgb(0x2A, 0x22, 0x1E),
    },
];

pub struct Back {
    pub name: &'static str,
    base: Color,
    line: Color,
    animal: Animal,
}

pub const BACKS: [Back; 5] = [
    Back {
        name: "Fox",
        base: rgb(0xB0, 0x57, 0x3A),
        line: PAPER,
        animal: Animal::Fox,
    },
    Back {
        name: "Heron",
        base: rgb(0x4C, 0x67, 0x79),
        line: PAPER,
        animal: Animal::Heron,
    },
    Back {
        name: "Owl",
        base: rgb(0xC2, 0x9A, 0x48),
        line: INK,
        animal: Animal::Owl,
    },
    Back {
        name: "Whale",
        base: rgb(0x2E, 0x5B, 0x57),
        line: PAPER,
        animal: Animal::Whale,
    },
    Back {
        name: "Hare",
        base: rgb(0x6D, 0x54, 0x65),
        line: PAPER,
        animal: Animal::Hare,
    },
];

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

/// Outline of a rounded rectangle.
pub fn rounded_rect_lines(r: Rect, radius: f32, thickness: f32, color: Color) {
    const SEGMENTS: usize = 6;
    let rad = radius.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    let corners = [
        (vec2(r.x + r.w - rad, r.y + rad), -PI / 2.0),
        (vec2(r.x + r.w - rad, r.y + r.h - rad), 0.0),
        (vec2(r.x + rad, r.y + r.h - rad), PI / 2.0),
        (vec2(r.x + rad, r.y + rad), PI),
    ];
    let points: Vec<Vec2> = corners
        .iter()
        .flat_map(|&(c, start)| {
            (0..=SEGMENTS).map(move |i| {
                let a = start + (i as f32 / SEGMENTS as f32) * (PI / 2.0);
                c + vec2(a.cos(), a.sin()) * rad
            })
        })
        .collect();
    for (i, &a) in points.iter().enumerate() {
        let b = points[(i + 1) % points.len()];
        draw_line(a.x, a.y, b.x, b.y, thickness, color);
    }
}

/// Empty pile placeholder: a faint outline with an optional letter.
pub fn draw_slot(r: Rect, label: Option<&str>) {
    rounded_rect_lines(r, corner_radius(r), (r.w * 0.014).max(1.0), SLOT_LINE);
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
    draw_circle_lines(c.x, c.y, r.w * 0.22, (r.w * 0.04).max(1.5), SLOT_MARK);
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
    // A hard drop edge reads as card thickness and separates stacked cards.
    let drop = (r.w * 0.014).max(1.0);
    rounded_rect(Rect::new(r.x, r.y + drop, r.w, r.h), radius, CARD_SHADOW);
    if card.face_up {
        rounded_rect(r, radius, PAPER);
        draw_face(card, r);
    } else {
        draw_back(r, back);
    }
}

fn draw_back(r: Rect, back: &Back) {
    let radius = corner_radius(r);
    rounded_rect(r, radius, back.base);
    let inset = (r.w * 0.06).max(3.0);
    let frame = Rect::new(
        r.x + inset,
        r.y + inset,
        r.w - 2.0 * inset,
        r.h - 2.0 * inset,
    );
    rounded_rect_lines(
        frame,
        (radius - inset + 2.0).max(2.0),
        (r.w * 0.01).max(1.0),
        alpha(back.line, 0.35),
    );
    art::draw_back(back.animal, r, (r.w * 0.018).max(1.0), back.line);
}

fn ink(card: Card) -> Color {
    match card.color() {
        soli_core::Color::Red => BRICK,
        soli_core::Color::Black => INK,
    }
}

fn draw_face(card: Card, r: Rect) {
    let color = ink(card);
    let label = card.rank_label();

    // Corner index: rank and suit side by side in the top-left, sized to
    // read in the strip a fanned card leaves visible.
    let pad_x = r.w * 0.096;
    let pad_y = r.w * 0.08;
    let rank_size = r.w * 0.24;
    let m = measure(label, rank_size);
    let baseline = r.y + pad_y + m.offset_y;
    text(label, r.x + pad_x, baseline, rank_size, color);
    let suit_size = r.w * 0.15;
    draw_suit(
        card.suit,
        vec2(
            r.x + pad_x + m.width + r.w * 0.03 + suit_size * 0.48,
            baseline - suit_size * 0.5,
        ),
        suit_size,
        color,
    );

    match card.rank {
        1 => {
            let size = r.w * 0.56;
            draw_suit(card.suit, vec2(r.center().x, r.y + r.h * 0.56), size, color);
        }
        2..=10 => {
            // One large suit sits in the bottom-right corner.
            let size = r.w * 0.44;
            let pad = r.w * 0.1;
            let c = vec2(r.x + r.w - pad - size * 0.48, r.y + r.h - pad - size * 0.5);
            draw_suit(card.suit, c, size, color);
        }
        _ => {
            // Court cards carry an animal in the suit colour.
            let animal = match card.rank {
                11 => Animal::Fox,
                12 => Animal::Heron,
                _ => Animal::Owl,
            };
            let art = Rect::new(r.x + r.w * 0.17, r.y + r.h * 0.3, r.w * 0.66, r.h * 0.6);
            art::draw_icon(animal, art, (r.w * 0.02).max(1.0), color);
        }
    }
}

/// Draws a suit symbol centred on `c` with overall height `s`.
pub fn draw_suit(suit: Suit, c: Vec2, s: f32, color: Color) {
    let p = |x: f32, y: f32| vec2(c.x + x * s, c.y + y * s);
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
