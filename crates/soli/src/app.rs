use crate::anim::{Animator, CardKey};
use crate::layout::Layout;
use crate::persist::{Settings, Stats};
use crate::render::{self, BACKS, TABLES};
use crate::ui::{self, format_time};
use macroquad::prelude::*;
use soli_core::{Card, DrawMode, Hint, Klondike, PileId};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

const DOUBLE_CLICK_SECS: f64 = 0.35;
/// Pace of automatic moves (auto-move and auto-finish).
const AUTO_STEP_SECS: f32 = 0.08;
const HINT_SECS: f64 = 2.5;

struct Drag {
    from: PileId,
    count: usize,
    /// Mouse position relative to the top-left of the first dragged card.
    grab: Vec2,
}

#[derive(PartialEq)]
enum Overlay {
    None,
    Help,
    Stats,
}

pub struct App {
    pub game: Klondike,
    drag: Option<Drag>,
    anim: Animator,
    animating: bool,
    last_press: Option<(PileId, f64)>,
    /// Auto-finish in progress.
    autoplay: bool,
    /// Safe foundation moves may be available after the last player action.
    auto_pending: bool,
    auto_timer: f32,
    elapsed: f32,
    /// The player has acted this round, so abandoning it counts as a loss.
    played: bool,
    /// This round's result is already in the stats.
    recorded: bool,
    hint: Option<(Hint, f64)>,
    hint_cursor: usize,
    toast: Option<(String, f64)>,
    overlay: Overlay,
    pub settings: Settings,
    stats: Stats,
    /// Settings and stats are written to disk (off for screenshot runs).
    persist: bool,
}

fn key(card: Card) -> CardKey {
    (card.rank, card.suit)
}

pub fn random_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    nanos ^ (nanos >> 29)
}

fn all_piles() -> impl Iterator<Item = PileId> {
    [PileId::Stock, PileId::Waste]
        .into_iter()
        .chain((0..PileId::FOUNDATIONS).map(PileId::Foundation))
        .chain((0..PileId::TABLEAUS).map(PileId::Tableau))
}

impl App {
    pub fn new(settings: Settings, stats: Stats, seed: u64, persist: bool) -> Self {
        let mode = settings.draw_mode();
        let auto = settings.auto_move;
        App {
            game: Klondike::new(seed, mode),
            drag: None,
            anim: Animator::default(),
            animating: false,
            last_press: None,
            autoplay: false,
            auto_pending: auto,
            auto_timer: 0.0,
            elapsed: 0.0,
            played: false,
            recorded: false,
            hint: None,
            hint_cursor: 0,
            toast: None,
            overlay: Overlay::None,
            settings,
            stats,
            persist,
        }
    }

    /// Records an abandoned round as a loss. Call before replacing the game or
    /// quitting.
    pub fn finish_round(&mut self) {
        if self.played && !self.recorded {
            self.recorded = true;
            self.stats.mode_mut(self.game.draw_mode()).record_loss();
            if self.persist {
                self.stats.save();
            }
        }
    }

    fn start_round(&mut self, seed: u64, mode: DrawMode) {
        self.finish_round();
        let settings = std::mem::take(&mut self.settings);
        let stats = std::mem::take(&mut self.stats);
        let mut next = App::new(settings, stats, seed, self.persist);
        next.game = Klondike::new(seed, mode);
        next.toast = self.toast.take();
        *self = next;
    }

    /// Shows an overlay or hint by name ("help", "stats", "hint"); used by
    /// screenshot runs.
    pub fn show_overlay(&mut self, name: &str) {
        match name {
            "help" => self.overlay = Overlay::Help,
            "stats" => self.overlay = Overlay::Stats,
            "hint" => self.show_hint(),
            _ => {}
        }
    }

    fn save_settings(&self) {
        if self.persist {
            self.settings.save();
        }
    }

