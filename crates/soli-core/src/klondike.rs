use crate::card::Card;
use crate::rng::SplitMix64;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PileId {
    Stock,
    Waste,
    Foundation(usize),
    Tableau(usize),
}

impl PileId {
    pub const FOUNDATIONS: usize = 4;
    pub const TABLEAUS: usize = 7;
}

/// Everything that changes during play. Small enough (52 cards) that undo just
/// keeps a snapshot of it per move.
#[derive(Debug, Clone)]
struct Board {
    stock: Vec<Card>,
    waste: Vec<Card>,
    foundations: [Vec<Card>; PileId::FOUNDATIONS],
    tableau: [Vec<Card>; PileId::TABLEAUS],
    /// How many waste cards from the latest draw are still showing (draw-3 fan).
    waste_visible: usize,
    moves: u32,
}

pub struct Klondike {
    board: Board,
    history: Vec<Board>,
    draw_mode: DrawMode,
    seed: u64,
}

impl Klondike {
    pub fn new(seed: u64, draw_mode: DrawMode) -> Self {
        let mut deck = Card::deck();
        SplitMix64::new(seed).shuffle(&mut deck);

        let mut tableau: [Vec<Card>; PileId::TABLEAUS] = Default::default();
        for (i, pile) in tableau.iter_mut().enumerate() {
            for _ in 0..=i {
                pile.push(deck.pop().expect("deck has enough cards"));
            }
            pile.last_mut().unwrap().face_up = true;
        }

        Klondike {
            board: Board {
                stock: deck,
                waste: Vec::new(),
                foundations: Default::default(),
                tableau,
                waste_visible: 0,
                moves: 0,
            },
            history: Vec::new(),
            draw_mode,
            seed,
        }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn draw_mode(&self) -> DrawMode {
        self.draw_mode
    }

    pub fn moves(&self) -> u32 {
        self.board.moves
    }

    /// Number of waste cards that should be drawn fanned out (draw-3 only).
    pub fn waste_visible(&self) -> usize {
        self.board.waste_visible.min(self.board.waste.len())
    }

    pub fn pile(&self, id: PileId) -> &[Card] {
        let b = &self.board;
        match id {
            PileId::Stock => &b.stock,
            PileId::Waste => &b.waste,
            PileId::Foundation(i) => &b.foundations[i],
            PileId::Tableau(i) => &b.tableau[i],
        }
    }

    fn pile_mut(&mut self, id: PileId) -> &mut Vec<Card> {
        let b = &mut self.board;
        match id {
            PileId::Stock => &mut b.stock,
            PileId::Waste => &mut b.waste,
            PileId::Foundation(i) => &mut b.foundations[i],
            PileId::Tableau(i) => &mut b.tableau[i],
        }
    }

    pub fn is_won(&self) -> bool {
        self.board.foundations.iter().all(|f| f.len() == 13)
    }

    pub fn can_undo(&self) -> bool {
        !self.history.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        match self.history.pop() {
            Some(prev) => {
                self.board = prev;
                true
            }
            None => false,
        }
    }

    /// Click on the stock: deal to the waste, or recycle the waste when the
    /// stock is empty. Returns false if both are empty.
    pub fn draw(&mut self) -> bool {
        if self.board.stock.is_empty() && self.board.waste.is_empty() {
            return false;
        }
        self.history.push(self.board.clone());
        let b = &mut self.board;
        if b.stock.is_empty() {
            while let Some(mut c) = b.waste.pop() {
                c.face_up = false;
                b.stock.push(c);
            }
            b.waste_visible = 0;
        } else {
            let n = self.draw_mode.count().min(b.stock.len());
            for _ in 0..n {
                let mut c = b.stock.pop().unwrap();
                c.face_up = true;
                b.waste.push(c);
            }
            b.waste_visible = n;
        }
        b.moves += 1;
        true
    }

    /// Whether the top `count` cards of `from` can be moved onto `to`.
    pub fn can_move(&self, from: PileId, count: usize, to: PileId) -> bool {
        let src = self.pile(from);
        if from == to || count == 0 || count > src.len() {
            return false;
        }
        let moving = &src[src.len() - count..];
        if !moving.iter().all(|c| c.face_up) {
            return false;
        }
        match from {
            PileId::Stock => return false,
            PileId::Waste | PileId::Foundation(_) if count != 1 => return false,
            _ => {}
        }
        // Face-up runs are always built legally, but checking keeps the engine
        // honest if a state ever gets constructed some other way.
        let is_run = moving
            .windows(2)
            .all(|w| w[0].color() != w[1].color() && w[0].rank == w[1].rank + 1);
        if !is_run {
            return false;
        }

        let first = moving[0];
        let dst = self.pile(to);
        match to {
            PileId::Stock | PileId::Waste => false,
            PileId::Foundation(_) => {
                count == 1
                    && match dst.last() {
                        None => first.rank == 1,
                        Some(top) => top.suit == first.suit && first.rank == top.rank + 1,
                    }
            }
            PileId::Tableau(_) => match dst.last() {
                None => first.rank == 13,
                Some(top) => {
                    top.face_up && top.color() != first.color() && first.rank + 1 == top.rank
                }
            },
        }
    }

    pub fn move_cards(&mut self, from: PileId, count: usize, to: PileId) -> bool {
        if !self.can_move(from, count, to) {
            return false;
        }
        self.history.push(self.board.clone());

        let src = self.pile_mut(from);
        let moving = src.split_off(src.len() - count);
        if let Some(top) = src.last_mut() {
            top.face_up = true;
        }
        self.pile_mut(to).extend(moving);

        if from == PileId::Waste {
            self.board.waste_visible = self.board.waste_visible.saturating_sub(1).max(1);
        }
        self.board.moves += 1;
        true
    }

    /// Best destination for a quick move (double-click): a foundation first,
    /// then a non-empty tableau, then an empty one.
    pub fn auto_target(&self, from: PileId, count: usize) -> Option<PileId> {
        let foundations = (0..PileId::FOUNDATIONS).map(PileId::Foundation);
        let tableaus = (0..PileId::TABLEAUS).map(PileId::Tableau);
        let (filled, empty): (Vec<_>, Vec<_>) =
            tableaus.partition(|&t| !self.pile(t).is_empty());
        foundations
            .chain(filled)
            .chain(empty)
            .find(|&to| self.can_move(from, count, to))
    }

    /// True once every card is face up and the stock and waste are empty, so the
    /// game can be finished mechanically.
    pub fn can_autocomplete(&self) -> bool {
        let b = &self.board;
        !self.is_won()
            && b.stock.is_empty()
            && b.waste.is_empty()
            && b.tableau.iter().flatten().all(|c| c.face_up)
    }

    /// Moves the lowest-ranked card that fits onto a foundation. Returns false
    /// when nothing can move.
    pub fn autocomplete_step(&mut self) -> bool {
        let candidates = std::iter::once(PileId::Waste)
            .chain((0..PileId::TABLEAUS).map(PileId::Tableau))
            .filter_map(|from| self.pile(from).last().map(|c| (c.rank, from)));
        let mut sorted: Vec<_> = candidates.collect();
        sorted.sort_by_key(|&(rank, _)| rank);
        for (_, from) in sorted {
            if let Some(to) = (0..PileId::FOUNDATIONS)
                .map(PileId::Foundation)
                .find(|&to| self.can_move(from, 1, to))
            {
                return self.move_cards(from, 1, to);
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Suit;

    fn up(rank: u8, suit: Suit) -> Card {
        Card { rank, suit, face_up: true }
    }

    fn empty_game() -> Klondike {
        let mut g = Klondike::new(1, DrawMode::One);
        g.board = Board {
            stock: vec![],
            waste: vec![],
            foundations: Default::default(),
            tableau: Default::default(),
            waste_visible: 0,
            moves: 0,
        };
        g
    }

    #[test]
    fn deal_layout() {
        let g = Klondike::new(42, DrawMode::One);
        for i in 0..7 {
            let t = g.pile(PileId::Tableau(i));
            assert_eq!(t.len(), i + 1);
            assert!(t.last().unwrap().face_up);
            assert!(t[..i].iter().all(|c| !c.face_up));
        }
        assert_eq!(g.pile(PileId::Stock).len(), 52 - 28);
        assert!(g.pile(PileId::Waste).is_empty());
    }

    #[test]
    fn same_seed_same_deal() {
        let a = Klondike::new(7, DrawMode::One);
        let b = Klondike::new(7, DrawMode::Three);
        assert_eq!(a.pile(PileId::Stock), b.pile(PileId::Stock));
        assert_ne!(
            Klondike::new(8, DrawMode::One).pile(PileId::Stock),
            a.pile(PileId::Stock)
        );
    }

    #[test]
    fn draw_three_and_recycle() {
        let mut g = Klondike::new(3, DrawMode::Three);
        assert!(g.draw());
        assert_eq!(g.pile(PileId::Waste).len(), 3);
        assert_eq!(g.waste_visible(), 3);
        // 24 cards in stock: 8 draws empties it.
        for _ in 0..7 {
            g.draw();
        }
        assert!(g.pile(PileId::Stock).is_empty());
        let waste_before: Vec<_> = g.pile(PileId::Waste).iter().map(|c| (c.rank, c.suit)).collect();
        assert!(g.draw()); // recycle
        assert!(g.pile(PileId::Waste).is_empty());
        let stock = g.pile(PileId::Stock);
        assert!(stock.iter().all(|c| !c.face_up));
        // Top of the stock is the bottom of the old waste.
        assert_eq!((stock.last().unwrap().rank, stock.last().unwrap().suit), waste_before[0]);
    }

    #[test]
    fn tableau_rules() {
        let mut g = empty_game();
        g.board.tableau[0] = vec![up(8, Suit::Spades)];
        g.board.tableau[1] = vec![up(7, Suit::Hearts)];
        g.board.tableau[2] = vec![up(7, Suit::Clubs)];
        g.board.tableau[3] = vec![up(13, Suit::Diamonds)];
        assert!(g.can_move(PileId::Tableau(1), 1, PileId::Tableau(0)));
        assert!(!g.can_move(PileId::Tableau(2), 1, PileId::Tableau(0)), "same colour");
        assert!(!g.can_move(PileId::Tableau(0), 1, PileId::Tableau(4)), "only kings on empty");
        assert!(g.can_move(PileId::Tableau(3), 1, PileId::Tableau(4)));
    }

    #[test]
    fn move_run_flips_new_top() {
        let mut g = empty_game();
        g.board.tableau[0] = vec![Card::new(2, Suit::Clubs), up(9, Suit::Hearts), up(8, Suit::Clubs)];
        g.board.tableau[1] = vec![up(10, Suit::Spades)];
        assert!(g.move_cards(PileId::Tableau(0), 2, PileId::Tableau(1)));
        assert_eq!(g.pile(PileId::Tableau(1)).len(), 3);
        assert!(g.pile(PileId::Tableau(0))[0].face_up);
        assert!(g.undo());
        assert!(!g.pile(PileId::Tableau(0))[0].face_up);
        assert_eq!(g.pile(PileId::Tableau(1)).len(), 1);
    }

    #[test]
    fn foundation_rules() {
        let mut g = empty_game();
        g.board.tableau[0] = vec![up(2, Suit::Hearts)];
        g.board.tableau[1] = vec![up(1, Suit::Hearts)];
        assert!(!g.can_move(PileId::Tableau(0), 1, PileId::Foundation(0)));
        assert_eq!(g.auto_target(PileId::Tableau(1), 1), Some(PileId::Foundation(0)));
        g.move_cards(PileId::Tableau(1), 1, PileId::Foundation(0));
        assert!(g.can_move(PileId::Tableau(0), 1, PileId::Foundation(0)));
        assert!(!g.can_move(PileId::Tableau(0), 1, PileId::Foundation(1)));
    }

    #[test]
    fn autocomplete_finishes_game() {
        let mut g = empty_game();
        for (i, suit) in Suit::ALL.iter().enumerate() {
            // Each tableau holds one suit, King at the bottom (not a legal
            // tableau run, but fine for exercising autocomplete).
            g.board.tableau[i] = (1..=13).rev().map(|r| up(r, *suit)).collect();
        }
        assert!(g.can_autocomplete());
        while g.autocomplete_step() {}
        assert!(g.is_won());
    }
}
