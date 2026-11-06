use crate::card::Card;
use crate::game::{DrawMode, Game, Hint, History, PileId, Variant, rules};
use crate::rng::SplitMix64;

const FOUNDATIONS: usize = 4;
const TABLEAUS: usize = 7;

/// Everything that changes during play. Small enough (52 cards) that undo just
/// keeps a snapshot of it per move.
#[derive(Debug, Clone)]
struct Board {
    stock: Vec<Card>,
    waste: Vec<Card>,
    foundations: [Vec<Card>; FOUNDATIONS],
    tableau: [Vec<Card>; TABLEAUS],
    /// How many waste cards from the latest draw are still showing (draw-3 fan).
    waste_visible: usize,
    moves: u32,
}

pub struct Klondike {
    board: Board,
    history: History<Board>,
    draw_mode: DrawMode,
    seed: u64,
}

fn foundations() -> impl Iterator<Item = PileId> {
    (0..FOUNDATIONS).map(PileId::Foundation)
}

fn tableaus() -> impl Iterator<Item = PileId> {
    (0..TABLEAUS).map(PileId::Tableau)
}

impl Klondike {
    pub fn new(seed: u64, draw_mode: DrawMode) -> Self {
        let mut deck = Card::deck();
        SplitMix64::new(seed).shuffle(&mut deck);

        let mut tableau: [Vec<Card>; TABLEAUS] = Default::default();
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
            history: History::new(),
            draw_mode,
            seed,
        }
    }

    pub fn draw_mode(&self) -> DrawMode {
        self.draw_mode
    }

    fn pile_mut(&mut self, id: PileId) -> &mut Vec<Card> {
        let b = &mut self.board;
        match id {
            PileId::Stock => &mut b.stock,
            PileId::Waste => &mut b.waste,
            PileId::Foundation(i) => &mut b.foundations[i],
            PileId::Tableau(i) => &mut b.tableau[i],
            PileId::Cell(_) | PileId::Slot(_) => unreachable!("Klondike has no cells or slots"),
        }
    }

    fn apply_move(&mut self, from: PileId, count: usize, to: PileId, automatic: bool) -> bool {
        if !self.can_move(from, count, to) {
            return false;
        }
        self.history.push(&self.board, automatic);

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

    fn foundation_for(&self, from: PileId) -> Option<PileId> {
        foundations().find(|&to| self.can_move(from, 1, to))
    }

    fn fits_a_foundation(&self, card: Card) -> bool {
        self.board
            .foundations
            .iter()
            .any(|f| rules::fits_foundation(card, f))
    }
}

impl Game for Klondike {
    fn variant(&self) -> Variant {
        Variant::Klondike(self.draw_mode)
    }

    fn seed(&self) -> u64 {
        self.seed
    }

    fn moves(&self) -> u32 {
        self.board.moves
    }

    fn piles(&self) -> Vec<PileId> {
        [PileId::Stock, PileId::Waste]
            .into_iter()
            .chain(foundations())
            .chain(tableaus())
            .collect()
    }

    fn pile(&self, id: PileId) -> &[Card] {
        let b = &self.board;
        match id {
            PileId::Stock => &b.stock,
            PileId::Waste => &b.waste,
            PileId::Foundation(i) => &b.foundations[i],
            PileId::Tableau(i) => &b.tableau[i],
            PileId::Cell(_) | PileId::Slot(_) => &[],
        }
    }

    fn waste_fan(&self) -> usize {
        self.board.waste_visible.min(self.board.waste.len())
    }

    fn can_pick_up(&self, from: PileId, count: usize) -> bool {
        let src = self.pile(from);
        if count == 0 || count > src.len() {
            return false;
        }
        match from {
            PileId::Stock | PileId::Cell(_) | PileId::Slot(_) => false,
            PileId::Waste | PileId::Foundation(_) => count == 1,
            // Face-up runs are always built legally, but checking keeps the
            // engine honest if a state ever gets constructed some other way.
            PileId::Tableau(_) => rules::is_alternating_run(&src[src.len() - count..]),
        }
    }

    fn can_move(&self, from: PileId, count: usize, to: PileId) -> bool {
        if from == to || !self.can_pick_up(from, count) {
            return false;
        }
        let src = self.pile(from);
        let first = src[src.len() - count];
        let dst = self.pile(to);
        match to {
            PileId::Foundation(_) => count == 1 && rules::fits_foundation(first, dst),
            PileId::Tableau(_) => match dst.last() {
                None => first.rank == 13,
                Some(&top) => rules::builds_alternating(first, top),
            },
            _ => false,
        }
    }

