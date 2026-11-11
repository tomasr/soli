//! A solver for Spider, used to find deals that can be won.
//!
//! It plays with full knowledge of the face-down cards and the stock, so a
//! solution it finds proves the deal can be won, though the player still has
//! to find a way through without that knowledge. The search is depth first,
//! with a table of positions already seen and a budget of positions to visit.
//! It skips moves that can't help, so failing to find a solution doesn't prove
//! there isn't one.

use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hasher};

use super::{TABLEAUS, deal};
use crate::card::Card;
use crate::game::{Hint, PileId, SpiderSuits};

/// A card packed into a byte: rank in the low four bits, suit in the next two,
/// face up in bit 6. Copies of the same card are interchangeable here.
type Packed = u8;

const FACE_UP: Packed = 0x40;

fn pack(card: Card) -> Packed {
    card.rank | (card.suit as u8) << 4 | if card.face_up { FACE_UP } else { 0 }
}

fn rank(c: Packed) -> u8 {
    c & 0x0f
}

fn suit(c: Packed) -> u8 {
    (c >> 4) & 0x03
}

fn face_up(c: Packed) -> bool {
    c & FACE_UP != 0
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Move {
    Run { from: u8, to: u8, count: u8 },
    Deal,
}

/// What a move changed besides moving cards, so it can be undone.
#[derive(Clone, Copy, Default)]
struct Undo {
    /// The source column's new top card was turned up.
    flipped: bool,
    /// Bit sets over columns: which completed a run, and which had the card
    /// under that run turned up.
    collected: u16,
    collected_flipped: u16,
    /// Suit of each column's completed run, two bits per column.
    collected_suits: u32,
}

impl Undo {
    fn record(&mut self, col: usize, collected: Option<(u8, bool)>) {
        if let Some((s, flipped)) = collected {
            self.collected |= 1 << col;
            self.collected_flipped |= (flipped as u16) << col;
            self.collected_suits |= (s as u32) << (col * 2);
        }
    }
}

#[derive(Clone)]
struct State {
    tableau: [Vec<Packed>; TABLEAUS],
    /// Dealt from the end.
    stock: Vec<Packed>,
    completed: u8,
}

impl State {
    /// Length of the face-up, same-suit, descending run at the end of a column.
    fn run_len(col: &[Packed]) -> usize {
        let Some(&top) = col.last() else {
            return 0;
        };
        let mut n = 1;
        let mut prev = top;
        for &c in col.iter().rev().skip(1) {
            if !face_up(c) || suit(c) != suit(prev) || rank(c) != rank(prev) + 1 {
                break;
            }
            n += 1;
            prev = c;
        }
        n
    }

    /// Removes a completed King-to-Ace run from the end of column `i`, and
    /// returns its suit and whether the card it uncovered was turned up.
    fn collect(&mut self, i: usize) -> Option<(u8, bool)> {
        let col = &mut self.tableau[i];
        if col.len() < 13 || Self::run_len(col) < 13 || rank(col[col.len() - 13]) != 13 {
            return None;
        }
        let s = suit(col[col.len() - 1]);
        col.truncate(col.len() - 13);
        self.completed += 1;
        let flipped = match col.last_mut() {
            Some(top) if !face_up(*top) => {
                *top |= FACE_UP;
                true
            }
            _ => false,
        };
        Some((s, flipped))
    }

    fn apply(&mut self, m: Move) -> Undo {
        let mut undo = Undo::default();
        match m {
            Move::Run { from, to, count } => {
                let (from, to) = (from as usize, to as usize);
                let src = &mut self.tableau[from];
                let start = src.len() - count as usize;
                let moving: Vec<Packed> = src.drain(start..).collect();
                if let Some(top) = src.last_mut()
                    && !face_up(*top)
                {
                    *top |= FACE_UP;
                    undo.flipped = true;
                }
                self.tableau[to].extend(moving);
                let collected = self.collect(to);
                undo.record(to, collected);
            }
            Move::Deal => {
                for col in &mut self.tableau {
                    let card = self.stock.pop().expect("stock deals in tens");
                    col.push(card | FACE_UP);
                }
                for i in 0..TABLEAUS {
                    let collected = self.collect(i);
                    undo.record(i, collected);
                }
            }
        }
        undo
    }

    fn revert(&mut self, m: Move, undo: Undo) {
        for i in 0..TABLEAUS {
            if undo.collected & (1 << i) == 0 {
                continue;
            }
            let col = &mut self.tableau[i];
            if undo.collected_flipped & (1 << i) != 0 {
                *col.last_mut().expect("flipped card is there") &= !FACE_UP;
            }
            let s = ((undo.collected_suits >> (i * 2)) & 3) as u8;
            col.extend((1..=13).rev().map(|r| r | s << 4 | FACE_UP));
            self.completed -= 1;
        }
        match m {
            Move::Run { from, to, count } => {
                let (from, to) = (from as usize, to as usize);
                if undo.flipped {
                    *self.tableau[from]
                        .last_mut()
                        .expect("flipped card is there") &= !FACE_UP;
                }
                let dst = &mut self.tableau[to];
                let moving: Vec<Packed> = dst.drain(dst.len() - count as usize..).collect();
                self.tableau[from].extend(moving);
            }
            Move::Deal => {
                for col in self.tableau.iter_mut().rev() {
                    let card = col.pop().expect("dealt card is there");
                    self.stock.push(card & !FACE_UP);
                }
            }
        }
    }

    /// How promising a position is: completed runs, turned-up cards, empty
    /// columns and cards stacked in order count for it.
    fn eval(&self) -> i32 {
        let mut score = self.completed as i32 * 1000;
        for col in &self.tableau {
            if col.is_empty() {
                score += 150;
                continue;
            }
            let down = col.iter().take_while(|&&c| !face_up(c)).count();
            score -= down as i32 * 40;
            for w in col[down..].windows(2) {
                let (hi, lo) = (w[0], w[1]);
                score += if rank(hi) != rank(lo) + 1 {
                    -12
                } else if suit(hi) == suit(lo) {
                    10
                } else {
                    2
                };
            }
        }
        score
    }

    fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |b: u8| h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
        for col in &self.tableau {
            for &c in col {
                feed(c);
            }
            feed(0xff);
        }
        feed(self.stock.len() as u8);
        h
    }

    /// Moves worth trying, best first.
    fn moves(&self) -> Vec<Move> {
        let mut scored: Vec<(i32, Move)> = Vec::new();
        let first_empty = self.tableau.iter().position(|c| c.is_empty());

        for (from, col) in self.tableau.iter().enumerate() {
            if col.is_empty() {
                continue;
            }
            let run = Self::run_len(col);
            let base = col[col.len() - run];
            let below = (col.len() > run).then(|| col[col.len() - run - 1]);
            let whole = below.is_none();
            let reveals = below.is_some_and(|b| !face_up(b));
            // Sitting on a card one higher, of another suit (the same suit
            // would have made it part of the run).
            let seated = below.is_some_and(|b| face_up(b) && rank(b) == rank(base) + 1);
            // Moving a seated run is only worth it to put it on its own suit,
            // or to make room for a card of the right suit that's waiting.
            let wanted = seated && {
                let b = below.unwrap();
                self.tableau.iter().enumerate().any(|(j, other)| {
                    j != from && other.last().is_some() && {
                        let n = Self::run_len(other);
                        let ob = other[other.len() - n];
                        rank(ob) == rank(base) && suit(ob) == suit(b)
                    }
                })
            };

            for (to, dst) in self.tableau.iter().enumerate() {
                if to == from {
                    continue;
                }
                let score = match dst.last() {
                    None => {
                        if whole || Some(to) != first_empty || (seated && !wanted) {
                            continue;
                        }
                        if reveals { 30 } else { 5 }
                    }
                    Some(&top) if rank(top) == rank(base) + 1 => {
                        let same = suit(top) == suit(base);
                        if seated && !same && !wanted {
                            continue;
                        }
                        let mut s = if same { 100 } else { 0 };
                        if reveals {
                            s += 50;
                        }
                        if whole {
                            s += 40;
                        }
                        if !seated && !reveals && !whole {
                            s += 20;
                        }
                        s + run as i32
                    }
                    Some(_) => continue,
                };
                scored.push((
                    score,
                    Move::Run {
                        from: from as u8,
                        to: to as u8,
                        count: run as u8,
                    },
                ));
            }
        }
        if first_empty.is_none() && !self.stock.is_empty() {
            scored.push((-1, Move::Deal));
        }
        scored.sort_by_key(|&(s, _)| std::cmp::Reverse(s));
        scored.into_iter().map(|(_, m)| m).collect()
    }
}

