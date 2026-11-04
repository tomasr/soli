// Release builds are GUI apps with no console window. Debug builds keep the
// console so panics and debug output stay visible.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod anim;
mod layout;
mod render;

use anim::{Animator, CardKey};
use layout::Layout;
use macroquad::miniquad::conf::Platform;
use macroquad::prelude::*;
use soli_core::{DrawMode, Klondike, PileId};
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DOUBLE_CLICK_SECS: f64 = 0.35;
const AUTOPLAY_STEP_SECS: f32 = 0.07;

/// Frame pacing. VSync is off (DWM composites windowed apps, so there is no
/// tearing) because a vsynced OpenGL swap chain queues frames and makes dragged
/// cards trail the cursor. Instead we cap the frame rate ourselves: high while
/// something moves, lower when the table is static.
const ACTIVE_FRAME: Duration = Duration::from_micros(1_000_000 / 165);
const IDLE_FRAME: Duration = Duration::from_micros(1_000_000 / 60);

/// Initial window size in logical pixels. The window is created in physical
/// pixels before the display scale is known, so main() grows it to match.
const WINDOW_SIZE: (f32, f32) = (1100.0, 760.0);

fn window_conf() -> Conf {
    Conf {
        window_title: "Soli".to_owned(),
        window_width: WINDOW_SIZE.0 as i32,
        window_height: WINDOW_SIZE.1 as i32,
        high_dpi: true,
        sample_count: 4,
        platform: Platform {
            swap_interval: Some(0),
            ..Default::default()
        },
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
    anim: Animator,
    animating: bool,
    last_press: Option<(PileId, f64)>,
    autoplay: bool,
    autoplay_timer: f32,
    elapsed: f32,
}

fn key(card: soli_core::Card) -> CardKey {
    (card.rank, card.suit)
}

impl App {
    fn new(seed: u64, mode: DrawMode) -> Self {
        App {
            game: Klondike::new(seed, mode),
            drag: None,
            anim: Animator::default(),
            animating: false,
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

    /// True while the screen needs to update at full rate.
    fn busy(&self) -> bool {
        self.drag.is_some()
            || self.animating
            || self.autoplay
            || is_mouse_button_down(MouseButton::Left)
    }

    fn drag_origin(&self, drag: &Drag, mouse: Vec2) -> Vec2 {
        mouse - drag.grab
    }

    /// Where every card is currently drawn: dragged, in flight, or at rest.
    fn drawn_positions(&self, layout: &Layout, now: f64) -> HashMap<CardKey, Vec2> {
        let mouse: Vec2 = mouse_position().into();
        let mut out = HashMap::with_capacity(52);
        for pile in Self::all_piles() {
            let cards = self.game.pile(pile);
            let positions = layout.card_positions(&self.game, pile);
            let dragged_from = match &self.drag {
                Some(d) if d.from == pile => cards.len() - d.count,
                _ => cards.len(),
            };
            for (i, (&card, &pos)) in cards.iter().zip(&positions).enumerate() {
                let drawn = match &self.drag {
                    Some(d) if i >= dragged_from => {
                        let step = (i - dragged_from) as f32 * layout.fan_up;
                        self.drag_origin(d, mouse) + vec2(0.0, step)
                    }
                    _ => self.anim.position(key(card), pos, now),
                };
                out.insert(key(card), drawn);
            }
        }
        out
    }

    fn resting_positions(&self, layout: &Layout) -> HashMap<CardKey, Vec2> {
        let mut out = HashMap::with_capacity(52);
        for pile in Self::all_piles() {
            let positions = layout.card_positions(&self.game, pile);
            for (&card, pos) in self.game.pile(pile).iter().zip(positions) {
                out.insert(key(card), pos);
            }
        }
        out
    }

    /// Runs an action and animates every card it moved from where it was drawn.
    fn animated(&mut self, layout: &Layout, action: impl FnOnce(&mut Self)) {
        let now = get_time();
        let before = self.drawn_positions(layout, now);
        action(self);
        let after = self.resting_positions(layout);
        self.anim.start(&before, &after, now);
        self.animating = true;
    }

    /// The pile and card index under the point, if any.
    fn hit_test(&self, layout: &Layout, p: Vec2) -> Option<(PileId, usize)> {
        for pile in Self::all_piles() {
            if pile == PileId::Stock {
                if layout.rect_at(layout.slot(pile)).contains(p) {
                    return Some((pile, 0));
                }
                continue;
            }
            let positions = layout.card_positions(&self.game, pile);
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
            self.animated(layout, |app| {
                app.game.draw();
            });
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
                self.animated(layout, |app| {
                    app.game.move_cards(pile, count, to);
                });
            }
            return;
        }
        self.last_press = Some((pile, now));

        // Grab the card where it is currently drawn, which may be mid-flight.
        let drawn = self.drawn_positions(layout, now)[&key(cards[idx])];
        self.drag = Some(Drag { from: pile, count, grab: mouse - drawn });
    }

    /// Legal drop target overlapping the dragged cards the most.
    fn drop_target(&self, layout: &Layout, drag: &Drag, mouse: Vec2) -> Option<PileId> {
        let dragged = layout.rect_at(self.drag_origin(drag, mouse));
        let targets = (0..PileId::FOUNDATIONS)
            .map(PileId::Foundation)
            .chain((0..PileId::TABLEAUS).map(PileId::Tableau));
        targets
            .filter(|&to| self.game.can_move(drag.from, drag.count, to))
            .filter_map(|to| {
                // A tableau accepts drops anywhere along the column.
                let slot = layout.rect_at(layout.slot(to));
                let area = match layout.card_positions(&self.game, to).last() {
                    Some(&top) if matches!(to, PileId::Tableau(_)) => {
                        let bottom = top.y + layout.card.y;
                        Rect::new(slot.x, slot.y, slot.w, bottom - slot.y)
                    }
                    Some(&top) => layout.rect_at(top),
                    None => slot,
                };
                let overlap = dragged.intersect(area).map(|r| r.w * r.h)?;
                Some((to, overlap))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(to, _)| to)
    }

    fn handle_release(&mut self, layout: &Layout, mouse: Vec2) {
        let Some(drag) = &self.drag else {
            return;
        };
        let target = self.drop_target(layout, drag, mouse);
        let (from, count) = (drag.from, drag.count);
        // Either way the cards glide from the cursor: to the new pile, or back.
        self.animated(layout, |app| {
            app.drag = None;
            if let Some(to) = target {
                app.game.move_cards(from, count, to);
            }
        });
    }

    fn new_game(&mut self, seed: u64, mode: DrawMode) {
        *self = App::new(seed, mode);
    }

    fn update(&mut self, layout: &Layout) {
        let mouse: Vec2 = mouse_position().into();
        let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);

        if is_key_pressed(KeyCode::N) || is_key_pressed(KeyCode::F2) {
            self.new_game(random_seed(), self.game.draw_mode());
        }
        if is_key_pressed(KeyCode::D) {
            let mode = match self.game.draw_mode() {
                DrawMode::One => DrawMode::Three,
                DrawMode::Three => DrawMode::One,
            };
            self.new_game(random_seed(), mode);
        }
        if is_key_pressed(KeyCode::R) {
            self.new_game(self.game.seed(), self.game.draw_mode());
        }
        if (ctrl && is_key_pressed(KeyCode::Z)) || is_key_pressed(KeyCode::Backspace) {
            self.animated(layout, |app| {
                app.drag = None;
                app.autoplay = false;
                app.game.undo();
            });
        }
        if is_key_pressed(KeyCode::A) && self.game.can_autocomplete() {
            self.autoplay = true;
        }

        if self.autoplay {
            self.autoplay_timer += get_frame_time();
            while self.autoplay_timer >= AUTOPLAY_STEP_SECS {
                self.autoplay_timer -= AUTOPLAY_STEP_SECS;
                let mut moved = false;
                self.animated(layout, |app| moved = app.game.autocomplete_step());
                if !moved {
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

        self.animating = self.anim.update(get_time());
        if self.game.moves() > 0 && !self.game.is_won() {
            self.elapsed += get_frame_time();
        }
    }

    fn draw(&self, layout: &Layout) {
        clear_background(render::TABLE);
        let mouse: Vec2 = mouse_position().into();
        let now = get_time();
        let drawn = self.drawn_positions(layout, now);
        let rect = |card| layout.rect_at(drawn[&key(card)]);

        let target = self
            .drag
            .as_ref()
            .and_then(|d| self.drop_target(layout, d, mouse));

        // Cards in flight are drawn after every pile so they pass over them.
        let mut flying = Vec::new();

        for pile in Self::all_piles() {
            let slot = layout.rect_at(layout.slot(pile));
            let label = match pile {
                PileId::Foundation(_) => Some("A"),
                PileId::Tableau(_) => Some("K"),
                PileId::Stock | PileId::Waste => None,
            };
            render::draw_slot(slot, label);
            if pile == PileId::Stock && !self.game.pile(PileId::Waste).is_empty() {
                render::draw_recycle_mark(slot);
            }

            let cards = self.game.pile(pile);
            let hidden = match &self.drag {
                Some(d) if d.from == pile => d.count,
                _ => 0,
            };
            let visible = &cards[..cards.len() - hidden];

            if Some(pile) == target {
                let r = visible.last().map(|&c| rect(c)).unwrap_or(slot);
                render::draw_outline(r, render::HIGHLIGHT);
            }

            let (resting, moving): (Vec<_>, Vec<_>) =
                visible.iter().partition(|&&c| !self.anim.is_flying(key(c)));
            flying.extend(moving);
            // Stacked piles only need their top two resting cards drawn.
            let skip = match pile {
                PileId::Stock | PileId::Foundation(_) => resting.len().saturating_sub(2),
                _ => 0,
            };
            for &card in &resting[skip..] {
                render::draw_card(card, rect(card));
            }
        }

        for card in flying {
            render::draw_card(card, rect(card));
        }
        if let Some(d) = &self.drag {
            let cards = self.game.pile(d.from);
            for &card in &cards[cards.len() - d.count..] {
                render::draw_card(card, rect(card));
            }
        }

        self.draw_status(layout);
        if self.game.is_won() {
            self.draw_win_banner();
        }
    }

    fn draw_status(&self, layout: &Layout) {
        let (sw, sh) = (screen_width(), screen_height());
        let h = layout.status_height;
        let y = sh - h;
        draw_rectangle(0.0, y, sw, h, Color::new(0.0, 0.0, 0.0, 0.35));

        let size = 15.0;
        let baseline = y + (h + render::measure("M", size).offset_y) / 2.0;
        let pad = 12.0;

        let secs = self.elapsed as u32;
        let mode = match self.game.draw_mode() {
            DrawMode::One => "Draw 1",
            DrawMode::Three => "Draw 3",
        };
        let left = format!(
            "{mode}     Moves {}     Time {:02}:{:02}     Deal #{}",
            self.game.moves(),
            secs / 60,
            secs % 60,
            self.game.seed() % 1_000_000
        );
        render::text(&left, pad, baseline, size, WHITE);

        let right = if self.game.can_autocomplete() && !self.autoplay {
            "A  auto-finish"
        } else {
            "N  new     D  draw 1/3     R  restart     Ctrl+Z  undo"
        };
        let m = render::measure(right, size);
        render::text(right, sw - m.width - pad, baseline, size, WHITE);
    }

    fn draw_win_banner(&self) {
        let (sw, sh) = (screen_width(), screen_height());
        let (w, h) = (480.0f32.min(sw), 130.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        render::rounded_rect(r, 14.0, Color::new(0.0, 0.0, 0.0, 0.75));

        let title = "You won!";
        let m = render::measure(title, 40.0);
        render::text(title, r.x + (w - m.width) / 2.0, r.y + 58.0, 40.0, WHITE);
        let secs = self.elapsed as u32;
        let sub = format!(
            "{} moves in {:02}:{:02}. Press N for a new game.",
            self.game.moves(),
            secs / 60,
            secs % 60
        );
        let m = render::measure(&sub, 18.0);
        render::text(&sub, r.x + (w - m.width) / 2.0, r.y + 98.0, 18.0, WHITE);
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
    render::init_font();
    let dpi = screen_dpi_scale();
    if dpi > 1.0 {
        request_new_screen_size(WINDOW_SIZE.0 * dpi, WINDOW_SIZE.1 * dpi);
    }

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
        let frame_start = Instant::now();
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

        // Sleep before presenting, so input is read as late as possible after.
        let budget = if app.busy() { ACTIVE_FRAME } else { IDLE_FRAME };
        if let Some(rest) = budget.checked_sub(frame_start.elapsed()) {
            std::thread::sleep(rest);
        }
        next_frame().await;
    }
}
