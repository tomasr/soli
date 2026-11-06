//! Spider: two decks, ten columns. Build down regardless of suit, but only a
//! same-suit run moves as a unit; a full King-to-Ace run of one suit is
//! removed to a foundation automatically.

use crate::card::{Card, Suit};
use crate::game::{Game, Hint, History, PileId, SpiderSuits, Variant};
use crate::rng::SplitMix64;

const FOUNDATIONS: usize = 8;
const TABLEAUS: usize = 10;

#[derive(Debug, Clone)]
struct Board {
    stock: Vec<Card>,
    foundations: [Vec<Card>; FOUNDATIONS],
    tableau: [Vec<Card>; TABLEAUS],
    moves: u32,
}

pub struct Spider {
    board: Board,
    history: History<Board>,
    suits: SpiderSuits,
    seed: u64,
}

fn tableaus() -> impl Iterator<Item = PileId> {
    (0..TABLEAUS).map(PileId::Tableau)
}

/// 104 cards: eight suit-sets of 13, drawn from as many suits as the
/// difficulty allows.
fn spider_deck(suits: SpiderSuits) -> Vec<Card> {
    let used: &[Suit] = match suits {
        SpiderSuits::One => &[Suit::Spades],
        SpiderSuits::Two => &[Suit::Spades, Suit::Hearts],
        SpiderSuits::Four => &Suit::ALL,
    };
    let sets_per_suit = 8 / used.len();
    let mut deck = Vec::with_capacity(104);
    for &suit in used {
        for copy in 0..sets_per_suit {
            for rank in 1..=13 {
                deck.push(Card {
                    copy: copy as u8,
                    ..Card::new(rank, suit)
                });
            }
        }
    }
    deck
}

/// The top `count` cards form a face-up, same-suit, descending run.
fn is_suited_run(cards: &[Card]) -> bool {
    cards.iter().all(|c| c.face_up)
        && cards
            .windows(2)
            .all(|w| w[0].suit == w[1].suit && w[0].rank == w[1].rank + 1)
}

impl Spider {
    pub fn new(seed: u64, suits: SpiderSuits) -> Self {
        let mut deck = spider_deck(suits);
        SplitMix64::new(seed).shuffle(&mut deck);

        let mut tableau: [Vec<Card>; TABLEAUS] = Default::default();
        for (i, pile) in tableau.iter_mut().enumerate() {
            let n = if i < 4 { 6 } else { 5 };
            for _ in 0..n {
                pile.push(deck.pop().expect("deck has enough cards"));
            }
            pile.last_mut().unwrap().face_up = true;
        }

        Spider {
            board: Board {
                stock: deck,
                foundations: Default::default(),
                tableau,
                moves: 0,
            },
            history: History::new(),
            suits,
            seed,
        }
    }

    fn column_mut(&mut self, id: PileId) -> &mut Vec<Card> {
        match id {
            PileId::Tableau(i) => &mut self.board.tableau[i],
            _ => unreachable!("only tableau piles change in Spider moves"),
        }
    }

    /// Removes a completed King-to-Ace run from the end of a column, if there
    /// is one, and turns up the card it uncovers.
    fn collect_completed(&mut self, col: usize) {
        let pile = &self.board.tableau[col];
        if pile.len() < 13 {
            return;
        }
        let tail = &pile[pile.len() - 13..];
        if tail[0].rank != 13 || !is_suited_run(tail) {
            return;
        }
        let pile = &mut self.board.tableau[col];
        let run = pile.split_off(pile.len() - 13);
        if let Some(top) = pile.last_mut() {
            top.face_up = true;
        }
        if let Some(slot) = self.board.foundations.iter_mut().find(|f| f.is_empty()) {
            // Stored King first, so the King is the card shown on top.
            slot.extend(run.into_iter().rev());
        }
    }

    /// Length of the same-suit run at the end of a column.
    fn suited_run_len(pile: &[Card]) -> usize {
        (1..=pile.len())
            .take_while(|&n| is_suited_run(&pile[pile.len() - n..]))
            .last()
            .unwrap_or(0)
    }
}

impl Game for Spider {
    fn variant(&self) -> Variant {
        Variant::Spider(self.suits)
    }

    fn seed(&self) -> u64 {
        self.seed
    }

    fn moves(&self) -> u32 {
        self.board.moves
    }

    fn piles(&self) -> Vec<PileId> {
        std::iter::once(PileId::Stock)
            .chain((0..FOUNDATIONS).map(PileId::Foundation))
            .chain(tableaus())
            .collect()
    }

