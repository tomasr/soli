use crate::anim::{Animator, CardKey};
use crate::art::{self, Animal};
use crate::layout::Layout;
use crate::persist::{Settings, Stats};
use crate::render::{self, BACKS, TABLES};
use crate::ui::{self, format_time};
use macroquad::prelude::*;
use soli_core::{Card, Game, Hint, PileId, Variant};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

const DOUBLE_CLICK_SECS: f64 = 0.35;
/// Pace of automatic moves (auto-move and auto-finish).
const AUTO_STEP_SECS: f32 = 0.08;
const HINT_SECS: f64 = 2.5;
/// How long the "can't move this" feedback lingers after the button is let go.
const BLOCKED_FADE_SECS: f64 = 0.35;

/// Feedback for pressing on a run that can't be picked up as a whole: the
/// cards from `idx` up are dimmed, except the top `movable` which glow.
struct Blocked {
    pile: PileId,
    idx: usize,
    movable: usize,
    released: Option<f64>,
}

struct Drag {
    from: PileId,
    count: usize,
    /// Mouse position relative to the top-left of the first dragged card.
    grab: Vec2,
    /// Where the button went down, to tell a click from a drag.
    start: Vec2,
}

/// How far the mouse may move between press and release and still count as
/// a click.
const CLICK_SLOP: f32 = 4.0;

/// Games where cards are paired by clicking one and then the other.
fn pairs_by_click(variant: Variant) -> bool {
    matches!(variant, Variant::Cairn(_))
}

#[derive(PartialEq)]
enum Overlay {
    None,
    Help,
    Stats,
    Games,
}

pub struct App {
    pub game: Box<dyn Game>,
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
    blocked: Option<Blocked>,
    /// Card picked by a click, waiting for its partner (Cairn).
    selected: Option<PileId>,
    toast: Option<(String, f64)>,
    overlay: Overlay,
    /// Variant highlighted in the game chooser.
    menu_focus: Variant,
    /// Mouse position last frame, so hovering only moves the chooser's
    /// highlight when the mouse actually moves.
    last_mouse: Vec2,
    pub settings: Settings,
    stats: Stats,
    /// Settings and stats are written to disk (off for screenshot runs).
    persist: bool,
}

fn key(card: Card) -> CardKey {
    card.id()
}

pub fn random_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    nanos ^ (nanos >> 29)
}

impl App {
    pub fn new(settings: Settings, stats: Stats, seed: u64, persist: bool) -> Self {
        let auto = settings.auto_move;
        let variant = settings.variant();
        App {
            game: variant.new_game(seed),
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
            blocked: None,
            selected: None,
            toast: None,
            overlay: Overlay::None,
            menu_focus: variant,
            last_mouse: Vec2::ZERO,
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
            self.stats.mode_mut(self.game.variant()).record_loss();
            if self.persist {
                self.stats.save();
            }
        }
    }

    fn start_round(&mut self, seed: u64, variant: Variant) {
        self.finish_round();
        let settings = std::mem::take(&mut self.settings);
        let stats = std::mem::take(&mut self.stats);
        let mut next = App::new(settings, stats, seed, self.persist);
        next.game = variant.new_game(seed);
        next.toast = self.toast.take();
        *self = next;
    }

    /// Switches to another game, remembering the choice.
    fn choose_variant(&mut self, variant: Variant) {
        self.overlay = Overlay::None;
        self.settings.set_variant(variant);
        self.save_settings();
        self.start_round(random_seed(), variant);
        self.show_toast(variant.name());
    }

    /// Opens the game chooser, as at startup. The current deal stays behind
    /// it, so closing the chooser just plays that.
    pub fn open_game_chooser(&mut self) {
        self.overlay = Overlay::Games;
        self.menu_focus = self.game.variant();
    }

    /// The variant to offer for a game: the one being played, else the one
    /// played most, else the game's default.
    fn preferred(&self, family: usize) -> Variant {
        let f = &Variant::FAMILIES[family];
        let current = self.game.variant();
        if f.variants.contains(&current) {
            return current;
        }
        f.variants
            .iter()
            .copied()
            .filter(|&v| self.stats.mode(v).played > 0)
            .max_by_key(|&v| self.stats.mode(v).played)
            .unwrap_or(f.default)
    }

