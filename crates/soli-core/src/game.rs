//! What every solitaire variant has in common, so a front end can drive any of
//! them through one interface.

use crate::cairn::Cairn;
use crate::card::Card;
use crate::freecell::FreeCell;
use crate::klondike::Klondike;
use crate::spider::Spider;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PileId {
    Stock,
    Waste,
    Foundation(usize),
    Tableau(usize),
    /// FreeCell's free cells; Cairn's reserve.
    Cell(usize),
    /// A position in Cairn's layout, holding at most one card.
    Slot(usize),
}

/// Where a layout slot sits, in half-card units (`x` across, `y` down), and
/// how high it is stacked: a slot is covered by any higher slot it overlaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotPos {
    pub x: i8,
    pub y: i8,
    pub z: u8,
}

/// A suggested next step, as returned by [`Game::hints`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    Move {
        from: PileId,
        count: usize,
        to: PileId,
    },
    /// Click the stock (draw, recycle or deal).
    Stock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawMode {
    One,
    Three,
}

impl DrawMode {
    pub fn count(self) -> usize {
        match self {
            DrawMode::One => 1,
            DrawMode::Three => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpiderSuits {
    One,
    Two,
    Four,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CairnLevel {
    Easy,
    Normal,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Klondike(DrawMode),
    Spider(SpiderSuits),
    FreeCell,
    Cairn(CairnLevel),
}

/// A game and the ways it can be played, as offered in the game chooser.
pub struct Family {
    pub name: &'static str,
    pub variants: &'static [Variant],
    /// Offered first when the player hasn't played this game yet.
    pub default: Variant,
}

impl Variant {
    pub const ALL: [Variant; 9] = [
        Variant::Klondike(DrawMode::One),
        Variant::Klondike(DrawMode::Three),
        Variant::Spider(SpiderSuits::One),
        Variant::Spider(SpiderSuits::Two),
        Variant::Spider(SpiderSuits::Four),
        Variant::FreeCell,
        Variant::Cairn(CairnLevel::Easy),
        Variant::Cairn(CairnLevel::Normal),
        Variant::Cairn(CairnLevel::Hard),
    ];

    pub const FAMILIES: [Family; 4] = [
        Family {
            name: "Klondike",
            variants: &[
                Variant::Klondike(DrawMode::One),
                Variant::Klondike(DrawMode::Three),
            ],
            default: Variant::Klondike(DrawMode::One),
        },
        Family {
            name: "Spider",
            variants: &[
                Variant::Spider(SpiderSuits::One),
                Variant::Spider(SpiderSuits::Two),
                Variant::Spider(SpiderSuits::Four),
            ],
            default: Variant::Spider(SpiderSuits::One),
        },
        Family {
            name: "FreeCell",
            variants: &[Variant::FreeCell],
            default: Variant::FreeCell,
        },
        Family {
            name: "Cairn",
            variants: &[
                Variant::Cairn(CairnLevel::Easy),
                Variant::Cairn(CairnLevel::Normal),
                Variant::Cairn(CairnLevel::Hard),
            ],
            default: Variant::Cairn(CairnLevel::Normal),
        },
    ];

    /// Stable identifier, used in settings and statistics files.
    pub fn key(self) -> &'static str {
        match self {
            Variant::Klondike(DrawMode::One) => "klondike-draw1",
            Variant::Klondike(DrawMode::Three) => "klondike-draw3",
            Variant::Spider(SpiderSuits::One) => "spider-1suit",
            Variant::Spider(SpiderSuits::Two) => "spider-2suits",
            Variant::Spider(SpiderSuits::Four) => "spider-4suits",
            Variant::FreeCell => "freecell",
            // Cairn's first version had no levels; its rules are Easy's.
            Variant::Cairn(CairnLevel::Easy) => "cairn",
            Variant::Cairn(CairnLevel::Normal) => "cairn-normal",
            Variant::Cairn(CairnLevel::Hard) => "cairn-hard",
        }
    }

    pub fn from_key(key: &str) -> Option<Variant> {
        Variant::ALL.into_iter().find(|v| v.key() == key)
    }

    /// Index into [`Variant::FAMILIES`].
    pub fn family(self) -> usize {
        Variant::FAMILIES
            .iter()
            .position(|f| f.variants.contains(&self))
            .expect("every variant belongs to a family")
    }

    /// How this variant differs from the others in its family.
    pub fn option_name(self) -> &'static str {
        match self {
            Variant::Klondike(DrawMode::One) => "Draw 1",
            Variant::Klondike(DrawMode::Three) => "Draw 3",
            Variant::Spider(SpiderSuits::One) => "1 suit",
            Variant::Spider(SpiderSuits::Two) => "2 suits",
            Variant::Spider(SpiderSuits::Four) => "4 suits",
            Variant::FreeCell => "Classic",
            Variant::Cairn(CairnLevel::Easy) => "Easy",
            Variant::Cairn(CairnLevel::Normal) => "Normal",
            Variant::Cairn(CairnLevel::Hard) => "Hard",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Variant::Klondike(DrawMode::One) => "Klondike · Draw 1",
            Variant::Klondike(DrawMode::Three) => "Klondike · Draw 3",
            Variant::Spider(SpiderSuits::One) => "Spider · 1 suit",
            Variant::Spider(SpiderSuits::Two) => "Spider · 2 suits",
            Variant::Spider(SpiderSuits::Four) => "Spider · 4 suits",
            Variant::FreeCell => "FreeCell",
            Variant::Cairn(CairnLevel::Easy) => "Cairn · Easy",
            Variant::Cairn(CairnLevel::Normal) => "Cairn · Normal",
            Variant::Cairn(CairnLevel::Hard) => "Cairn · Hard",
        }
    }

    pub fn new_game(self, seed: u64) -> Box<dyn Game> {
        match self {
            Variant::Klondike(mode) => Box::new(Klondike::new(seed, mode)),
            Variant::Spider(suits) => Box::new(Spider::new(seed, suits)),
            Variant::FreeCell => Box::new(FreeCell::new(seed)),
            Variant::Cairn(level) => Box::new(Cairn::new(seed, level)),
        }
    }
}

pub trait Game {
    fn variant(&self) -> Variant;

    /// Seed that recreates this deal with [`Variant::new_game`].
    fn seed(&self) -> u64;

    /// Number shown to the player for this deal.
    fn deal_number(&self) -> u64 {
        self.seed() % 1_000_000
    }

    fn moves(&self) -> u32;

    /// Every pile in the game, in a fixed order.
    fn piles(&self) -> Vec<PileId>;

    fn pile(&self, id: PileId) -> &[Card];

    /// Positions of the layout's [`PileId::Slot`] piles, bottom first. Empty
    /// for games without a layout.
    fn slots(&self) -> &[SlotPos] {
        &[]
    }

    /// Name of this deal's layout, for games that have several.
    fn layout_name(&self) -> Option<&'static str> {
        None
    }

    /// Number of waste cards drawn fanned out (Klondike draw-3).
    fn waste_fan(&self) -> usize {
        0
    }

    /// Whether the player may pick up the top `count` cards of a pile.
    fn can_pick_up(&self, from: PileId, count: usize) -> bool;

    fn can_move(&self, from: PileId, count: usize, to: PileId) -> bool;

    /// A player move. Returns false (and changes nothing) if it is illegal.
    fn move_cards(&mut self, from: PileId, count: usize, to: PileId) -> bool;

    /// A card lies over this pile's top card. Cards held back only by their
    /// neighbours (Cairn's open-side rule) are not covered.
    fn is_covered(&self, _pile: PileId) -> bool {
        false
    }

    /// Why the top card of `pile` can't be played, when it isn't covered
    /// and the reason may not be obvious.
    fn blocked_reason(&self, _pile: PileId) -> Option<&'static str> {
        None
    }

