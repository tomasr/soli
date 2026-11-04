mod layout;
mod render;

use layout::{Layout, STATUS_HEIGHT};
use macroquad::prelude::*;
use soli_core::{DrawMode, Klondike, PileId};
use std::time::{SystemTime, UNIX_EPOCH};

const DOUBLE_CLICK_SECS: f64 = 0.35;
const AUTOPLAY_STEP_SECS: f32 = 0.07;

fn window_conf() -> Conf {
    Conf {
        window_title: "Soli".to_owned(),
        window_width: 1280,
        window_height: 860,
        high_dpi: true,
        sample_count: 4,
        ..Default::default()
    }
}

struct Drag {
    from: PileId,
    count: usize,
    /// Mouse position relative to the top-left of the first dragged card.
    grab: Vec2,
}

struct App {
    game: Klondike,
    drag: Option<Drag>,
    last_press: Option<(PileId, f64)>,
    autoplay: bool,
    autoplay_timer: f32,
    elapsed: f32,
}

impl App {
    fn new(seed: u64, mode: DrawMode) -> Self {
        App {
            game: Klondike::new(seed, mode),
            drag: None,
            last_press: None,
            autoplay: false,
            autoplay_timer: 0.0,
            elapsed: 0.0,
        }
    }

    fn all_piles() -> impl Iterator<Item = PileId> {
        [PileId::Stock, PileId::Waste]
            .into_iter()
            .chain((0..PileId::FOUNDATIONS).map(PileId::Foundation))
            .chain((0..PileId::TABLEAUS).map(PileId::Tableau))
    }

    /// The pile and card index under the point, if any.
    fn hit_test(&self, layout: &Layout, p: Vec2) -> Option<(PileId, usize)> {
        for pile in Self::all_piles() {
            let positions = layout.card_positions(&self.game, pile);
            if pile == PileId::Stock {
                if layout.rect_at(layout.slot(pile)).contains(p) {
                    return Some((pile, 0));
                }
                continue;
            }
            if let Some(i) = positions
                .iter()
                .rposition(|&pos| layout.rect_at(pos).contains(p))
            {
                return Some((pile, i));
            }
        }
        None
    }

    fn handle_press(&mut self, layout: &Layout, mouse: Vec2) {
        let Some((pile, idx)) = self.hit_test(layout, mouse) else {
            return;
        };
        if pile == PileId::Stock {
            self.game.draw();
            return;
        }
        let cards = self.game.pile(pile);
        let len = cards.len();
        if !cards[idx].face_up {
            return;
        }
        // Only the top card can be taken from the waste or a foundation.
        if matches!(pile, PileId::Waste | PileId::Foundation(_)) && idx != len - 1 {
            return;
        }
        let count = len - idx;

        let now = get_time();
        let is_double = matches!(self.last_press, Some((p, t)) if p == pile && now - t < DOUBLE_CLICK_SECS);
        if is_double {
            self.last_press = None;
            if let Some(to) = self.game.auto_target(pile, count) {
                self.game.move_cards(pile, count, to);
            }
            return;
        }
        self.last_press = Some((pile, now));

        let pos = layout.card_positions(&self.game, pile)[idx];
        self.drag = Some(Drag { from: pile, count, grab: mouse - pos });
    }

    /// Legal drop target overlapping the dragged cards the most.
    fn drop_target(&self, layout: &Layout, drag: &Drag, mouse: Vec2) -> Option<PileId> {
        let dragged = layout.rect_at(mouse - drag.grab);
        let targets = (0..PileId::FOUNDATIONS)
            .map(PileId::Foundation)
            .chain((0..PileId::TABLEAUS).map(PileId::Tableau));
        targets
            .filter(|&to| self.game.can_move(drag.from, drag.count, to))
            .filter_map(|to| {
                let top = layout
                    .card_positions(&self.game, to)
                    .last()
                    .copied()
                    .unwrap_or_else(|| layout.slot(to));
                let area = dragged
                    .intersect(layout.rect_at(top))
                    .map(|r| r.w * r.h)?;
                Some((to, area))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(to, _)| to)
    }

    fn handle_release(&mut self, layout: &Layout, mouse: Vec2) {
        if let Some(drag) = self.drag.take()
            && let Some(to) = self.drop_target(layout, &drag, mouse)
        {
            self.game.move_cards(drag.from, drag.count, to);
        }
    }

    fn new_game(&mut self, mode: DrawMode) {
        *self = App::new(random_seed(), mode);
    }

    fn update(&mut self, layout: &Layout) {
        let mouse: Vec2 = mouse_position().into();
        let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);

        if is_key_pressed(KeyCode::N) || is_key_pressed(KeyCode::F2) {
            self.new_game(self.game.draw_mode());
        }
        if is_key_pressed(KeyCode::D) {
            let mode = match self.game.draw_mode() {
                DrawMode::One => DrawMode::Three,
                DrawMode::Three => DrawMode::One,
            };
            self.new_game(mode);
        }
        if is_key_pressed(KeyCode::R) {
            *self = App::new(self.game.seed(), self.game.draw_mode());
        }
        if (ctrl && is_key_pressed(KeyCode::Z)) || is_key_pressed(KeyCode::Backspace) {
            self.drag = None;
            self.autoplay = false;
            self.game.undo();
        }
        if is_key_pressed(KeyCode::A) && self.game.can_autocomplete() {
            self.autoplay = true;
        }

        if self.autoplay {
            self.autoplay_timer += get_frame_time();
            while self.autoplay_timer >= AUTOPLAY_STEP_SECS {
                self.autoplay_timer -= AUTOPLAY_STEP_SECS;
                if !self.game.autocomplete_step() {
                    self.autoplay = false;
                    break;
                }
            }
        } else {
            if is_mouse_button_pressed(MouseButton::Left) {
                self.handle_press(layout, mouse);
            }
            if is_mouse_button_released(MouseButton::Left) {
                self.handle_release(layout, mouse);
            }
        }

        if self.game.moves() > 0 && !self.game.is_won() {
            self.elapsed += get_frame_time();
        }
    }