    fn move_cards(&mut self, from: PileId, count: usize, to: PileId) -> bool {
        self.apply_move(from, count, to, false)
    }

    /// Deal to the waste, or recycle the waste when the stock is empty.
    fn click_stock(&mut self) -> bool {
        if self.board.stock.is_empty() && self.board.waste.is_empty() {
            return false;
        }
        self.history.push(&self.board, false);
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

    fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    fn undo(&mut self) -> bool {
        self.history.undo(&mut self.board)
    }

    /// Every card is face up and the stock and waste are empty.
    fn can_autocomplete(&self) -> bool {
        let b = &self.board;
        !self.is_won()
            && b.stock.is_empty()
            && b.waste.is_empty()
            && b.tableau.iter().flatten().all(|c| c.face_up)
    }

    /// Moves the lowest-ranked card that fits onto a foundation.
    fn autocomplete_step(&mut self) -> bool {
        let mut candidates: Vec<_> = std::iter::once(PileId::Waste)
            .chain(tableaus())
            .filter_map(|from| self.pile(from).last().map(|c| (c.rank, from)))
            .collect();
        candidates.sort_by_key(|&(rank, _)| rank);
        for (_, from) in candidates {
            if let Some(to) = self.foundation_for(from) {
                return self.apply_move(from, 1, to, true);
            }
        }
        false
    }

    fn auto_move_step(&mut self) -> bool {
        for from in std::iter::once(PileId::Waste).chain(tableaus()) {
            let Some(&card) = self.pile(from).last() else {
                continue;
            };
            if !rules::is_safe_for_foundation(card, &self.board.foundations) {
                continue;
            }
            if let Some(to) = self.foundation_for(from) {
                return self.apply_move(from, 1, to, true);
            }
        }
        false
    }

    /// Useful moves, best first, ending with [`Hint::Stock`] while the stock
    /// or waste can still be cycled. Moves that only shuffle cards around, like
    /// a run between columns that reveals nothing, are left out.
    fn hints(&self) -> Vec<Hint> {
        let mut scored: Vec<(u32, Hint)> = Vec::new();
        let mut push =
            |score, from, count, to| scored.push((score, Hint::Move { from, count, to }));

        if !self.board.waste.is_empty() {
            if let Some(to) = self.foundation_for(PileId::Waste) {
                push(100, PileId::Waste, 1, to);
            }
            if let Some(to) = tableaus().find(|&to| self.can_move(PileId::Waste, 1, to)) {
                push(50, PileId::Waste, 1, to);
            }
        }

        for from in tableaus() {
            let pile = self.pile(from);
            let Some(first_up) = pile.iter().position(|c| c.face_up) else {
                continue;
            };
            let reveals = |idx: usize| idx > 0 && !pile[idx - 1].face_up;

            if let Some(to) = self.foundation_for(from) {
                let bonus = if reveals(pile.len() - 1) { 20 } else { 0 };
                push(100 + bonus, from, 1, to);
            }

            for idx in first_up..pile.len() {
                let score = if reveals(idx) {
                    80
                } else if idx == 0 {
                    // Clears the column; only counted onto a real card below.
                    30
                } else if self.fits_a_foundation(pile[idx - 1]) {
                    // Splitting a run frees the card left behind for a foundation.
                    60
                } else {
                    continue;
                };
                let count = pile.len() - idx;
                let target = tableaus().find(|&to| {
                    !(idx == 0 && self.pile(to).is_empty()) && self.can_move(from, count, to)
                });
                if let Some(to) = target {
                    push(score, from, count, to);
                }
            }
        }

        // Stable sort keeps scan order among equally good moves.
        scored.sort_by_key(|&(score, _)| std::cmp::Reverse(score));
        let mut hints: Vec<Hint> = scored.into_iter().map(|(_, h)| h).collect();
        if !self.board.stock.is_empty() || !self.board.waste.is_empty() {
            hints.push(Hint::Stock);
        }
        hints
    }

    fn is_won(&self) -> bool {
        self.board.foundations.iter().all(|f| f.len() == 13)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Suit;

    fn up(rank: u8, suit: Suit) -> Card {
        Card::new(rank, suit).turned_up()
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
        assert!(g.click_stock());
        assert_eq!(g.pile(PileId::Waste).len(), 3);
        assert_eq!(g.waste_fan(), 3);
        // 24 cards in stock: 8 draws empties it.
        for _ in 0..7 {
            g.click_stock();
        }
        assert!(g.pile(PileId::Stock).is_empty());
        let waste_before: Vec<_> = g
            .pile(PileId::Waste)
            .iter()
            .map(|c| (c.rank, c.suit))
            .collect();
        assert!(g.click_stock()); // recycle
        assert!(g.pile(PileId::Waste).is_empty());
        let stock = g.pile(PileId::Stock);
        assert!(stock.iter().all(|c| !c.face_up));
        // Top of the stock is the bottom of the old waste.
        assert_eq!(
            (stock.last().unwrap().rank, stock.last().unwrap().suit),
            waste_before[0]
        );
    }

    #[test]
    fn tableau_rules() {
        let mut g = empty_game();
        g.board.tableau[0] = vec![up(8, Suit::Spades)];
        g.board.tableau[1] = vec![up(7, Suit::Hearts)];
        g.board.tableau[2] = vec![up(7, Suit::Clubs)];
        g.board.tableau[3] = vec![up(13, Suit::Diamonds)];
        assert!(g.can_move(PileId::Tableau(1), 1, PileId::Tableau(0)));
        assert!(
            !g.can_move(PileId::Tableau(2), 1, PileId::Tableau(0)),
            "same colour"
        );
        assert!(
            !g.can_move(PileId::Tableau(0), 1, PileId::Tableau(4)),
            "only kings on empty"
        );
        assert!(g.can_move(PileId::Tableau(3), 1, PileId::Tableau(4)));
    }

    #[test]
    fn move_run_flips_new_top() {
        let mut g = empty_game();
        g.board.tableau[0] = vec![
            Card::new(2, Suit::Clubs),
            up(9, Suit::Hearts),
            up(8, Suit::Clubs),
        ];
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
        assert_eq!(
            g.auto_target(PileId::Tableau(1), 1),
            Some(PileId::Foundation(0))
        );
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

    #[test]
    fn undo_rolls_back_automatic_moves_with_player_move() {
        let mut g = empty_game();
        g.board.tableau[0] = vec![up(1, Suit::Hearts), up(5, Suit::Clubs)];
        g.board.tableau[1] = vec![up(6, Suit::Diamonds)];
        assert!(g.move_cards(PileId::Tableau(0), 1, PileId::Tableau(1)));
        assert!(g.auto_move_step(), "ace is now exposed and always safe");
        assert!(!g.auto_move_step());
        assert!(g.undo());
        assert_eq!(g.pile(PileId::Tableau(0)).len(), 2);
        assert_eq!(g.pile(PileId::Tableau(1)).len(), 1);
        assert!(!g.can_undo());
    }

    #[test]
    fn safe_auto_move_waits_for_opposite_colours() {
        let mut g = empty_game();
        g.board.foundations[0] = (1..=2).map(|r| up(r, Suit::Hearts)).collect();
        g.board.tableau[0] = vec![up(3, Suit::Hearts)];
        // Black twos could still need the red three in the tableau.
        assert!(!g.auto_move_step());
        g.board.foundations[1] = (1..=2).map(|r| up(r, Suit::Clubs)).collect();
        assert!(!g.auto_move_step(), "spades still missing");
        g.board.foundations[2] = (1..=2).map(|r| up(r, Suit::Spades)).collect();
        assert!(g.auto_move_step());
        assert_eq!(g.pile(PileId::Foundation(0)).len(), 3);
    }

    #[test]
    fn hints_prefer_foundation_and_reveals_and_skip_pointless_moves() {
        let mut g = empty_game();
        g.board.tableau[0] = vec![Card::new(4, Suit::Clubs), up(9, Suit::Hearts)];
        g.board.tableau[1] = vec![up(10, Suit::Spades)];
        g.board.tableau[2] = vec![up(10, Suit::Clubs), up(9, Suit::Diamonds)];
        g.board.tableau[3] = vec![up(1, Suit::Spades)];
        let hints = g.hints();
        assert_eq!(
            hints[0],
            Hint::Move {
                from: PileId::Tableau(3),
                count: 1,
                to: PileId::Foundation(0)
            }
        );
        assert!(hints.contains(&Hint::Move {
            from: PileId::Tableau(0),
            count: 1,
            to: PileId::Tableau(1)
        }));
        // Moving the 9 of diamonds off the 10 of clubs reveals nothing.
        assert!(!hints.iter().any(|h| matches!(
            h,
            Hint::Move {
                from: PileId::Tableau(2),
                ..
            }
        )));
        assert!(!hints.contains(&Hint::Stock), "stock and waste are empty");
    }
}