    /// Keyboard navigation in the game chooser.
    fn menu_keys(&mut self) {
        let family = self.menu_focus.family();
        let families = Variant::FAMILIES.len();
        let options = Variant::FAMILIES[family].variants;
        let option = options
            .iter()
            .position(|&v| v == self.menu_focus)
            .unwrap_or(0);
        const DIGITS: [KeyCode; 9] = [
            KeyCode::Key1,
            KeyCode::Key2,
            KeyCode::Key3,
            KeyCode::Key4,
            KeyCode::Key5,
            KeyCode::Key6,
            KeyCode::Key7,
            KeyCode::Key8,
            KeyCode::Key9,
        ];
        if let Some(i) = DIGITS[..families].iter().position(|&k| is_key_pressed(k)) {
            self.menu_focus = self.preferred(i);
        }
        if is_key_pressed(KeyCode::Down) {
            self.menu_focus = self.preferred((family + 1) % families);
        }
        if is_key_pressed(KeyCode::Up) {
            self.menu_focus = self.preferred((family + families - 1) % families);
        }
        if is_key_pressed(KeyCode::Right) {
            self.menu_focus = options[(option + 1) % options.len()];
        }
        if is_key_pressed(KeyCode::Left) {
            self.menu_focus = options[(option + options.len() - 1) % options.len()];
        }
    }