    fn show_toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), get_time()));
    }

    /// True while the screen needs to update at full rate.
    pub fn busy(&self) -> bool {
        self.drag.is_some()
            || self.animating
            || self.autoplay
            || self.auto_pending
            || self.hint.is_some()
            || self.toast.as_ref().is_some_and(|(_, t)| get_time() - t < 2.0)
            || is_mouse_button_down(MouseButton::Left)
    }

    fn drag_origin(&self, drag: &Drag, mouse: Vec2) -> Vec2 {
        mouse - drag.grab
    }

    /// Where every card is currently drawn: dragged, in flight, or at rest.
    fn drawn_positions(&self, layout: &Layout, now: f64) -> HashMap<CardKey, Vec2> {
        let mouse: Vec2 = mouse_position().into();
        let mut out = HashMap::with_capacity(52);
        for pile in all_piles() {
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
        for pile in all_piles() {
            let positions = layout.card_positions(&self.game, pile);
            for (&card, pos) in self.game.pile(pile).iter().zip(positions) {
                out.insert(key(card), pos);
            }
        }
        out
    }

    /// Runs an action and animates every card it moved from where it was drawn.
    fn animated<R>(&mut self, layout: &Layout, action: impl FnOnce(&mut Self) -> R) -> R {
        let now = get_time();
        let before = self.drawn_positions(layout, now);
        let result = action(self);
        let after = self.resting_positions(layout);
        self.anim.start(&before, &after, now);
        self.animating = true;
        result
    }

    /// Bookkeeping after anything the player did that changed the board.
    fn player_acted(&mut self) {
        self.played = true;
        self.hint = None;
        self.hint_cursor = 0;
        self.auto_pending = self.settings.auto_move;
        self.auto_timer = 0.0;
    }

    /// The pile and card index under the point, if any.
    fn hit_test(&self, layout: &Layout, p: Vec2) -> Option<(PileId, usize)> {
        for pile in all_piles() {
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
            if self.animated(layout, |app| app.game.draw()) {
                self.player_acted();
            }
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
        let is_double =
            matches!(self.last_press, Some((p, t)) if p == pile && now - t < DOUBLE_CLICK_SECS);
        if is_double {
            self.last_press = None;
            if let Some(to) = self.game.auto_target(pile, count) {
                self.animated(layout, |app| app.game.move_cards(pile, count, to));
                self.player_acted();
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
        let moved = self.animated(layout, |app| {
            app.drag = None;
            target.is_some_and(|to| app.game.move_cards(from, count, to))
        });
        if moved {
            self.player_acted();
        }
    }

    fn show_hint(&mut self) {
        let hints = self.game.hints();
        if hints.is_empty() {
            self.show_toast("No moves left");
            return;
        }
        let hint = hints[self.hint_cursor % hints.len()];
        self.hint_cursor += 1;
        self.hint = Some((hint, get_time()));
    }

    fn handle_keys(&mut self, layout: &Layout) {
        let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);

        if is_key_pressed(KeyCode::Escape) {
            self.overlay = Overlay::None;
        }
        if is_key_pressed(KeyCode::F1) {
            self.overlay = if self.overlay == Overlay::Help { Overlay::None } else { Overlay::Help };
        }
        if is_key_pressed(KeyCode::S) {
            self.overlay = if self.overlay == Overlay::Stats { Overlay::None } else { Overlay::Stats };
        }
        if self.overlay != Overlay::None {
            return;
        }

        if is_key_pressed(KeyCode::N) || is_key_pressed(KeyCode::F2) {
            self.start_round(random_seed(), self.game.draw_mode());
        }
        if is_key_pressed(KeyCode::R) {
            self.start_round(self.game.seed(), self.game.draw_mode());
        }
        if is_key_pressed(KeyCode::D) {
            self.settings.draw_three = !self.settings.draw_three;
            self.save_settings();
            let mode = self.settings.draw_mode();
            self.start_round(random_seed(), mode);
            self.show_toast(match mode {
                DrawMode::One => "Draw 1",
                DrawMode::Three => "Draw 3",
            });
        }
        if is_key_pressed(KeyCode::M) {
            self.settings.auto_move = !self.settings.auto_move;
            self.auto_pending = self.settings.auto_move;
            self.save_settings();
            self.show_toast(if self.settings.auto_move { "Auto-move on" } else { "Auto-move off" });
        }
        if is_key_pressed(KeyCode::T) {
            self.settings.table = (self.settings.table + 1) % TABLES.len();
            self.save_settings();
            self.show_toast(format!("Table: {}", TABLES[self.settings.table].name));
        }
        if is_key_pressed(KeyCode::B) {
            self.settings.card_back = (self.settings.card_back + 1) % BACKS.len();
            self.save_settings();
            self.show_toast(format!("Card back: {}", BACKS[self.settings.card_back].name));
        }
        if is_key_pressed(KeyCode::H) && !self.game.is_won() {
            self.show_hint();
        }
        if (ctrl && is_key_pressed(KeyCode::Z)) || is_key_pressed(KeyCode::Backspace) {
            self.animated(layout, |app| {
                app.drag = None;
                app.autoplay = false;
                // Don't immediately redo the automatic moves just undone.
                app.auto_pending = false;
                app.hint = None;
                app.game.undo();
            });
        }
        if is_key_pressed(KeyCode::A) && self.game.can_autocomplete() {
            self.autoplay = true;
            self.auto_timer = 0.0;
        }
    }

    pub fn update(&mut self, layout: &Layout) {
        let mouse: Vec2 = mouse_position().into();
        let now = get_time();
        self.handle_keys(layout);

        if self.overlay != Overlay::None {
            if is_mouse_button_pressed(MouseButton::Left) {
                self.overlay = Overlay::None;
            }
        } else if !self.autoplay {
            if is_mouse_button_pressed(MouseButton::Left) {
                self.handle_press(layout, mouse);
            }
            if is_mouse_button_released(MouseButton::Left) {
                self.handle_release(layout, mouse);
            }
        }

        // Automatic moves, one per step so each one is visible. Paused while a
        // drag is in progress so cards never move out from under the cursor.
        let auto_active = self.autoplay || (self.auto_pending && self.settings.auto_move);
        if auto_active && self.drag.is_none() {
            self.auto_timer += get_frame_time();
            while self.auto_timer >= AUTO_STEP_SECS {
                self.auto_timer -= AUTO_STEP_SECS;
                let finishing = self.autoplay;
                let moved = self.animated(layout, |app| {
                    if finishing {
                        app.game.autocomplete_step()
                    } else {
                        app.game.auto_move_step()
                    }
                });
                if !moved {
                    self.autoplay = false;
                    self.auto_pending = false;
                    self.auto_timer = 0.0;
                    break;
                }
            }
        }

        if self.game.is_won() && !self.recorded {
            self.recorded = true;
            let (secs, moves) = (self.elapsed as u32, self.game.moves());
            self.stats.mode_mut(self.game.draw_mode()).record_win(secs, moves);
            if self.persist {
                self.stats.save();
            }
        }

        self.animating = self.anim.update(now);
        if self.hint.is_some_and(|(_, t)| now - t > HINT_SECS) {
            self.hint = None;
        }
        if self.played && !self.game.is_won() {
            self.elapsed += get_frame_time();
        }
    }

    pub fn draw(&self, layout: &Layout) {
        let table = &TABLES[self.settings.table % TABLES.len()];
        let back = &BACKS[self.settings.card_back % BACKS.len()];
        clear_background(table.felt);
        let mouse: Vec2 = mouse_position().into();
        let now = get_time();
        let drawn = self.drawn_positions(layout, now);
        let rect = |card: Card| layout.rect_at(drawn[&key(card)]);
        let hidden = |pile: PileId| match &self.drag {
            Some(d) if d.from == pile => d.count,
            _ => 0,
        };
        // Rect of a pile's top card (ignoring dragged cards), or its empty slot.
        let top_rect = |pile: PileId| {
            let cards = self.game.pile(pile);
            cards[..cards.len() - hidden(pile)]
                .last()
                .map(|&c| rect(c))
                .unwrap_or_else(|| layout.rect_at(layout.slot(pile)))
        };

        for pile in all_piles() {
            let slot = layout.rect_at(layout.slot(pile));
            let label = match pile {
                PileId::Foundation(_) => Some("A"),
                PileId::Tableau(_) => Some("K"),
                PileId::Stock | PileId::Waste => None,
            };
            render::draw_slot(slot, label, table);
            if pile == PileId::Stock && !self.game.pile(PileId::Waste).is_empty() {
                render::draw_recycle_mark(slot, table);
            }
        }

        // Highlights sit between the slots and the cards, so they read as a
        // glow around the cards.
        if let Some(target) = self.drag.as_ref().and_then(|d| self.drop_target(layout, d, mouse)) {
            render::draw_outline(top_rect(target), render::HIGHLIGHT);
        }
        if let Some((hint, shown)) = self.hint {
            let pulse = 0.65 + 0.35 * ((now - shown) * 6.0).cos() as f32;
            let color = Color { a: pulse, ..render::HINT };
            match hint {
                Hint::Move { from, count, to } => {
                    let cards = self.game.pile(from);
                    let run = &cards[cards.len() - count..];
                    let first = rect(run[0]);
                    let last = rect(run[run.len() - 1]);
                    let span = Rect::new(first.x, first.y, first.w, last.y + last.h - first.y);
                    render::draw_outline(span, color);
                    render::draw_outline(top_rect(to), color);
                }
                Hint::Draw => render::draw_outline(top_rect(PileId::Stock), color),
            }
        }

        // Cards in flight are drawn after every pile so they pass over them.
        let mut flying = Vec::new();
        for pile in all_piles() {
            let cards = self.game.pile(pile);
            let visible = &cards[..cards.len() - hidden(pile)];
            let (resting, moving): (Vec<_>, Vec<_>) =
                visible.iter().partition(|&&c| !self.anim.is_flying(key(c)));
            flying.extend(moving);
            // Stacked piles only need their top two resting cards drawn.
            let skip = match pile {
                PileId::Stock | PileId::Foundation(_) => resting.len().saturating_sub(2),
                _ => 0,
            };
            for &card in &resting[skip..] {
                render::draw_card(card, rect(card), back);
            }
        }
        for card in flying {
            render::draw_card(card, rect(card), back);
        }
        if let Some(d) = &self.drag {
            let cards = self.game.pile(d.from);
            for &card in &cards[cards.len() - d.count..] {
                render::draw_card(card, rect(card), back);
            }
        }

        self.draw_status(layout);
        if let Some((text, shown)) = &self.toast {
            ui::toast(text, now - shown, screen_height() - layout.status_height);
        }
        if self.game.is_won() && self.overlay == Overlay::None {
            self.draw_win_banner();
        }
        match self.overlay {
            Overlay::None => {}
            Overlay::Help => draw_help(),
            Overlay::Stats => self.draw_stats(),
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

        let mode = match self.game.draw_mode() {
            DrawMode::One => "Draw 1",
            DrawMode::Three => "Draw 3",
        };
        let left = format!(
            "{mode}     Moves {}     Time {}     Deal #{}",
            self.game.moves(),
            format_time(self.elapsed as u32),
            self.game.seed() % 1_000_000
        );
        render::text(&left, pad, baseline, size, WHITE);

        let right = if self.game.can_autocomplete() && !self.autoplay {
            "A  auto-finish"
        } else {
            "H  hint     S  stats     F1  help"
        };
        ui::right_text(right, sw - pad, baseline, size, WHITE);
    }

    fn draw_win_banner(&self) {
        let r = ui::panel(480.0, 150.0);
        let cx = r.x + r.w / 2.0;
        ui::centered_text("You won!", cx, r.y + 56.0, 40.0, WHITE);
        let stats = self.stats.mode(self.game.draw_mode());
        let secs = self.elapsed as u32;
        let record = if stats.best_time_secs == Some(secs) && stats.won > 1 {
            "  New best time!"
        } else {
            ""
        };
        let sub = format!("{} moves in {}.{record}", self.game.moves(), format_time(secs));
        ui::centered_text(&sub, cx, r.y + 94.0, 18.0, WHITE);
        ui::centered_text("N  new game     S  stats", cx, r.y + 126.0, 15.0, ui::DIM);
    }

    fn draw_stats(&self) {
        let r = ui::panel(460.0, 360.0);
        let cx = r.x + r.w / 2.0;
        ui::centered_text("Statistics", cx, r.y + 44.0, 26.0, WHITE);

        let current = self.game.draw_mode();
        let modes = [DrawMode::One, DrawMode::Three];
        let cols = [r.x + r.w * 0.68, r.x + r.w * 0.9];
        let size = 16.0;
        let mut y = r.y + 86.0;
        for (i, mode) in modes.iter().enumerate() {
            let label = if *mode == DrawMode::One { "Draw 1" } else { "Draw 3" };
            let color = if *mode == current { ui::ACCENT } else { ui::DIM };
            ui::right_text(label, cols[i], y, size, color);
        }
        y += 34.0;

        let all: Vec<_> = modes.iter().map(|&m| self.stats.mode(m)).collect();
        let none = || "–".to_owned();
        type Row = (&'static str, fn(&crate::persist::ModeStats) -> Option<String>);
        let rows: [Row; 7] = [
            ("Played", |s| Some(s.played.to_string())),
            ("Won", |s| Some(s.won.to_string())),
            ("Win rate", |s| s.win_rate().map(|r| format!("{:.0}%", r * 100.0))),
            ("Current streak", |s| Some(s.current_streak.to_string())),
            ("Best streak", |s| Some(s.best_streak.to_string())),
            ("Best time", |s| s.best_time_secs.map(format_time)),
            ("Fewest moves", |s| s.fewest_moves.map(|m| m.to_string())),
        ];
        for (label, value) in rows {
            render::text(label, r.x + 36.0, y, size, WHITE);
            for (i, s) in all.iter().enumerate() {
                ui::right_text(&value(s).unwrap_or_else(none), cols[i], y, size, WHITE);
            }
            y += 30.0;
        }
        ui::centered_text("Abandoned games count as losses", cx, r.y + r.h - 20.0, 13.0, ui::DIM);
    }
}

fn draw_help() {
    let entries: [(&str, &str); 13] = [
        ("Click stock", "Draw / recycle waste"),
        ("Drag", "Move a card or run"),
        ("Double-click", "Send card to best spot"),
        ("H", "Hint (repeat for more)"),
        ("Ctrl+Z", "Undo"),
        ("A", "Auto-finish when possible"),
        ("M", "Toggle auto-move to foundations"),
        ("N  /  R", "New game  /  Restart deal"),
        ("D", "Switch draw 1 / draw 3"),
        ("T", "Change table colour"),
        ("B", "Change card back"),
        ("S", "Statistics"),
        ("F1  /  Esc", "This help  /  Close"),
    ];
    let row_h = 27.0;
    let r = ui::panel(500.0, 90.0 + entries.len() as f32 * row_h);
    ui::centered_text("Keys", r.x + r.w / 2.0, r.y + 44.0, 26.0, WHITE);
    let mut y = r.y + 82.0;
    for (k, what) in entries {
        ui::right_text(k, r.x + 170.0, y, 15.0, ui::ACCENT);
        render::text(what, r.x + 190.0, y, 15.0, WHITE);
        y += row_h;
    }
}
