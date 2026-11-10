//! Card drawing and the palette. Cards are built from primitives; the only
//! asset is the embedded UI font.
//!
//! Building a card from primitives takes thousands of triangles (the animal
//! line art most of all), far too many to redo every frame. So every face and
//! back is painted once into an atlas texture at the current card size, and a
//! card on the table is a single textured quad.

use crate::art::{self, Animal};
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::prelude::*;
use soli_core::{Card, Suit};
use std::cell::RefCell;
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

/// Draws a card from the atlas. `prepare_cards` must have been called for
/// this card size at the start of the frame; otherwise the card is painted
/// from primitives.
pub fn draw_card(card: Card, r: Rect, back: &Back) {
    draw_atlas_card(card, r, back, false);
}

/// Draws a card under `COVERED_SHADE`.
pub fn draw_covered_card(card: Card, r: Rect, back: &Back) {
    draw_atlas_card(card, r, back, true);
}

fn draw_atlas_card(card: Card, r: Rect, back: &Back, covered: bool) {
    let index = if card.face_up {
        Some(face_index(card))
    } else {
        BACKS
            .iter()
            .position(|b| b.name == back.name)
            .map(|i| 52 + i)
    };
    let index = index.map(|i| if covered { i + ATLAS_CARDS } else { i });
    if !index.is_some_and(|i| draw_cell(i, r)) {
        paint_card(card, r, back, covered);
    }
}

/// Draws one atlas cell for a card at `r`. Returns false if the atlas doesn't
/// hold cards of that size.
fn draw_cell(index: usize, r: Rect) -> bool {
    CARDS.with_borrow(|cards| {
        let Some(atlas) = cards.atlas.as_ref() else {
            return false;
        };
        let dpi = screen_dpi_scale();
        if index >= atlas.cells || atlas.card != physical_size(vec2(r.w, r.h), dpi) {
            return false;
        }
        let (cw, ch) = atlas.cell;
        let (col, row) = (index as u32 % ATLAS_COLS, index as u32 / ATLAS_COLS);
        // Land the card on whole physical pixels so atlas texels map 1:1.
        let x = (r.x * dpi).round() - ATLAS_PAD as f32;
        let y = (r.y * dpi).round() - ATLAS_PAD as f32;
        draw_texture_ex(
            &atlas.target.texture,
            x / dpi,
            y / dpi,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(cw as f32 / dpi, ch as f32 / dpi)),
                source: Some(Rect::new(
                    (col * cw) as f32,
                    (row * ch) as f32,
                    cw as f32,
                    ch as f32,
                )),
                ..Default::default()
            },
        );
        true
    })
}

/// Paints a card from primitives.
fn paint_card(card: Card, r: Rect, back: &Back, covered: bool) {
    let radius = corner_radius(r);
    let drop = drop_edge(r.w);
    rounded_rect(Rect::new(r.x, r.y + drop, r.w, r.h), radius, CARD_SHADOW);
    if card.face_up {
        rounded_rect(r, radius, PAPER);
        // The frame's top edge is what tells fanned face-up cards apart. It
        // sits closer to the edge than the back's, to clear the corner index.
        inner_frame(r, r.w * 0.045, alpha(INK, 0.16));
        draw_face(card, r);
    } else {
        draw_back(r, back);
    }
    // Baked into the atlas rather than drawn over an atlas card: two
    // antialiased edges blended on top of each other leave a light fringe
    // that multisampling, at paint time, does not.
    if covered {
        rounded_rect(r, radius, COVERED_SHADE);
    }
}

/// Shade over layout cards that have a card on top of them. Cards held back
/// only by their neighbours are left unshaded, so the player has to spot
/// those.
const COVERED_SHADE: Color = Color::new(0.0, 0.0, 0.0, 0.32);

/// A hard drop edge under each card reads as card thickness and separates
/// stacked cards.
fn drop_edge(card_w: f32) -> f32 {
    (card_w * 0.014).max(1.0)
}