/// The positions are already hashed, so the set uses the hash as is.
#[derive(Default)]
struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, _: &[u8]) {
        unreachable!("only u64 keys")
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
}

/// One position kept in the beam: where it came from, for rebuilding the
/// solution.
#[derive(Clone, Copy)]
struct Step {
    parent: u32,
    via: Move,
}

/// Search settings: how many positions are kept at each step, and how many
/// steps are taken between deals.
pub struct Effort {
    pub width: usize,
    pub stage_steps: usize,
}

impl Effort {
    pub const DEFAULT: Effort = Effort {
        width: 400,
        stage_steps: 60,
    };
}

type Seen = HashSet<u64, BuildHasherDefault<IdentityHasher>>;

/// Looks for a way to win the deal for `seed`. Returns the moves, as hints a
/// [`super::Spider`] can replay.
///
/// It's a beam search, split into stages at the deals: within a stage it keeps
/// the `width` best positions at each step, for `stage_steps` steps, and
/// remembers the best positions it saw that a deal can be made from. The next
/// stage starts by dealing onto those.
pub fn solve(seed: u64, suits: SpiderSuits, effort: &Effort) -> Option<Vec<Hint>> {
    let (tableau, stock) = deal(seed, suits);
    let start = State {
        tableau: tableau.map(|col| col.into_iter().map(pack).collect()),
        stock: stock.into_iter().map(pack).collect(),
        completed: 0,
    };
    // Every kept position's step; index 0 is the start.
    let mut steps = vec![Step {
        parent: u32::MAX,
        via: Move::Deal,
    }];
    let mut seen = Seen::default();
    seen.insert(start.hash());
    let mut beam = vec![(start, 0u32)];

    loop {
        let last_stage = beam.iter().all(|(s, _)| s.stock.is_empty());
        let limit = if last_stage {
            effort.stage_steps * 4
        } else {
            effort.stage_steps
        };
        let mut dealable: Vec<(i32, State, u32)> = Vec::new();
        let keep_dealable = |beam: &[(State, u32)], dealable: &mut Vec<(i32, State, u32)>| {
            for (s, idx) in beam {
                if !s.stock.is_empty() && s.tableau.iter().all(|c| !c.is_empty()) {
                    dealable.push((s.eval(), s.clone(), *idx));
                }
            }
            if dealable.len() > effort.width * 2 {
                dealable.sort_by_key(|d| std::cmp::Reverse(d.0));
                dealable.truncate(effort.width);
            }
        };
        keep_dealable(&beam, &mut dealable);

        for _ in 0..limit {
            let mut candidates: Vec<(i32, u64, usize, Move)> = Vec::new();
            for (i, (state, _)) in beam.iter_mut().enumerate() {
                for m in state.moves() {
                    if m == Move::Deal {
                        continue;
                    }
                    let undo = state.apply(m);
                    if state.completed == 8 {
                        steps.push(Step {
                            parent: beam[i].1,
                            via: m,
                        });
                        return Some(solution(&steps));
                    }
                    let h = state.hash();
                    if !seen.contains(&h) {
                        candidates.push((state.eval(), h, i, m));
                    }
                    state.revert(m, undo);
                }
            }
            if candidates.is_empty() {
                break;
            }
            candidates.sort_by_key(|c| std::cmp::Reverse(c.0));
            let mut next = Vec::with_capacity(effort.width);
            for (_, h, i, m) in candidates {
                if next.len() == effort.width {
                    break;
                }
                if !seen.insert(h) {
                    continue;
                }
                let (parent, parent_idx) = &beam[i];
                let mut child = parent.clone();
                child.apply(m);
                steps.push(Step {
                    parent: *parent_idx,
                    via: m,
                });
                next.push((child, (steps.len() - 1) as u32));
            }
            beam = next;
            keep_dealable(&beam, &mut dealable);
        }

        if last_stage || dealable.is_empty() {
            return None;
        }
        dealable.sort_by_key(|d| std::cmp::Reverse(d.0));
        dealable.truncate(effort.width);
        beam = dealable
            .into_iter()
            .map(|(_, mut s, idx)| {
                s.apply(Move::Deal);
                seen.insert(s.hash());
                steps.push(Step {
                    parent: idx,
                    via: Move::Deal,
                });
                (s, (steps.len() - 1) as u32)
            })
            .collect();
    }
}