    /// Shows an overlay or hint by name ("help", "stats", "games", "hint",
    /// "blocked");
    /// used by screenshot runs.
    pub fn show_overlay(&mut self, name: &str) {
        match name {
            "help" => self.overlay = Overlay::Help,
            "stats" => self.overlay = Overlay::Stats,
            "games" => self.overlay = Overlay::Games,
            "hint" => self.show_hint(),
            "blocked" => {
                // Press on the third card from the top of the first column.
                let pile = PileId::Tableau(0);
                let count = 3.min(self.game.pile(pile).len());
                let idx = self.game.pile(pile).len() - count;
                let movable = (1..count)
                    .rev()
                    .find(|&k| self.game.can_pick_up(pile, k))
                    .unwrap_or(0);
                self.blocked = Some(Blocked {
                    pile,
                    idx,
                    movable,
                    released: None,
                });
            }
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
            || self.blocked.is_some()
            || self
                .toast
                .as_ref()
                .is_some_and(|(_, t)| get_time() - t < 2.0)
            || is_mouse_button_down(MouseButton::Left)
    }

    fn drag_origin(&self, drag: &Drag, mouse: Vec2) -> Vec2 {
        mouse - drag.grab
    }

    /// Where every card is currently drawn: dragged, in flight, or at rest.
    fn drawn_positions(&self, layout: &Layout, now: f64) -> HashMap<CardKey, Vec2> {
        let mouse: Vec2 = mouse_position().into();
        let mut out = HashMap::with_capacity(52);
        for pile in self.game.piles() {
            let cards = self.game.pile(pile);
            let positions = layout.card_positions(self.game.as_ref(), pile);
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
        for pile in self.game.piles() {
            let positions = layout.card_positions(self.game.as_ref(), pile);
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
        self.blocked = None;
        self.selected = None;
        self.hint_cursor = 0;
        self.auto_pending = self.settings.auto_move;
        self.auto_timer = 0.0;
    }

    /// The pile and card index under the point, if any. Piles drawn later
    /// lie on top, so they are tried first.
    fn hit_test(&self, layout: &Layout, p: Vec2) -> Option<(PileId, usize)> {
        for pile in self.game.piles().into_iter().rev() {
            if pile == PileId::Stock {
                // The whole stock acts as one button, including Spider's
                // fanned-out deals.
                let positions = layout.card_positions(self.game.as_ref(), pile);
                let hit = std::iter::once(layout.slot(pile))
                    .chain(positions)
                    .any(|pos| layout.rect_at(pos).contains(p));
                if hit {
                    return Some((pile, 0));
                }
                continue;
            }
            let positions = layout.card_positions(self.game.as_ref(), pile);
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
            // A click on an empty pile (the reserve) can take the selected
            // card.
            if let Some(sel) = self.selected.take() {
                let to = self.game.piles().into_iter().find(|&to| {
                    self.game.pile(to).is_empty()
                        && layout.rect_at(layout.slot(to)).contains(mouse)
                        && self.game.can_move(sel, 1, to)
                });
                if let Some(to) = to {
                    self.animated(layout, |app| app.game.move_cards(sel, 1, to));
                    self.player_acted();
                }
            }
            return;
        };
        if pile == PileId::Stock {
            if let Some(reason) = self.game.stock_blocked() {
                self.show_toast(reason);
            } else if self.animated(layout, |app| app.game.click_stock()) {
                self.player_acted();
            }
            return;
        }
        let cards = self.game.pile(pile);
        let count = cards.len() - idx;
        if !self.game.can_pick_up(pile, count) {
            if pairs_by_click(self.game.variant()) {
                // Covered cards are shaded; anything else gets a reason.
                self.selected = None;
                if let Some(reason) = self.game.blocked_reason(pile) {
                    self.show_toast(reason);
                }
                return;
            }
            if cards[idx].face_up {
                let movable = (1..count)
                    .rev()
                    .find(|&k| self.game.can_pick_up(pile, k))
                    .unwrap_or(0);
                self.blocked = Some(Blocked {
                    pile,
                    idx,
                    movable,
                    released: None,
                });
            }
            return;
        }

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
        self.drag = Some(Drag {
            from: pile,
            count,
            grab: mouse - drawn,
            start: mouse,
        });
    }

    /// A click on a playable card in a pairing game: select it, or pair it
    /// with the card already selected.
    fn click_card(&mut self, layout: &Layout, pile: PileId) {
        match self.selected {
            Some(sel) if sel == pile => self.selected = None,
            Some(sel) if self.game.can_move(sel, 1, pile) => {
                self.animated(layout, |app| app.game.move_cards(sel, 1, pile));
                self.player_acted();
            }
            Some(sel) => {
                if let Some(reason) = self.game.refusal(sel, 1, pile) {
                    self.show_toast(reason);
                }
                self.selected = Some(pile);
            }
            None => self.selected = Some(pile),
        }
    }

    /// Legal drop target overlapping the dragged cards the most.
    fn drop_target(&self, layout: &Layout, drag: &Drag, mouse: Vec2) -> Option<PileId> {
        let dragged = layout.rect_at(self.drag_origin(drag, mouse));
        self.game
            .piles()
            .into_iter()
            .filter(|&to| self.game.can_move(drag.from, drag.count, to))
            .filter_map(|to| {
                // A tableau accepts drops anywhere along the column.
                let slot = layout.rect_at(layout.slot(to));
                let area = match layout.card_positions(self.game.as_ref(), to).last() {
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
        let clicked =
            pairs_by_click(self.game.variant()) && mouse.distance(drag.start) < CLICK_SLOP;
        let target = if clicked {
            None
        } else {
            self.drop_target(layout, drag, mouse)
        };
        let (from, count) = (drag.from, drag.count);
        // Either way the cards glide from the cursor: to the new pile, or back.
        let moved = self.animated(layout, |app| {
            app.drag = None;
            target.is_some_and(|to| app.game.move_cards(from, count, to))
        });
        if moved {
            self.player_acted();
        } else if clicked {
            self.click_card(layout, from);
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
            self.overlay = if self.overlay == Overlay::Help {
                Overlay::None
            } else {
                Overlay::Help
            };
        }
        if is_key_pressed(KeyCode::S) {
            self.overlay = if self.overlay == Overlay::Stats {
                Overlay::None
            } else {
                Overlay::Stats
            };
        }
        if self.overlay == Overlay::Games {
            // N again deals another game of the current kind.
            if is_key_pressed(KeyCode::N) {
                self.choose_variant(self.game.variant());
                return;
            }
            if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
                self.choose_variant(self.menu_focus);
                return;
            }
            self.menu_keys();
        }
        if self.overlay != Overlay::None {
            return;
        }

        if is_key_pressed(KeyCode::N) {
            self.open_game_chooser();
            return;
        }
        if is_key_pressed(KeyCode::F2) {
            self.start_round(random_seed(), self.game.variant());
        }
        if is_key_pressed(KeyCode::R) {
            self.start_round(self.game.seed(), self.game.variant());
        }
        if is_key_pressed(KeyCode::M) {
            self.settings.auto_move = !self.settings.auto_move;
            self.auto_pending = self.settings.auto_move;
            self.save_settings();
            self.show_toast(if self.settings.auto_move {
                "Auto-move on"
            } else {
                "Auto-move off"
            });
        }
        if is_key_pressed(KeyCode::T) {
            self.settings.table = (self.settings.table + 1) % TABLES.len();
            self.save_settings();
            self.show_toast(format!("Table: {}", TABLES[self.settings.table].name));
        }
        if is_key_pressed(KeyCode::B) {
            self.settings.card_back = (self.settings.card_back + 1) % BACKS.len();
            self.save_settings();
            self.show_toast(format!(
                "Card back: {}",
                BACKS[self.settings.card_back].name
            ));
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
                app.blocked = None;
                app.selected = None;
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

        if self.overlay == Overlay::Games {
            let menu = GamesMenu::compute();
            let hovered = menu.chip_at(mouse);
            if mouse != self.last_mouse
                && let Some(v) = hovered
            {
                self.menu_focus = v;
            }
            if is_mouse_button_pressed(MouseButton::Left) {
                if let Some(v) = hovered {
                    self.choose_variant(v);
                } else if let Some(f) = menu.rows.iter().position(|r| r.contains(mouse)) {
                    self.menu_focus = self.preferred(f);
                } else if !menu.panel.contains(mouse) {
                    self.overlay = Overlay::None;
                }
            }
        } else if self.overlay != Overlay::None {
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
            self.stats
                .mode_mut(self.game.variant())
                .record_win(secs, moves);
            if self.persist {
                self.stats.save();
            }
        }

        self.animating = self.anim.update(now);
        if self.hint.is_some_and(|(_, t)| now - t > HINT_SECS) {
            self.hint = None;
        }
        if let Some(b) = &mut self.blocked {
            if !is_mouse_button_down(MouseButton::Left) && b.released.is_none() {
                b.released = Some(now);
            }
            if b.released.is_some_and(|t| now - t > BLOCKED_FADE_SECS) {
                self.blocked = None;
            }
        }
        if self.played && !self.game.is_won() {
            self.elapsed += get_frame_time();
        }
        self.last_mouse = mouse;
    }

    pub fn draw(&self, layout: &Layout) {
        let table = &TABLES[self.settings.table % TABLES.len()];
        let back = &BACKS[self.settings.card_back % BACKS.len()];
        // Only Cairn shades covered cards.
        render::prepare_cards(
            layout.card,
            matches!(self.game.variant(), Variant::Cairn(_)),
        );
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

        for pile in self.game.piles() {
            // A cleared layout slot leaves no outline behind.
            if matches!(pile, PileId::Slot(_)) {
                continue;
            }
            let slot = layout.rect_at(layout.slot(pile));
            let label = slot_label(self.game.variant(), pile);
            render::draw_slot(slot, label);
            let recycles = matches!(
                self.game.variant(),
                Variant::Klondike(_) | Variant::Cairn(_)
            );
            if pile == PileId::Stock
                && recycles
                && !self.game.pile(PileId::Waste).is_empty()
                && self.game.stock_blocked().is_none()
            {
                render::draw_recycle_mark(slot);
            }
        }

        // Layout cards overlap, so a glow drawn under everything would be
        // hidden by neighbours; theirs is drawn just before the card itself.
        let mut slot_glows: Vec<(PileId, Color)> = Vec::new();
        if let Some(sel) = self.selected
            && self.game.can_pick_up(sel, 1)
        {
            if matches!(sel, PileId::Slot(_)) {
                slot_glows.push((sel, render::OCHRE));
            } else {
                render::draw_outline(top_rect(sel), render::OCHRE);
            }
        }

        // Highlights sit between the slots and the cards, so they read as a
        // glow around the cards.
        if let Some(target) = self
            .drag
            .as_ref()
            .and_then(|d| self.drop_target(layout, d, mouse))
        {
            if matches!(target, PileId::Slot(_)) {
                slot_glows.push((target, render::HIGHLIGHT));
            } else {
                render::draw_outline(top_rect(target), render::HIGHLIGHT);
            }
        }
        if let Some((hint, shown)) = self.hint {
            let pulse = 0.65 + 0.35 * ((now - shown) * 6.0).cos() as f32;
            let color = Color {
                a: pulse,
                ..render::HINT
            };
            match hint {
                Hint::Move { from, to, .. } if matches!(from, PileId::Slot(_)) => {
                    slot_glows.push((from, color));
                    if matches!(to, PileId::Slot(_)) {
                        slot_glows.push((to, color));
                    } else {
                        render::draw_outline(top_rect(to), color);
                    }
                }
                Hint::Move { from, count, to } => {
                    let cards = self.game.pile(from);
                    let run = &cards[cards.len() - count..];
                    let first = rect(run[0]);
                    let last = rect(run[run.len() - 1]);
                    let span = Rect::new(first.x, first.y, first.w, last.y + last.h - first.y);
                    render::draw_outline(span, color);
                    render::draw_outline(top_rect(to), color);
                }
                Hint::Stock => render::draw_outline(top_rect(PileId::Stock), color),
            }
        }
        // The part of a blocked run that could move glows like a hint; the
        // rest is dimmed once the cards are drawn.
        let blocked = self.blocked.as_ref().and_then(|b| {
            let cards = self.game.pile(b.pile);
            let fade = b
                .released
                .map_or(1.0, |t| 1.0 - ((now - t) / BLOCKED_FADE_SECS) as f32);
            (b.idx < cards.len() && b.movable < cards.len() - b.idx).then_some((
                b,
                cards,
                fade.max(0.0),
            ))
        });
        if let Some((b, cards, fade)) = blocked
            && b.movable > 0
        {
            let first = rect(cards[cards.len() - b.movable]);
            let last = rect(cards[cards.len() - 1]);
            let span = Rect::new(first.x, first.y, first.w, last.y + last.h - first.y);
            render::draw_outline(
                span,
                Color {
                    a: fade,
                    ..render::HINT
                },
            );
        }

        // Cards in flight are drawn after every pile so they pass over them.
        let mut flying = Vec::new();
        for pile in self.game.piles() {
            let cards = self.game.pile(pile);
            let visible = &cards[..cards.len() - hidden(pile)];
            let (resting, moving): (Vec<_>, Vec<_>) =
                visible.iter().partition(|&&c| !self.anim.is_flying(key(c)));
            flying.extend(moving);
            // Stacked piles only need their top two resting cards drawn.
            let variant = self.game.variant();
            let skip = match pile {
                PileId::Foundation(_) | PileId::Cell(_) => resting.len().saturating_sub(2),
                // Spider's stock is fanned by deal, so every card shows.
                PileId::Stock if matches!(variant, Variant::Klondike(_) | Variant::Cairn(_)) => {
                    resting.len().saturating_sub(2)
                }
                PileId::Waste if matches!(variant, Variant::Cairn(_)) => {
                    resting.len().saturating_sub(2)
                }
                _ => 0,
            };
            for &card in &resting[skip..] {
                if let Some(&(_, color)) = slot_glows.iter().find(|(p, _)| *p == pile) {
                    render::draw_outline(rect(card), color);
                }
                if self.game.is_covered(pile) {
                    render::draw_covered_card(card, rect(card), back);
                } else {
                    render::draw_card(card, rect(card), back);
                }
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
        if let Some((b, cards, fade)) = blocked {
            // One shade over the exposed part of the stuck cards: from the
            // first of them down to where the movable part starts (or the
            // bottom of the last card, if none can move).
            let first = rect(cards[b.idx]);
            let bottom = match b.movable {
                0 => rect(cards[cards.len() - 1]).bottom(),
                n => rect(cards[cards.len() - n]).y,
            };
            let shade = Rect::new(first.x, first.y, first.w, bottom - first.y);
            render::rounded_rect(
                shade,
                first.w * 0.08,
                Color::new(0.0, 0.0, 0.0, 0.45 * fade),
            );
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
            Overlay::Games => self.draw_games(),
        }
    }

    fn draw_status(&self, layout: &Layout) {
        let (sw, sh) = (screen_width(), screen_height());
        let h = layout.status_height;
        let y = sh - h;
        let cy = y + h / 2.0;
        draw_rectangle(0.0, y, sw, h, Color::new(0.0, 0.0, 0.0, 0.28));

        let size = 14.0;
        let baseline = ui::baseline_for(cy, size);
        let pad = 20.0;

        let mut x = pad;
        let name = match self.game.layout_name() {
            Some(layout) => format!("{} · {layout}", self.game.variant().name()),
            None => self.game.variant().name().to_owned(),
        };
        render::text(&name, x, baseline, size, ui::FG);
        x += render::measure(&name, size).width + 28.0;
        let mut fields = vec![
            ("Moves", self.game.moves().to_string()),
            ("Time", format_time(self.elapsed as u32)),
            ("Deal", format!("#{}", self.game.deal_number())),
        ];
        if let Some((pass, passes)) = self.game.stock_pass() {
            fields.push(("Pass", format!("{pass}/{passes}")));
        }
        for (label, value) in fields {
            render::text(label, x, baseline, size, ui::DIM);
            x += render::measure(label, size).width + 6.0;
            render::text(&value, x, baseline, size, ui::FG);
            x += render::measure(&value, size).width + 24.0;
        }

        let hints: &[(&str, &str)] = if self.game.can_autocomplete() && !self.autoplay {
            &[("A", "auto-finish")]
        } else {
            &[
                ("N", "new game"),
                ("H", "hint"),
                ("S", "stats"),
                ("F1", "help"),
            ]
        };
        ui::key_hints(hints, sw - pad - ui::key_hints_width(hints), cy);
    }

    fn draw_win_banner(&self) {
        let r = ui::panel(460.0, 230.0);
        let cx = r.x + r.w / 2.0;
        art::draw_icon(
            Animal::Fox,
            Rect::new(cx - 18.0, r.y + 30.0, 36.0, 36.0),
            1.8,
            ui::ACCENT,
        );
        ui::centered_text("You won", cx, r.y + 116.0, 38.0, ui::FG);

        let stats = self.stats.mode(self.game.variant());
        let secs = self.elapsed as u32;
        let size = 16.0;
        let sub = format!("{} moves in {}", self.game.moves(), format_time(secs));
        let record =
            (stats.best_time_secs == Some(secs) && stats.won > 1).then_some("  ·  New best time");
        let sub_w = render::measure(&sub, size).width
            + record.map_or(0.0, |t| render::measure(t, size).width);
        let mut x = cx - sub_w / 2.0;
        let baseline = r.y + 152.0;
        render::text(&sub, x, baseline, size, ui::DIM);
        if let Some(t) = record {
            x += render::measure(&sub, size).width;
            render::text(t, x, baseline, size, ui::ACCENT);
        }

        let hints = [("N", "new game"), ("S", "stats")];
        let w = ui::key_hints_width(&hints);
        ui::key_hints(&hints, cx - w / 2.0, r.y + 192.0);
    }

    fn draw_stats(&self) {
        let r = ui::panel(760.0, 160.0 + Variant::ALL.len() as f32 * 34.0);
        let cx = r.x + r.w / 2.0;
        ui::centered_text("Statistics", cx, r.y + 44.0, 26.0, ui::FG);

        let size = 15.0;
        type Column = (
            &'static str,
            fn(&crate::persist::ModeStats) -> Option<String>,
        );
        let columns: [Column; 6] = [
            ("Played", |s| Some(s.played.to_string())),
            ("Won", |s| Some(s.won.to_string())),
            ("Win %", |s| {
                s.win_rate().map(|r| format!("{:.0}%", r * 100.0))
            }),
            ("Streak", |s| {
                Some(format!("{} / {}", s.current_streak, s.best_streak))
            }),
            ("Best time", |s| s.best_time_secs.map(format_time)),
            ("Fewest", |s| s.fewest_moves.map(|m| m.to_string())),
        ];
        let first_col = r.x + 200.0;
        let col_w = (r.x + r.w - 32.0 - first_col) / columns.len() as f32;
        let col_right = |i: usize| first_col + (i + 1) as f32 * col_w;

        let mut y = r.y + 92.0;
        for (i, (label, _)) in columns.iter().enumerate() {
            ui::right_text(label, col_right(i), y, size, ui::DIM);
        }
        y += 36.0;

        let current = self.game.variant();
        for variant in Variant::ALL {
            let stats = self.stats.mode(variant);
            let color = if variant == current {
                ui::ACCENT
            } else {
                ui::FG
            };
            render::text(variant.name(), r.x + 32.0, y, size, color);
            for (i, (_, value)) in columns.iter().enumerate() {
                let text = value(&stats).unwrap_or_else(|| "–".to_owned());
                ui::right_text(&text, col_right(i), y, size, color);
            }
            y += 34.0;
        }
        ui::centered_text(
            "Streak is current / best. Abandoned games count as losses.",
            cx,
            r.y + r.h - 20.0,
            13.0,
            ui::DIM,
        );
    }

    fn draw_games(&self) {
        let menu = GamesMenu::compute();
        let r = ui::panel(menu.panel.w, menu.panel.h);
        let left = r.x + 28.0;
        let current = self.game.variant();
        let focus = self.menu_focus;
        render::text("New game", left, r.y + 56.0, 28.0, ui::FG);
        let sub = format!("Press N again for another {}.", current.name());
        render::text(&sub, left, r.y + 84.0, 14.0, ui::DIM);

        for (f, (family, row)) in Variant::FAMILIES.iter().zip(&menu.rows).enumerate() {
            let focused = focus.family() == f;
            if focused {
                render::rounded_rect(*row, 10.0, render::alpha(ui::FG, 0.06));
            }
            let cy = row.center().y;
            let color = if focused { ui::FG } else { ui::DIM };
            ui::keycap(&(f + 1).to_string(), row.x + 12.0, cy, ui::FG);
            art::draw_icon(
                family_animal(f),
                Rect::new(row.x + 50.0, cy - 13.0, 26.0, 26.0),
                1.6,
                color,
            );
            render::text(
                family.name,
                row.x + 92.0,
                ui::baseline_for(cy, 17.0),
                17.0,
                color,
            );
        }

        for &(variant, chip) in &menu.chips {
            let (fill, edge, text) = if variant == focus {
                (render::alpha(ui::ACCENT, 0.18), ui::ACCENT, ui::ACCENT)
            } else {
                (
                    render::alpha(ui::FG, 0.03),
                    render::alpha(ui::FG, 0.16),
                    ui::FG,
                )
            };
            render::rounded_rect(chip, 8.0, fill);
            render::rounded_rect_lines(chip, 8.0, 1.0, edge);
            let baseline = ui::baseline_for(chip.center().y, CHIP_TEXT);
            ui::centered_text(
                variant.option_name(),
                chip.center().x,
                baseline,
                CHIP_TEXT,
                text,
            );
            // A dot marks the game on the table.
            if variant == current {
                draw_circle(chip.x + chip.w - 9.0, chip.y + 9.0, 3.0, ui::ACCENT);
            }
        }

        // What the highlighted choice is, and how it has gone so far.
        let y = menu.detail_y;
        render::text(focus.name(), left, y + 20.0, 18.0, ui::FG);
        render::text(variant_blurb(focus), left, y + 46.0, 14.0, ui::DIM);
        let stats = self.stats.mode(focus);
        let summary = match stats.win_rate() {
            Some(rate) => {
                let mut s = format!("{} played  ·  {:.0}% won", stats.played, rate * 100.0);
                if let Some(t) = stats.best_time_secs {
                    s += &format!("  ·  best {}", format_time(t));
                }
                s
            }
            None => "Not played yet".to_owned(),
        };
        render::text(&summary, left, y + 70.0, 14.0, ui::DIM);

        let hints = [
            ("↑↓", "game"),
            ("‹ ›", "variant"),
            ("Enter", "play"),
            ("N", "same again"),
            ("Esc", "keep deal"),
        ];
        let w = ui::key_hints_width(&hints);
        ui::key_hints(&hints, r.x + (r.w - w) / 2.0, r.y + r.h - 30.0);
    }
}

const CHIP_TEXT: f32 = 15.0;

/// Geometry of the game chooser, shared by drawing and clicking: one row
/// per game, with a chip for each way to play it.
struct GamesMenu {
    panel: Rect,
    rows: Vec<Rect>,
    chips: Vec<(Variant, Rect)>,
    /// Top of the description of the highlighted choice.
    detail_y: f32,
}

impl GamesMenu {
    fn compute() -> Self {
        let row_h = 58.0;
        let n = Variant::FAMILIES.len() as f32;
        let (w, h) = (640.0f32, 106.0 + n * row_h + 150.0);
        let (sw, sh) = (screen_width(), screen_height());
        let (w, h) = (w.min(sw), h.min(sh));
        let panel = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        let mut rows = Vec::new();
        let mut chips = Vec::new();
        for (f, family) in Variant::FAMILIES.iter().enumerate() {
            let y = panel.y + 106.0 + f as f32 * row_h;
            let row = Rect::new(panel.x + 16.0, y, panel.w - 32.0, row_h - 6.0);
            rows.push(row);
            let chip_h = 32.0;
            let mut x = row.x + 220.0;
            for &variant in family.variants {
                let cw = render::measure(variant.option_name(), CHIP_TEXT).width + 32.0;
                let cw = cw.max(84.0);
                chips.push((
                    variant,
                    Rect::new(x, row.center().y - chip_h / 2.0, cw, chip_h),
                ));
                x += cw + 8.0;
            }
        }
        let detail_y = panel.y + 106.0 + n * row_h + 12.0;
        GamesMenu {
            panel,
            rows,
            chips,
            detail_y,
        }
    }

    fn chip_at(&self, p: Vec2) -> Option<Variant> {
        self.chips
            .iter()
            .find(|(_, r)| r.contains(p))
            .map(|&(v, _)| v)
    }
}

fn family_animal(family: usize) -> Animal {
    match Variant::FAMILIES[family].default {
        Variant::Klondike(_) => Animal::Fox,
        Variant::Spider(_) => Animal::Spider,
        Variant::FreeCell => Animal::Swallow,
        Variant::Cairn(_) => Animal::Hare,
    }
}

/// One line on how a variant plays.
fn variant_blurb(variant: Variant) -> &'static str {
    use soli_core::{CairnLevel, DrawMode, SpiderSuits};
    match variant {
        Variant::Klondike(DrawMode::One) => {
            "Turn one card at a time, through the stock as often as you like."
        }
        Variant::Klondike(DrawMode::Three) => "Turn three cards at a time; only the top one plays.",
        Variant::Spider(SpiderSuits::One) => "All spades. The gentlest Spider.",
        Variant::Spider(SpiderSuits::Two) => {
            "Spades and hearts. Runs must keep to one suit to clear."
        }
        Variant::Spider(SpiderSuits::Four) => "Every suit. The hardest Spider.",
        Variant::FreeCell => "Every card face up, four free cells. Almost every deal can be won.",
        Variant::Cairn(CairnLevel::Easy) => {
            "Pair by rank. Three passes through the stock and a reserve."
        }
        Variant::Cairn(CairnLevel::Normal) => {
            "Pair by rank and colour; a card needs an open side. Two passes and a reserve."
        }
        Variant::Cairn(CairnLevel::Hard) => {
            "Pair by rank and colour; a card needs an open side. One pass, no reserve."
        }
    }
}

/// Placeholder letter shown in an empty pile, where the variant has a rule
/// about what may start it.
fn slot_label(variant: Variant, pile: PileId) -> Option<&'static str> {
    match (variant, pile) {
        (Variant::Klondike(_) | Variant::FreeCell, PileId::Foundation(_)) => Some("A"),
        (Variant::Klondike(_), PileId::Tableau(_)) => Some("K"),
        _ => None,
    }
}

fn draw_help() {
    let entries: [(&str, &str); 14] = [
        ("Click stock", "Draw, recycle or deal"),
        ("Drag", "Move a card or run"),
        ("Click, click", "Pair two cards (Cairn)"),
        ("Double-click", "Send card to best spot"),
        ("H", "Hint (repeat for more)"),
        ("Ctrl+Z", "Undo"),
        ("A", "Auto-finish when possible"),
        ("M", "Toggle auto-move to foundations"),
        ("N  /  N N", "Choose game  /  Same game again"),
        ("F2  /  R", "New deal  /  Restart deal"),
        ("T", "Change table colour"),
        ("B", "Change card back"),
        ("S", "Statistics"),
        ("F1  /  Esc", "This help  /  Close"),
    ];
    let row_h = 27.0;
    let r = ui::panel(500.0, 90.0 + entries.len() as f32 * row_h);
    ui::centered_text("Keys", r.x + r.w / 2.0, r.y + 44.0, 26.0, ui::FG);
    let mut y = r.y + 82.0;
    for (k, what) in entries {
        ui::right_text(k, r.x + 170.0, y, 15.0, ui::ACCENT);
        render::text(what, r.x + 190.0, y, 15.0, ui::FG);
        y += row_h;
    }
}