    /// Why moving `count` cards from `from` to `to` is refused, when the
    /// reason may not be obvious to the player.
    fn refusal(&self, _from: PileId, _count: usize, _to: PileId) -> Option<&'static str> {
        None
    }

    /// Clicking the stock. Returns false if nothing happened.
    fn click_stock(&mut self) -> bool {
        false
    }

    /// Why clicking the stock would currently do nothing, if there's a reason
    /// worth telling the player.
    fn stock_blocked(&self) -> Option<&'static str> {
        None
    }

    /// Current pass through the stock and how many are allowed, for games
    /// that limit them.
    fn stock_pass(&self) -> Option<(u32, u32)> {
        None
    }

    fn can_undo(&self) -> bool;

    /// Reverts the last player action along with any automatic moves that
    /// followed it.
    fn undo(&mut self) -> bool;

    /// Best destination for a quick move (double-click).
    fn auto_target(&self, from: PileId, count: usize) -> Option<PileId> {
        default_auto_target(self, from, count)
    }

    /// Makes one safe automatic move to a foundation. Returns false when
    /// there is none.
    fn auto_move_step(&mut self) -> bool {
        false
    }

    /// The rest of the game can be played out mechanically.
    fn can_autocomplete(&self) -> bool {
        false
    }

    fn autocomplete_step(&mut self) -> bool {
        false
    }

    /// Useful moves, best first.
    fn hints(&self) -> Vec<Hint>;

    fn is_won(&self) -> bool;
}