    fn draw(&self, layout: &Layout) {
        clear_background(render::TABLE);
        let mouse: Vec2 = mouse_position().into();

        let target = self
            .drag
            .as_ref()
            .and_then(|d| self.drop_target(layout, d, mouse));

        for pile in Self::all_piles() {
            let slot = layout.rect_at(layout.slot(pile));
            let label = match pile {
                PileId::Foundation(_) => Some("A"),
                PileId::Tableau(_) => Some("K"),
                PileId::Stock if self.game.pile(PileId::Waste).is_empty() => None,
                PileId::Stock => Some("O"),
                PileId::Waste => None,
            };
            render::draw_slot(slot, label);

            let cards = self.game.pile(pile);
            let positions = layout.card_positions(&self.game, pile);
            let hidden = match &self.drag {
                Some(d) if d.from == pile => d.count,
                _ => 0,
            };
            let visible = cards.len() - hidden;

            if Some(pile) == target {
                let r = positions[..visible]
                    .last()
                    .map(|&p| layout.rect_at(p))
                    .unwrap_or(slot);
                render::draw_outline(r, render::HIGHLIGHT);
            }

            // Stacked piles only need their top two cards drawn.
            let start = match pile {
                PileId::Stock | PileId::Foundation(_) => visible.saturating_sub(2),
                _ => 0,
            };
            for i in start..visible {
                render::draw_card(cards[i], layout.rect_at(positions[i]));
            }
        }

        if let Some(d) = &self.drag {
            let cards = self.game.pile(d.from);
            let origin = mouse - d.grab;
            for (i, &card) in cards[cards.len() - d.count..].iter().enumerate() {
                let pos = origin + vec2(0.0, i as f32 * layout.fan_up);
                render::draw_card(card, layout.rect_at(pos));
            }
        }

        self.draw_status();
        if self.game.is_won() {
            self.draw_win_banner();
        }
    }

    fn draw_status(&self) {
        let (sw, sh) = (screen_width(), screen_height());
        let y = sh - STATUS_HEIGHT;
        draw_rectangle(0.0, y, sw, STATUS_HEIGHT, Color::new(0.0, 0.0, 0.0, 0.35));
        let secs = self.elapsed as u32;
        let mode = match self.game.draw_mode() {
            DrawMode::One => "Draw 1",
            DrawMode::Three => "Draw 3",
        };
        let left = format!(
            "{mode}  Moves {}  Time {:02}:{:02}  Deal #{}",
            self.game.moves(),
            secs / 60,
            secs % 60,
            self.game.seed() % 1_000_000
        );
        draw_text(&left, 12.0, y + 20.0, 16.0, WHITE);

        let right = if self.game.can_autocomplete() && !self.autoplay {
            "A: auto-finish".to_owned()
        } else {
            "N new  D draw 1/3  R restart  Ctrl+Z undo".to_owned()
        };
        let m = measure_text(&right, None, 16, 1.0);
        draw_text(&right, sw - m.width - 12.0, y + 20.0, 16.0, WHITE);
    }

    fn draw_win_banner(&self) {
        let (sw, sh) = (screen_width(), screen_height());
        let (w, h) = (sw.min(520.0), 130.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        render::rounded_rect(r, 14.0, Color::new(0.0, 0.0, 0.0, 0.75));
        let title = "You won!";
        let m = measure_text(title, None, 48, 1.0);
        draw_text(title, r.x + (w - m.width) / 2.0, r.y + 55.0, 48.0, WHITE);
        let sub = format!("{} moves. Press N for a new game.", self.game.moves());
        let m = measure_text(&sub, None, 24, 1.0);
        draw_text(&sub, r.x + (w - m.width) / 2.0, r.y + 100.0, 24.0, WHITE);
    }
}

fn random_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    nanos ^ (nanos >> 29)
}

#[macroquad::main(window_conf)]
async fn main() {
    // `--screenshot <file>` renders one frame of a fixed deal and exits; handy
    // for checking rendering changes without clicking around.
    let args: Vec<String> = std::env::args().collect();
    let screenshot = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1).cloned());

    let mut app = match &screenshot {
        Some(_) => {
            let mut app = App::new(12345, DrawMode::Three);
            for _ in 0..3 {
                app.game.draw();
            }
            app
        }
        None => App::new(random_seed(), DrawMode::One),
    };

    let mut frames = 0;
    loop {
        let layout = Layout::compute(screen_width(), screen_height());
        if screenshot.is_none() {
            app.update(&layout);
        }
        app.draw(&layout);

        if let Some(path) = &screenshot {
            frames += 1;
            if frames == 3 {
                get_screen_data().export_png(path);
                break;
            }
        }
        next_frame().await;
    }
}