    fn pile(&self, id: PileId) -> &[Card] {
        match id {
            PileId::Stock => &self.board.stock,
            PileId::Foundation(i) => &self.board.foundations[i],
            PileId::Tableau(i) => &self.board.tableau[i],
            PileId::Waste | PileId::Cell(_) | PileId::Slot(_) => &[],
        }
    }

    fn can_pick_up(&self, from: PileId, count: usize) -> bool {
        let PileId::Tableau(_) = from else {
            return false;
        };
        let src = self.pile(from);
        count > 0 && count <= src.len() && is_suited_run(&src[src.len() - count..])
    }

    fn can_move(&self, from: PileId, count: usize, to: PileId) -> bool {
        if from == to || !self.can_pick_up(from, count) {
            return false;
        }
        let PileId::Tableau(_) = to else {
            return false;
        };
        let src = self.pile(from);
        let first = src[src.len() - count];
        match self.pile(to).last() {
            None => true,
            Some(top) => top.face_up && top.rank == first.rank + 1,
        }
    }

    fn move_cards(&mut self, from: PileId, count: usize, to: PileId) -> bool {
        if !self.can_move(from, count, to) {
            return false;
        }
        self.history.push(&self.board, false);
        let src = self.column_mut(from);
        let moving = src.split_off(src.len() - count);
        if let Some(top) = src.last_mut() {
            top.face_up = true;
        }
        self.column_mut(to).extend(moving);
        if let PileId::Tableau(col) = to {
            self.collect_completed(col);
        }
        self.board.moves += 1;
        true
    }

    /// Deals one card face up onto every column.
    fn click_stock(&mut self) -> bool {
        if self.stock_blocked().is_some() || self.board.stock.is_empty() {
            return false;
        }
        self.history.push(&self.board, false);
        for col in 0..TABLEAUS {
            let mut card = self.board.stock.pop().expect("stock deals in tens");
            card.face_up = true;
            self.board.tableau[col].push(card);
        }
        for col in 0..TABLEAUS {
            self.collect_completed(col);
        }
        self.board.moves += 1;
        true
    }

