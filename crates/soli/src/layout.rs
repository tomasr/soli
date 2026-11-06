use macroquad::prelude::*;
use soli_core::{Game, PileId, Variant};

const STATUS_HEIGHT: f32 = 40.0;

/// Screen geometry, recomputed every frame so the table follows window resizes.
pub struct Layout {
    variant: Variant,
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
    /// Horizontal spacing between Spider's remaining deals in the stock.
    fan_stock: f32,
    /// Top-left of each layout slot (Cairn).
    slots: Vec<Vec2>,
    /// Stock, waste, reserve and matched pile, below a layout (Cairn).
    under_layout: [Vec2; 4],
}

fn columns(variant: Variant) -> usize {
    match variant {
        Variant::Klondike(_) => 7,
        Variant::Spider(_) => 10,
        Variant::FreeCell => 8,
        Variant::Cairn => 0,
    }
}

/// Space between neighbouring layout cards, as a fraction of the card size.
const LAYOUT_GAP_X: f32 = 0.10;
const LAYOUT_GAP_Y: f32 = 0.06;
/// Space between a layout and the row of piles under it, in card heights.
const UNDER_GAP: f32 = 0.35;
const ASPECT: f32 = 1.4;

impl Layout {
    /// `sw`/`sh` are in logical pixels (macroquad's coordinate space with
    /// `high_dpi`), so nothing here needs to know about the display scale.
    pub fn compute(sw: f32, sh: f32, game: &dyn Game) -> Self {
        let variant = game.variant();
        if !game.slots().is_empty() {
            return Self::compute_layout(sw, sh, game);
        }
        let status_height = STATUS_HEIGHT;
        const GAP_RATIO: f32 = 0.18;
        let cols = columns(variant) as f32;
        let margin = (sw.min(sh) * 0.03).max(8.0);
        let by_width = (sw - 2.0 * margin) / (cols + (cols - 1.0) * GAP_RATIO);
        // Leave room for the top row plus a reasonably long tableau column.
        let by_height = (sh - status_height - 2.0 * margin) / (ASPECT * 3.6);
        let w = by_width.min(by_height).max(20.0);
        let h = w * ASPECT;
        let gap = w * GAP_RATIO;
        let total = cols * w + (cols - 1.0) * gap;
        Layout {
            variant,
            card: vec2(w, h),
            status_height,
            gap,
            x0: (sw - total) / 2.0,
            top_y: margin,
            tableau_y: margin + h + gap * 1.5,
            bottom: sh - status_height - margin * 0.5,
            fan_up: h * 0.27,
            fan_down: h * 0.11,
            fan_waste: w * 0.44,
            fan_stock: w * 0.12,
            slots: Vec::new(),
            under_layout: [Vec2::ZERO; 4],
        }
    }

    /// A game laid out in slots (Cairn): the layout centred, with the stock,
    /// waste, reserve and matched pile in a row beneath it.
    fn compute_layout(sw: f32, sh: f32, game: &dyn Game) -> Self {
        let status_height = STATUS_HEIGHT;
        let slots = game.slots();
        // Extent in half-card units; every card is two units each way.
        let units_x = slots.iter().map(|s| s.x).max().unwrap_or(0) as f32;
        let units_y = slots.iter().map(|s| s.y).max().unwrap_or(0) as f32;
        let margin = (sw.min(sh) * 0.03).max(8.0);
        let avail_w = sw - 2.0 * margin;
        let avail_h = sh - status_height - 2.0 * margin;

        // Widths and heights in cards, so the card size falls out of the fit.
        let across = (units_x / 2.0 * (1.0 + LAYOUT_GAP_X) + 1.0).max(5.0);
        let layout_down = units_y / 2.0 * (1.0 + LAYOUT_GAP_Y) + 1.0;
        let down = layout_down + UNDER_GAP + 1.0;
        let w = (avail_w / across).min(avail_h / down / ASPECT).max(20.0);
        let h = w * ASPECT;
        let unit = vec2(
            w * (1.0 + LAYOUT_GAP_X) / 2.0,
            h * (1.0 + LAYOUT_GAP_Y) / 2.0,
        );

        let layout_w = units_x * unit.x + w;
        let row_w = layout_w.max(across * w);
        let x0 = (sw - layout_w) / 2.0;
        let y0 = margin + (avail_h - down * h) / 2.0;
        let slot_pos = slots
            .iter()
            .map(|s| vec2(x0 + s.x as f32 * unit.x, y0 + s.y as f32 * unit.y))
            .collect();

        let gap = w * 0.18;
        let row_y = y0 + layout_down * h + UNDER_GAP * h;
        let row_x = (sw - row_w) / 2.0;
        let under_layout = [
            vec2(row_x, row_y),
            vec2(row_x + w + gap, row_y),
            vec2(sw / 2.0 - w / 2.0, row_y),
            vec2(row_x + row_w - w, row_y),
        ];
        Layout {
            variant: game.variant(),
            card: vec2(w, h),
            status_height,
            gap,
            x0,
            top_y: y0,
            tableau_y: y0,
            bottom: sh - status_height - margin * 0.5,
            fan_up: h * 0.27,
            fan_down: h * 0.11,
            fan_waste: w * 0.44,
            fan_stock: w * 0.12,
            slots: slot_pos,
            under_layout,
        }
    }

    fn column_x(&self, col: usize) -> f32 {
        self.x0 + col as f32 * (self.card.x + self.gap)
    }

    /// Top-left of a pile's base slot.
    pub fn slot(&self, pile: PileId) -> Vec2 {
        let top = |col: usize| vec2(self.column_x(col), self.top_y);
        match (self.variant, pile) {
            (_, PileId::Tableau(i)) => vec2(self.column_x(i), self.tableau_y),
            (Variant::Klondike(_), PileId::Stock) => top(0),
            (Variant::Klondike(_), PileId::Waste) => top(1),
            (Variant::Klondike(_), PileId::Foundation(i)) => top(3 + i),
            (Variant::Spider(_), PileId::Stock) => top(0),
            (Variant::Spider(_), PileId::Foundation(i)) => top(2 + i),
            (Variant::FreeCell, PileId::Foundation(i)) => top(i),
            (Variant::FreeCell, PileId::Cell(i)) => top(4 + i),
            (Variant::Cairn, PileId::Stock) => self.under_layout[0],
            (Variant::Cairn, PileId::Waste) => self.under_layout[1],
            (Variant::Cairn, PileId::Cell(0)) => self.under_layout[2],
            (Variant::Cairn, PileId::Foundation(0)) => self.under_layout[3],
            (_, PileId::Slot(i)) if i < self.slots.len() => self.slots[i],
            // Piles the variant doesn't have are never drawn.
            _ => vec2(-1000.0, -1000.0),
        }
    }

    pub fn rect_at(&self, pos: Vec2) -> Rect {
        Rect::new(pos.x, pos.y, self.card.x, self.card.y)
    }

    /// Top-left position of every card in a pile, bottom card first.
    pub fn card_positions(&self, game: &dyn Game, pile: PileId) -> Vec<Vec2> {
        let cards = game.pile(pile);
        let base = self.slot(pile);
        match pile {
            PileId::Stock if matches!(self.variant, Variant::Spider(_)) => {
                // One small step per remaining deal of ten.
                (0..cards.len())
                    .map(|i| base + vec2((i / 10) as f32 * self.fan_stock, 0.0))
                    .collect()
            }
            PileId::Stock | PileId::Foundation(_) | PileId::Cell(_) | PileId::Slot(_) => {
                vec![base; cards.len()]
            }
            PileId::Waste => {
                let first_fanned = cards.len() - game.waste_fan();
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
