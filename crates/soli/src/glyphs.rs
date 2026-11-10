//! The rank labels in card corners, rasterised into a texture of their own
//! for painting the card atlas.
//!
//! macroquad caches glyphs in a texture per font and replaces that texture,
//! repacked, whenever it runs out of room. Draw calls only run when the frame
//! ends, so glyphs queued before the replacement come out blank or garbled.
//! On screen that lasts a frame; painted into the atlas, it lasts until the
//! atlas is repainted. A texture built here is never touched again.

use macroquad::prelude::*;
use std::sync::OnceLock;

const LABELS: [&str; 13] = [
    "A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K",
];

/// Transparent pixels around each label, so filtering doesn't bleed.
const GAP: usize = 2;

fn font() -> &'static fontdue::Font {
    static FONT: OnceLock<fontdue::Font> = OnceLock::new();
    FONT.get_or_init(|| {
        fontdue::Font::from_bytes(
            include_bytes!("../assets/fonts/InterTight-SemiBold.ttf") as &[u8],
            fontdue::FontSettings::default(),
        )
        .expect("embedded font is valid")
    })
}

/// Where one label sits in the texture, in physical pixels.
struct Label {
    source: Rect,
    /// From the pen position to the left edge of the bitmap.
    left: f32,
    /// From the baseline up to the top of the bitmap.
    top: f32,
    /// How far the pen moves past the label.
    advance: f32,
}

pub struct RankLabels {
    px: u16,
    texture: Texture2D,
    labels: Vec<Label>,
}

impl RankLabels {
    /// Rasterises every rank label at `px` pixels.
    pub fn new(px: u16) -> RankLabels {
        let font = font();
        let size = px as f32;
        struct Glyph {
            x: i32,
            ymin: i32,
            width: usize,
            height: usize,
            coverage: Vec<u8>,
        }
        let shaped: Vec<(Vec<Glyph>, f32)> = LABELS
            .iter()
            .map(|label| {
                let mut pen = 0.0;
                let mut prev = None;
                let mut glyphs = Vec::new();
                for c in label.chars() {
                    if let Some(p) = prev {
                        pen += font.horizontal_kern(p, c, size).unwrap_or(0.0);
                    }
                    let (m, coverage) = font.rasterize(c, size);
                    glyphs.push(Glyph {
                        x: pen.round() as i32 + m.xmin,
                        ymin: m.ymin,
                        width: m.width,
                        height: m.height,
                        coverage,
                    });
                    pen += m.advance_width;
                    prev = Some(c);
                }
                (glyphs, pen)
            })
            .collect();

        // Each label's bounds: left and right from the pen origin, top and
        // bottom from the baseline (up is positive).
        let bounds: Vec<(i32, i32, i32, i32)> = shaped
            .iter()
            .map(|(glyphs, _)| {
                let left = glyphs.iter().map(|g| g.x).min().unwrap_or(0);
                let right = glyphs
                    .iter()
                    .map(|g| g.x + g.width as i32)
                    .max()
                    .unwrap_or(0);
                let top = glyphs
                    .iter()
                    .map(|g| g.ymin + g.height as i32)
                    .max()
                    .unwrap_or(0);
                let bottom = glyphs.iter().map(|g| g.ymin).min().unwrap_or(0);
                (left, right, top, bottom)
            })
            .collect();

        // One row of labels.
        let width: usize = bounds
            .iter()
            .map(|&(l, r, _, _)| (r - l) as usize + GAP)
            .sum::<usize>()
            + GAP;
        let height = bounds
            .iter()
            .map(|&(_, _, t, b)| (t - b) as usize)
            .max()
            .unwrap_or(0)
            + 2 * GAP;
        let mut alpha = vec![0u8; width * height];
        let mut labels = Vec::with_capacity(LABELS.len());
        let mut cursor = GAP;
        for ((glyphs, advance), &(left, right, top, bottom)) in shaped.iter().zip(&bounds) {
            for g in glyphs {
                let x0 = cursor + (g.x - left) as usize;
                let y0 = GAP + (top - (g.ymin + g.height as i32)) as usize;
                for row in 0..g.height {
                    for col in 0..g.width {
                        let a = g.coverage[row * g.width + col];
                        let px = &mut alpha[(y0 + row) * width + x0 + col];
                        *px = (*px).max(a);
                    }
                }
            }
            let (w, h) = ((right - left) as f32, (top - bottom) as f32);
            labels.push(Label {
                source: Rect::new(cursor as f32, GAP as f32, w, h),
                left: left as f32,
                top: top as f32,
                advance: *advance,
            });
            cursor += (right - left) as usize + GAP;
        }

        let image = Image {
            bytes: alpha.iter().flat_map(|&a| [255, 255, 255, a]).collect(),
            width: width as u16,
            height: height as u16,
        };
        let texture = Texture2D::from_image(&image);
        texture.set_filter(FilterMode::Linear);
        RankLabels {
            px,
            texture,
            labels,
        }
    }

    pub fn px(&self) -> u16 {
        self.px
    }

    fn label(&self, rank: u8) -> &Label {
        &self.labels[rank as usize - 1]
    }

    /// The label's advance width and its height above the baseline, in
    /// logical pixels, like macroquad's `measure_text` width and `offset_y`.
    pub fn measure(&self, rank: u8, dpi: f32) -> (f32, f32) {
        let l = self.label(rank);
        (l.advance / dpi, l.top / dpi)
    }

    /// Draws the label with its pen origin at `x` on the `baseline`.
    pub fn draw(&self, rank: u8, x: f32, baseline: f32, dpi: f32, color: Color) {
        let l = self.label(rank);
        draw_texture_ex(
            &self.texture,
            x + l.left / dpi,
            baseline - l.top / dpi,
            color,
            DrawTextureParams {
                dest_size: Some(vec2(l.source.w / dpi, l.source.h / dpi)),
                source: Some(l.source),
                ..Default::default()
            },
        );
    }
}
