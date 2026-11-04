use macroquad::prelude::*;
use soli_core::{Klondike, PileId};

const STATUS_HEIGHT: f32 = 30.0;

/// Screen geometry, recomputed every frame so the table follows window resizes.
pub struct Layout {
    pub card: Vec2,
    pub status_height: f32,
    pub gap: f32,
    x0: f32,
    top_y: f32,
    tableau_y: f32,
    bottom: f32,
    /// Vertical spacing between face-up cards in a tableau (before compression).
    pub fan_up: f32,
    fan_down: f32,
    /// Horizontal spacing of the draw-3 waste fan.
    fan_waste: f32,
}

impl Layout {
    /// `sw`/`sh` are in logical pixels (macroquad's coordinate space with
    /// `high_dpi`), so nothing here needs to know about the display scale.
    pub fn compute(sw: f32, sh: f32) -> Self {
        let status_height = STATUS_HEIGHT;
        const GAP_RATIO: f32 = 0.18;
        const ASPECT: f32 = 1.4;
        let margin = (sw.min(sh) * 0.03).max(8.0);
        let by_width = (sw - 2.0 * margin) / (7.0 + 6.0 * GAP_RATIO);
        // Leave room for the top row plus a reasonably long tableau column.
        let by_height = (sh - status_height - 2.0 * margin) / (ASPECT * 3.6);
        let w = by_width.min(by_height).max(20.0);
        let h = w * ASPECT;
        let gap = w * GAP_RATIO;
        let total = 7.0 * w + 6.0 * gap;
        Layout {
            card: vec2(w, h),
            status_height,
            gap,
            x0: (sw - total) / 2.0,
            top_y: margin,
            tableau_y: margin + h + gap * 1.5,
            bottom: sh - status_height - margin * 0.5,
            fan_up: h * 0.27,
            fan_down: h * 0.11,
            fan_waste: w * 0.24,
        }
    }

    fn column_x(&self, col: usize) -> f32 {
        self.x0 + col as f32 * (self.card.x + self.gap)
    }

    /// Top-left of a pile's base slot.
    pub fn slot(&self, pile: PileId) -> Vec2 {
        match pile {
            PileId::Stock => vec2(self.column_x(0), self.top_y),
            PileId::Waste => vec2(self.column_x(1), self.top_y),
            PileId::Foundation(i) => vec2(self.column_x(3 + i), self.top_y),
            PileId::Tableau(i) => vec2(self.column_x(i), self.tableau_y),
        }
    }

    pub fn rect_at(&self, pos: Vec2) -> Rect {
        Rect::new(pos.x, pos.y, self.card.x, self.card.y)
    }

    /// Top-left position of every card in a pile, bottom card first.
    pub fn card_positions(&self, game: &Klondike, pile: PileId) -> Vec<Vec2> {
        let cards = game.pile(pile);
        let base = self.slot(pile);
        match pile {
            PileId::Stock | PileId::Foundation(_) => vec![base; cards.len()],
            PileId::Waste => {
                let fanned = game.waste_visible();
                let first_fanned = cards.len() - fanned;
                (0..cards.len())
                    .map(|i| {
                        let step = i.saturating_sub(first_fanned) as f32;
                        base + vec2(step * self.fan_waste, 0.0)
                    })
                    .collect()
            }
            PileId::Tableau(_) => {
                let (down, up) = self.tableau_offsets(cards.iter().map(|c| c.face_up));
                let mut y = base.y;
                cards
                    .iter()
                    .map(|c| {
                        let pos = vec2(base.x, y);
                        y += if c.face_up { up } else { down };
                        pos
                    })
                    .collect()
            }
        }
    }

    /// Fan offsets for a tableau column, squeezed so the column stays on screen.
    fn tableau_offsets(&self, face_up: impl Iterator<Item = bool>) -> (f32, f32) {
        let (mut n_down, mut n_up) = (0.0, 0.0);
        let flags: Vec<bool> = face_up.collect();
        // The last card's offset doesn't take up space.
        for &f in flags.iter().take(flags.len().saturating_sub(1)) {
            if f {
                n_up += 1.0;
            } else {
                n_down += 1.0;
            }
        }
        let needed = n_down * self.fan_down + n_up * self.fan_up;
        let available = self.bottom - self.tableau_y - self.card.y;
        let scale = if needed > available && needed > 0.0 {
            (available / needed).max(0.0)
        } else {
            1.0
        };
        (self.fan_down * scale, self.fan_up * scale)
    }
}