    fn stock_blocked(&self) -> Option<&'static str> {
        let has_empty = self.board.tableau.iter().any(|t| t.is_empty());
        (!self.board.stock.is_empty() && has_empty).then_some("Fill every column before dealing")
    }

    fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    fn undo(&mut self) -> bool {
        self.history.undo(&mut self.board)
    }

    /// Prefers building on the same suit, then any card, then an empty column.
    fn auto_target(&self, from: PileId, count: usize) -> Option<PileId> {
        let src = self.pile(from);
        let first = *src.get(src.len().checked_sub(count)?)?;
        let rank = |to: PileId| match self.pile(to).last() {
            Some(top) if top.suit == first.suit => 0,
            Some(_) => 1,
            None => 2,
        };
        let mut targets: Vec<PileId> = tableaus()
            .filter(|&to| self.can_move(from, count, to))
            .collect();
        targets.sort_by_key(|&to| rank(to));
        targets.first().copied()
    }

    /// Moves that turn up a card or build same-suit runs, best first, then a
    /// deal from the stock.
    fn hints(&self) -> Vec<Hint> {
        let mut scored: Vec<(u32, Hint)> = Vec::new();
        for from in tableaus() {
            let pile = self.pile(from);
            let run = Self::suited_run_len(pile);
            for count in 1..=run {
                let idx = pile.len() - count;
                let first = pile[idx];
                let below = idx.checked_sub(1).map(|i| pile[i]);
                let reveals = below.is_some_and(|c| !c.face_up);
                // Already resting on a card one higher: moving it is only
                // worth it to join a run of its own suit.
                let seated = below.is_some_and(|c| c.face_up && c.rank == first.rank + 1);
                if seated && below.is_some_and(|c| c.suit == first.suit) {
                    continue;
                }

                let mut best: Option<(u32, PileId)> = None;
                for to in tableaus().filter(|&to| self.can_move(from, count, to)) {
                    let score = match self.pile(to).last() {
                        Some(top) if top.suit == first.suit => 70,
                        Some(_) if seated => continue,
                        Some(_) => 40,
                        // An empty column is precious; only spend it to turn
                        // a card up.
                        None if reveals => 25,
                        None => continue,
                    };
                    let score =
                        score + if reveals { 40 } else { 0 } + if idx == 0 { 20 } else { 0 };
                    if best.is_none_or(|(s, _)| score > s) {
                        best = Some((score, to));
                    }
                }
                if let Some((score, to)) = best {
                    scored.push((score, Hint::Move { from, count, to }));
                }
            }
        }
        scored.sort_by_key(|&(score, _)| std::cmp::Reverse(score));
        let mut hints: Vec<Hint> = scored.into_iter().map(|(_, h)| h).collect();
        if !self.board.stock.is_empty() && self.stock_blocked().is_none() {
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

    fn up(rank: u8, suit: Suit) -> Card {
        Card::new(rank, suit).turned_up()
    }

    fn empty_game() -> Spider {
        let mut g = Spider::new(1, SpiderSuits::Four);
        g.board = Board {
            stock: vec![],
            foundations: Default::default(),
            tableau: Default::default(),
            moves: 0,
        };
        g
    }

    #[test]
    fn deal_layout_and_decks() {
        for suits in [SpiderSuits::One, SpiderSuits::Two, SpiderSuits::Four] {
            let g = Spider::new(9, suits);
            let total: usize = tableaus().map(|t| g.pile(t).len()).sum();
            assert_eq!(total, 54);
            assert_eq!(g.pile(PileId::Stock).len(), 50);
            let mut ids: Vec<_> = (0..TABLEAUS)
                .flat_map(|i| g.board.tableau[i].iter())
                .chain(&g.board.stock)
                .map(|c| c.id())
                .collect();
            ids.sort_by_key(|&(r, s, c)| (r, s as u8, c));
            ids.dedup();
            assert_eq!(ids.len(), 104, "every card is distinguishable");
        }
    }

    #[test]
    fn only_suited_runs_move_but_any_suit_builds() {
        let mut g = empty_game();
        g.board.tableau[0] = vec![up(9, Suit::Hearts), up(8, Suit::Spades)];
        g.board.tableau[1] = vec![up(9, Suit::Clubs)];
        g.board.tableau[2] = vec![up(10, Suit::Diamonds)];
        assert!(!g.can_pick_up(PileId::Tableau(0), 2), "mixed suits");
        assert!(
            g.can_move(PileId::Tableau(0), 1, PileId::Tableau(1)),
            "8 on any 9"
        );
        assert!(g.can_move(PileId::Tableau(1), 1, PileId::Tableau(2)));
        assert!(
            g.can_move(PileId::Tableau(1), 1, PileId::Tableau(3)),
            "anything to empty"
        );
    }

    #[test]
    fn completed_run_goes_to_foundation_and_undo_restores_it() {
        let mut g = empty_game();
        g.board.tableau[0] = std::iter::once(Card::new(5, Suit::Clubs))
            .chain((2..=13).rev().map(|r| up(r, Suit::Spades)))
            .collect();
        g.board.tableau[1] = vec![up(1, Suit::Spades)];
        assert!(g.move_cards(PileId::Tableau(1), 1, PileId::Tableau(0)));
        assert_eq!(g.pile(PileId::Foundation(0)).len(), 13);
        assert_eq!(g.pile(PileId::Foundation(0)).last().unwrap().rank, 13);
        assert_eq!(g.pile(PileId::Tableau(0)).len(), 1);
        assert!(
            g.pile(PileId::Tableau(0))[0].face_up,
            "uncovered card turned up"
        );
        assert!(g.undo());
        assert_eq!(g.pile(PileId::Tableau(0)).len(), 13);
        assert!(g.pile(PileId::Foundation(0)).is_empty());
    }

    #[test]
    fn dealing_needs_every_column_filled() {
        let mut g = Spider::new(4, SpiderSuits::One);
        assert!(g.click_stock());
        assert_eq!(g.pile(PileId::Stock).len(), 40);
        assert!(tableaus().all(|t| g.pile(t).last().unwrap().face_up));
        g.board.tableau[3].clear();
        assert!(g.stock_blocked().is_some());
        assert!(!g.click_stock());
    }

    #[test]
    fn hints_prefer_same_suit_and_skip_breaking_runs() {
        let mut g = empty_game();
        g.board.tableau[0] = vec![up(10, Suit::Spades), up(9, Suit::Spades)];
        g.board.tableau[1] = vec![up(10, Suit::Hearts)];
        g.board.tableau[2] = vec![Card::new(3, Suit::Clubs), up(8, Suit::Hearts)];
        g.board.tableau[3] = vec![up(9, Suit::Hearts)];
        let hints = g.hints();
        // 8♥ onto 9♥ turns a card up and builds in suit.
        assert_eq!(
            hints[0],
            Hint::Move {
                from: PileId::Tableau(2),
                count: 1,
                to: PileId::Tableau(3)
            }
        );
        // 9♠ already sits on 10♠; never suggest pulling it off.
        assert!(!hints.contains(&Hint::Move {
            from: PileId::Tableau(0),
            count: 1,
            to: PileId::Tableau(1)
        }));
    }
}