/// Cards per row in the atlas, and the transparent margin around each card
/// so its antialiased edge isn't cut off.
const ATLAS_COLS: u32 = 12;
const ATLAS_PAD: u32 = 2;

/// The atlas holds the 52 faces, then every back; then, for games that shade
/// covered cards, all of them again under `COVERED_SHADE`.
const ATLAS_CARDS: usize = 52 + BACKS.len();

fn face_index(card: Card) -> usize {
    let suit = Suit::ALL
        .iter()
        .position(|&s| s == card.suit)
        .expect("suit is listed");
    suit * 13 + (card.rank as usize - 1)
}

fn physical_size(card: Vec2, dpi: f32) -> (u32, u32) {
    (
        (card.x * dpi).round().max(1.0) as u32,
        (card.y * dpi).round().max(1.0) as u32,
    )
}

struct Atlas {
    /// Card size in physical pixels.
    card: (u32, u32),
    /// Cell size in physical pixels: the card, its drop edge and padding.
    cell: (u32, u32),
    /// How many cards it holds.
    cells: usize,
    /// Kept whole: dropping a render target's pass before the frame that
    /// draws into it has run loses the drawing.
    target: RenderTarget,
    /// The multisampled target the cards were painted into, a row at a time.
    /// It must outlive the frame that copies it into `target`, and is
    /// dropped after that.
    painted: Option<RenderTarget>,
}

struct Cards {
    atlas: Option<Atlas>,
    /// Paints into the atlas, leaving it with premultiplied alpha. Blending
    /// the alpha channel the ordinary way would square the alpha of anything
    /// translucent, such as the drop edge.
    paint: Material,
    /// Copies the painted atlas into its final texture with straight alpha,
    /// which is what macroquad's default material blends. Drawing the atlas
    /// with the default material, rather than a premultiplied one, keeps a
    /// table of cards in one batch: macroquad starts a new draw call, with a
    /// render pass of its own, at every material change.
    unpremultiply: Material,
    /// Writes colour and alpha as given, to clear part of a target.
    replace: Material,
}

thread_local! {
    static CARDS: RefCell<Cards> = RefCell::new(Cards {
        atlas: None,
        paint: card_material(
            PAINT_FRAGMENT,
            Some(BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::SourceAlpha),
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )),
            Some(BlendState::new(
                Equation::Add,
                BlendFactor::One,
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )),
        ),
        unpremultiply: card_material(UNPREMULTIPLY_FRAGMENT, None, None),
        replace: card_material(PAINT_FRAGMENT, None, None),
    });
}

/// macroquad's default shaders.
const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;

varying lowp vec2 uv;
varying lowp vec4 color;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}"#;

const PAINT_FRAGMENT: &str = r#"#version 100
varying lowp vec4 color;
varying lowp vec2 uv;

uniform sampler2D Texture;

void main() {
    gl_FragColor = color * texture2D(Texture, uv);
}"#;

const UNPREMULTIPLY_FRAGMENT: &str = r#"#version 100
varying lowp vec4 color;
varying lowp vec2 uv;

uniform sampler2D Texture;

void main() {
    mediump vec4 c = texture2D(Texture, uv);
    gl_FragColor = c.a > 0.0 ? vec4(c.rgb / c.a, c.a) : vec4(0.0);
}"#;

