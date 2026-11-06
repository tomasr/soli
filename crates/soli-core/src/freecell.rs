//! FreeCell: one deck dealt face up into eight columns, four free cells that
//! hold one card each, alternating-colour building. Deals use the Microsoft
//! numbering, so "deal #N" matches the classic Windows game.

use crate::card::{Card, Suit};
use crate::game::{Game, Hint, History, PileId, Variant, rules};

const CELLS: usize = 4;
const FOUNDATIONS: usize = 4;
const TABLEAUS: usize = 8;
const DEALS: u64 = 1_000_000;

#[derive(Debug, Clone)]
struct Board {
    cells: [Vec<Card>; CELLS],
    foundations: [Vec<Card>; FOUNDATIONS],
    tableau: [Vec<Card>; TABLEAUS],
    moves: u32,
}

pub struct FreeCell {
    board: Board,
    history: History<Board>,
    deal: u64,
}

fn cells() -> impl Iterator<Item = PileId> {
    (0..CELLS).map(PileId::Cell)
}

fn foundations() -> impl Iterator<Item = PileId> {
    (0..FOUNDATIONS).map(PileId::Foundation)
}

fn tableaus() -> impl Iterator<Item = PileId> {
    (0..TABLEAUS).map(PileId::Tableau)
}

/// Microsoft FreeCell deal `n`: the cards in the order they are dealt, one
/// per column left to right, row by row.
fn ms_deal(n: u64) -> Vec<Card> {
    let mut state = n as u32;
    let mut rand = move || {
        state = state.wrapping_mul(214_013).wrapping_add(2_531_011);
        (state >> 16) & 0x7fff
    };
    // Card numbers count A♣ A♦ A♥ A♠ 2♣ ... K♠; the deck starts reversed.
    const SUITS: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];
    let mut deck: Vec<u32> = (0..52).rev().collect();
    for i in 0..52 {
        let j = 51 - (rand() as usize) % (52 - i);
        deck.swap(i, j);
    }
    deck.into_iter()
        .map(|c| Card::new((c / 4) as u8 + 1, SUITS[(c % 4) as usize]).turned_up())
        .collect()
}

impl FreeCell {
    /// Any seed maps onto deals 1..=1,000,000; a deal number maps to itself.
    pub fn new(seed: u64) -> Self {
        let deal = seed.wrapping_sub(1) % DEALS + 1;
        let mut tableau: [Vec<Card>; TABLEAUS] = Default::default();
        for (i, card) in ms_deal(deal).into_iter().enumerate() {
            tableau[i % TABLEAUS].push(card);
        }
        FreeCell {
            board: Board {
                cells: Default::default(),
                foundations: Default::default(),
                tableau,
                moves: 0,
            },
            history: History::new(),
            deal,
        }
    }

    fn pile_mut(&mut self, id: PileId) -> &mut Vec<Card> {
        let b = &mut self.board;
        match id {
            PileId::Cell(i) => &mut b.cells[i],
            PileId::Foundation(i) => &mut b.foundations[i],
            PileId::Tableau(i) => &mut b.tableau[i],
            _ => unreachable!("FreeCell has no stock or waste"),
        }
    }

    fn free_cells(&self) -> usize {
        self.board.cells.iter().filter(|c| c.is_empty()).count()
    }

    fn empty_columns(&self) -> usize {
        self.board.tableau.iter().filter(|t| t.is_empty()).count()
    }

    /// How many cards can move as a unit, using free cells and empty columns
    /// as temporary space. Moving into an empty column leaves one fewer.
    fn max_run(&self, onto_empty: bool) -> usize {
        let empty = self.empty_columns() - usize::from(onto_empty);
        (self.free_cells() + 1) << empty
    }