/// Follows the steps back from the last one to the start.
fn solution(steps: &[Step]) -> Vec<Hint> {
    let mut moves = Vec::new();
    let mut i = steps.len() - 1;
    while i != 0 {
        moves.push(steps[i].via);
        i = steps[i].parent as usize;
    }
    moves.reverse();
    moves
        .into_iter()
        .map(|m| match m {
            Move::Run { from, to, count } => Hint::Move {
                from: PileId::Tableau(from as usize),
                count: count as usize,
                to: PileId::Tableau(to as usize),
            },
            Move::Deal => Hint::Stock,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Game;
    use crate::spider::Spider;

    fn replay(seed: u64, suits: SpiderSuits, solution: &[Hint]) -> bool {
        let mut g = Spider::new(seed, suits);
        for &h in solution {
            let ok = match h {
                Hint::Move { from, count, to } => g.move_cards(from, count, to),
                Hint::Stock => g.click_stock(),
            };
            assert!(ok, "solver move {h:?} is legal");
        }
        g.is_won()
    }

    #[test]
    fn solutions_replay_to_a_win() {
        // Deals the solver is known to win.
        for (suits, seed) in [
            (SpiderSuits::One, 1),
            (SpiderSuits::Two, 1),
            (SpiderSuits::Four, 3),
        ] {
            let solution = solve(seed, suits, &Effort::DEFAULT).expect("solvable");
            assert!(replay(seed, suits, &solution), "seed {seed} {suits:?}");
        }
    }

    #[test]
    fn winnable_seed_finds_a_deal_the_solver_wins() {
        let seed = crate::spider::winnable_seed(0, SpiderSuits::Four);
        let solution = solve(seed, SpiderSuits::Four, &Effort::DEFAULT).expect("solvable");
        assert!(replay(seed, SpiderSuits::Four, &solution));
    }

    /// `cargo test --release -p soli-core solve_rate -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn solve_rate() {
        for suits in [SpiderSuits::One, SpiderSuits::Two, SpiderSuits::Four] {
            for width in [100, 400, 1500] {
                let effort = Effort {
                    width,
                    stage_steps: 60,
                };
                let start = std::time::Instant::now();
                let n = 50;
                let found = (0..n)
                    .filter(|&s| solve(s, suits, &effort).is_some())
                    .count();
                println!(
                    "{suits:?} width {width}: {found}/{n} in {:.2}s",
                    start.elapsed().as_secs_f64()
                );
            }
        }
    }
}