fn card_material(
    fragment: &str,
    color_blend: Option<BlendState>,
    alpha_blend: Option<BlendState>,
) -> Material {
    load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment,
        },
        MaterialParams {
            pipeline_params: PipelineParams {
                color_blend,
                alpha_blend,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("card shaders compile")
}

/// Makes sure the atlas holds cards of `card` size (logical pixels) at the
/// current display scale, repainting it if not. Call at the start of a frame,
/// before anything is drawn: painting the atlas clears pending draw calls.
/// `covered` adds the shaded cards that `draw_covered_card` needs, doubling
/// the atlas, so only games that shade cards ask for them.
pub fn prepare_cards(card: Vec2, covered: bool) {
    let dpi = screen_dpi_scale();
    let size = physical_size(card, dpi);
    let cells = if covered { 2 } else { 1 } * ATLAS_CARDS;
    CARDS.with_borrow_mut(|cards| {
        if let Some(atlas) = cards.atlas.as_mut()
            && atlas.card == size
            && atlas.cells == cells
        {
            atlas.painted = None;
            return;
        }
        cards.atlas = None;
        cards.atlas = Some(paint_atlas(size, cells, dpi, cards));
    });
}

fn paint_atlas(card: (u32, u32), cells: usize, dpi: f32, cards: &Cards) -> Atlas {
    let (w, h) = card;
    let logical = vec2(w as f32 / dpi, h as f32 / dpi);
    let drop = (drop_edge(logical.x) * dpi).ceil() as u32;
    let cell = (w + 2 * ATLAS_PAD, h + drop + 2 * ATLAS_PAD);
    let rows = (cells as u32).div_ceil(ATLAS_COLS);
    let (aw, ah) = (ATLAS_COLS * cell.0, rows * cell.1);
    let target = render_target(aw, ah);
    target.texture.set_filter(FilterMode::Linear);
    // Cards are painted a row at a time, so the multisampled target (four
    // samples a pixel) only has to hold one row.
    let painted = render_target_msaa(aw, cell.1);

    // Work in logical units, so the painting code and font sizing are the same
    // as on screen. A positive y zoom keeps the texture the right way up.
    let (lw, lh) = (aw as f32 / dpi, ah as f32 / dpi);
    let row_h = cell.1 as f32 / dpi;
    let camera = |into: &RenderTarget, h: f32| Camera2D {
        target: vec2(lw / 2.0, h / 2.0),
        zoom: vec2(2.0 / lw, 2.0 / h),
        render_target: Some(into.clone()),
        ..Default::default()
    };
    for row in 0..rows {
        set_camera(&camera(&painted, row_h));
        // Not clear_background, which would drop the rows queued so far.
        gl_use_material(&cards.replace);
        draw_rectangle(0.0, 0.0, lw, row_h, Color::new(0.0, 0.0, 0.0, 0.0));
        gl_use_material(&cards.paint);
        for col in 0..ATLAS_COLS {
            let index = (row * ATLAS_COLS + col) as usize;
            if index >= cells {
                break;
            }
            let r = Rect::new(
                (col * cell.0 + ATLAS_PAD) as f32 / dpi,
                ATLAS_PAD as f32 / dpi,
                logical.x,
                logical.y,
            );
            let (i, covered) = (index % ATLAS_CARDS, index >= ATLAS_CARDS);
            if i < 52 {
                let card = Card::new(i as u8 % 13 + 1, Suit::ALL[i / 13]).turned_up();
                paint_card(card, r, &BACKS[0], covered);
            } else {
                paint_card(Card::new(1, Suit::Spades), r, &BACKS[i - 52], covered);
            }
        }

        set_camera(&camera(&target, lh));
        gl_use_material(&cards.unpremultiply);
        draw_texture_ex(
            &painted.texture,
            0.0,
            row as f32 * row_h,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(lw, row_h)),
                ..Default::default()
            },
        );
    }
    gl_use_default_material();
    set_default_camera();
    Atlas {
        card,
        cell,
        cells,
        target,
        painted: Some(painted),
    }
}

/// A thin rounded frame `inset` inside a card's edge.
fn inner_frame(r: Rect, inset: f32, color: Color) {
    let inset = inset.max(3.0);
    let frame = Rect::new(
        r.x + inset,
        r.y + inset,
        r.w - 2.0 * inset,
        r.h - 2.0 * inset,
    );
    rounded_rect_lines(
        frame,
        (corner_radius(r) - inset + 2.0).max(2.0),
        (r.w * 0.01).max(1.0),
        color,
    );
}

fn draw_back(r: Rect, back: &Back) {
    let radius = corner_radius(r);
    rounded_rect(r, radius, back.base);
    inner_frame(r, r.w * 0.06, alpha(back.line, 0.35));
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