    fn apply_move(&mut self, from: PileId, count: usize, to: PileId, automatic: bool) -> bool {
        if !self.can_move(from, count, to) {
            return false;
        }
        self.history.push(&self.board, automatic);
        let src = self.pile_mut(from);
        let moving = src.split_off(src.len() - count);
        self.pile_mut(to).extend(moving);
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

impl Game for FreeCell {
    fn variant(&self) -> Variant {
        Variant::FreeCell
    }

    fn seed(&self) -> u64 {
        self.deal
    }

    fn deal_number(&self) -> u64 {
        self.deal
    }

    fn moves(&self) -> u32 {
        self.board.moves
    }

    fn piles(&self) -> Vec<PileId> {
        cells().chain(foundations()).chain(tableaus()).collect()
    }

    fn pile(&self, id: PileId) -> &[Card] {
        let b = &self.board;
        match id {
            PileId::Cell(i) => &b.cells[i],
            PileId::Foundation(i) => &b.foundations[i],
            PileId::Tableau(i) => &b.tableau[i],
            PileId::Stock | PileId::Waste | PileId::Slot(_) => &[],
        }
    }

    fn can_pick_up(&self, from: PileId, count: usize) -> bool {
        let src = self.pile(from);
        if count == 0 || count > src.len() {
            return false;
        }
        match from {
            PileId::Cell(_) => count == 1,
            PileId::Tableau(_) => {
                count <= self.max_run(false) && rules::is_alternating_run(&src[src.len() - count..])
            }
            _ => false,
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
            PileId::Cell(_) => count == 1 && dst.is_empty(),
            PileId::Foundation(_) => count == 1 && rules::fits_foundation(first, dst),
            PileId::Tableau(_) => match dst.last() {
                None => count <= self.max_run(true),
                Some(&top) => rules::builds_alternating(first, top),
            },
            _ => false,
        }
    }

    fn move_cards(&mut self, from: PileId, count: usize, to: PileId) -> bool {
        self.apply_move(from, count, to, false)
    }

    fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    fn undo(&mut self) -> bool {
        self.history.undo(&mut self.board)
    }

    fn auto_move_step(&mut self) -> bool {
        for from in cells().chain(tableaus()) {
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

    /// Every column is in descending order, so cards can go up lowest first.
    fn can_autocomplete(&self) -> bool {
        !self.is_won()
            && self
                .board
                .tableau
                .iter()
                .all(|t| t.windows(2).all(|w| w[0].rank > w[1].rank))
    }

    fn autocomplete_step(&mut self) -> bool {
        let mut candidates: Vec<_> = cells()
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

    fn hints(&self) -> Vec<Hint> {
        let mut scored: Vec<(u32, Hint)> = Vec::new();
        let mut push =
            |score, from, count, to| scored.push((score, Hint::Move { from, count, to }));

        for from in cells().chain(tableaus()) {
            if let Some(to) = self.foundation_for(from) {
                push(100, from, 1, to);
            }
        }

        // Out of a cell and back into play.
        for from in cells().filter(|&c| !self.pile(c).is_empty()) {
            if let Some(to) =
                tableaus().find(|&to| !self.pile(to).is_empty() && self.can_move(from, 1, to))
            {
                push(70, from, 1, to);
            }
        }

        for from in tableaus() {
            let pile = self.pile(from);
            for count in 1..=pile.len() {
                if !self.can_pick_up(from, count) {
                    break;
                }
                let idx = pile.len() - count;
                // A run already resting on its natural parent gains nothing by
                // moving to another one.
                if idx > 0 && rules::builds_alternating(pile[idx], pile[idx - 1]) {
                    continue;
                }
                let frees_foundation_card = idx > 0 && self.fits_a_foundation(pile[idx - 1]);
                let score = if idx == 0 {
                    65
                } else if frees_foundation_card {
                    80
                } else {
                    50
                };
                let onto_card = tableaus()
                    .find(|&to| !self.pile(to).is_empty() && self.can_move(from, count, to));
                if let Some(to) = onto_card {
                    push(score, from, count, to);
                } else if idx > 0 {
                    // An empty column is precious; spend it only on a run that
                    // doesn't already fill a column.
                    if let Some(to) = tableaus()
                        .find(|&to| self.pile(to).is_empty() && self.can_move(from, count, to))
                    {
                        push(30, from, count, to);
                    }
                }
            }
            // Parking the top card in a cell helps when it uncovers a card
            // that can go straight up.
            if pile.len() >= 2
                && self.fits_a_foundation(pile[pile.len() - 2])
                && let Some(to) = cells().find(|&c| self.can_move(from, 1, c))
            {
                push(60, from, 1, to);
            }
        }

        scored.sort_by_key(|&(score, _)| std::cmp::Reverse(score));
        scored.into_iter().map(|(_, h)| h).collect()
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

    fn label(c: Card) -> String {
        let rank = match c.rank {
            1 => "A".to_owned(),
            10 => "T".to_owned(),
            11 => "J".to_owned(),
            12 => "Q".to_owned(),
            13 => "K".to_owned(),
            r => r.to_string(),
        };
        let suit = match c.suit {
            Suit::Clubs => 'C',
            Suit::Diamonds => 'D',
            Suit::Hearts => 'H',
            Suit::Spades => 'S',
        };
        format!("{rank}{suit}")
    }

    fn empty_game() -> FreeCell {
        let mut g = FreeCell::new(1);
        g.board = Board {
            cells: Default::default(),
            foundations: Default::default(),
            tableau: Default::default(),
            moves: 0,
        };
        g
    }

    #[test]
    fn microsoft_deal_one() {
        let cards: Vec<String> = ms_deal(1).into_iter().map(label).collect();
        assert_eq!(cards[..8].join(" "), "JD 2D 9H JC 5D 7H 7C 5H");
        assert_eq!(cards[48..].join(" "), "6S 9C 2H 6H");
    }

    #[test]
    fn microsoft_deal_617() {
        let cards: Vec<String> = ms_deal(617).into_iter().map(label).collect();
        assert_eq!(cards[..8].join(" "), "7D AD 5C 3S 5S 8C 2D AH");
    }

    #[test]
    fn deal_numbers_round_trip() {
        assert_eq!(FreeCell::new(617).deal_number(), 617);
        assert_eq!(FreeCell::new(DEALS).deal_number(), DEALS);
        let g = FreeCell::new(1);
        assert_eq!(g.pile(PileId::Tableau(0)).len(), 7);
        assert_eq!(g.pile(PileId::Tableau(7)).len(), 6);
    }

    #[test]
    fn run_length_limited_by_free_space() {
        let mut g = empty_game();
        g.board.tableau[0] = vec![
            up(9, Suit::Spades),
            up(8, Suit::Hearts),
            up(7, Suit::Spades),
            up(6, Suit::Hearts),
        ];
        g.board.tableau[1] = vec![up(10, Suit::Diamonds)];
        for i in 2..TABLEAUS {
            g.board.tableau[i] = vec![up(13, Suit::Clubs)];
        }
        g.board.cells = [
            vec![up(1, Suit::Clubs)],
            vec![up(2, Suit::Clubs)],
            vec![],
            vec![],
        ];
        // Two free cells, no empty columns: three cards at most.
        assert!(!g.can_move(PileId::Tableau(0), 4, PileId::Tableau(1)));
        assert!(
            !g.can_move(PileId::Tableau(0), 2, PileId::Tableau(7)),
            "8 doesn't build on K"
        );
        g.board.cells[1].clear();
        assert!(
            g.can_move(PileId::Tableau(0), 4, PileId::Tableau(1)),
            "three free cells: four cards"
        );

        // An empty column doubles the limit, except when it is the destination.
        g.board.cells = [
            vec![up(1, Suit::Clubs)],
            vec![up(2, Suit::Clubs)],
            vec![up(3, Suit::Clubs)],
            vec![],
        ];
        g.board.tableau[7].clear();
        assert!(
            g.can_move(PileId::Tableau(0), 4, PileId::Tableau(1)),
            "(1 + 1) * 2 = 4"
        );
        assert!(
            !g.can_move(PileId::Tableau(0), 4, PileId::Tableau(7)),
            "into the empty column: 2"
        );
        assert!(g.can_move(PileId::Tableau(0), 2, PileId::Tableau(7)));
    }

    #[test]
    fn cells_hold_one_card_and_autocomplete_finishes() {
        let mut g = empty_game();
        for (i, suit) in Suit::ALL.iter().enumerate() {
            g.board.tableau[i] = (1..=13).rev().map(|r| up(r, *suit)).collect();
        }
        assert!(g.can_move(PileId::Tableau(0), 1, PileId::Cell(0)));
        assert!(g.move_cards(PileId::Tableau(0), 1, PileId::Cell(0)));
        assert!(
            !g.can_move(PileId::Tableau(1), 1, PileId::Cell(0)),
            "cell occupied"
        );
        assert!(g.can_autocomplete());
        while g.autocomplete_step() {}
        assert!(g.is_won());
        assert!(g.undo());
        assert_eq!(
            g.pile(PileId::Cell(0)).len(),
            0,
            "undo rolls back the auto moves and the move to the cell"
        );
    }
}