/// Foundations first, then a tableau with cards, then an empty tableau, then a
/// free cell.
fn default_auto_target<G: Game + ?Sized>(game: &G, from: PileId, count: usize) -> Option<PileId> {
    let piles = game.piles();
    let rank = |to: &PileId| match to {
        PileId::Foundation(_) => 0,
        PileId::Tableau(_) if !game.pile(*to).is_empty() => 1,
        PileId::Tableau(_) => 2,
        PileId::Cell(_) => 3,
        _ => 4,
    };
    let mut targets: Vec<PileId> = piles.into_iter().filter(|t| rank(t) < 4).collect();
    targets.sort_by_key(rank);
    targets
        .into_iter()
        .find(|&to| game.can_move(from, count, to))
}

/// Board snapshots for undo.
pub(crate) struct History<B> {
    snapshots: Vec<(B, bool)>,
}

impl<B: Clone> History<B> {
    pub fn new() -> Self {
        History {
            snapshots: Vec::new(),
        }
    }

    /// Saves `board` before a change. `automatic` marks changes made by the
    /// game rather than the player.
    pub fn push(&mut self, board: &B, automatic: bool) {
        self.snapshots.push((board.clone(), automatic));
    }

    pub fn can_undo(&self) -> bool {
        !self.snapshots.is_empty()
    }

    /// Rolls `board` back past any automatic changes and the player change
    /// before them.
    pub fn undo(&mut self, board: &mut B) -> bool {
        let mut undone = false;
        while let Some((snap, automatic)) = self.snapshots.pop() {
            *board = snap;
            undone = true;
            if !automatic {
                break;
            }
        }
        undone
    }
}

/// Rules shared by several variants.
pub(crate) mod rules {
    use crate::card::Card;

    /// `card` can go onto a foundation pile.
    pub fn fits_foundation(card: Card, foundation: &[Card]) -> bool {
        match foundation.last() {
            None => card.rank == 1,
            Some(top) => top.suit == card.suit && card.rank == top.rank + 1,
        }
    }

    /// `card` can be placed on `top` in an alternating-colour tableau.
    pub fn builds_alternating(card: Card, top: Card) -> bool {
        top.face_up && top.color() != card.color() && card.rank + 1 == top.rank
    }

    /// Face up, descending, alternating colours.
    pub fn is_alternating_run(cards: &[Card]) -> bool {
        cards.iter().all(|c| c.face_up) && cards.windows(2).all(|w| builds_alternating(w[1], w[0]))
    }

    /// A card can go to its foundation without ever being needed in the
    /// tableau again: both cards that could be built on it (next rank down,
    /// opposite colour) are already on foundations.
    pub fn is_safe_for_foundation(card: Card, foundations: &[Vec<Card>]) -> bool {
        if card.rank <= 2 {
            return true;
        }
        let opposite: Vec<_> = foundations
            .iter()
            .filter(|f| f.first().is_some_and(|c| c.color() != card.color()))
            .collect();
        opposite.len() == 2 && opposite.iter().all(|f| f.len() as u8 >= card.rank - 1)
    }
}
