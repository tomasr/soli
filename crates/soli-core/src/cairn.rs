//! Cairn: two decks, 104 cards. Most are dealt face up into an overlapping
//! layout; the rest form a stock drawn one card at a time onto a waste pile.
//! A card is free when no card above it overlaps it. Two free cards of the
//! same kind are removed as a pair; the top waste card and the reserve card
//! count as free. Clearing the layout wins.
//!
//! The level sets the rest (see [`Rules::for_level`]): whether a pair must
//! share colour as well as rank, whether a layout card also needs an open
//! side as in Mahjong, how many passes through the stock are allowed,
//! whether there is a one-card reserve, and how many traps the deal holds.
//!
//! Every deal is built around a solution (see [`generate`]), so every deal
//! can be won.

use crate::card::{Card, Color, Suit};
use crate::game::{CairnLevel, Game, Hint, History, PileId, SlotPos, Variant};
use crate::rng::SplitMix64;

const DEALS: u64 = 1_000_000;
const RESERVE: PileId = PileId::Cell(0);
const MATCHED: PileId = PileId::Foundation(0);
/// Most stock cards the built-in solution pairs with layout cards.
const MAX_STOCK_PAIRS: usize = 12;
/// Chance (percent) that a step of the built-in solution uses the stock even
/// when two layout cards could pair.
const STOCK_PAIR_CHANCE: u64 = 20;

/// Rule options, for trying out how hard the game should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rules {
    /// Pairs must share colour as well as rank, leaving four cards of each
    /// kind instead of eight.
    pub same_colour: bool,
    /// Like Mahjong: a layout card also needs nothing beside it on its left
    /// or its right.
    pub open_side: bool,
    /// Passes allowed through the stock.
    pub passes: u32,
    pub reserve: bool,
    /// Most stock cards the built-in solution pairs with layout cards.
    pub max_stock_pairs: usize,
    /// Chance (percent) that a step of the built-in solution uses the stock
    /// even when two layout cards could pair.
    pub stock_pair_chance: u64,
    /// The built-in solution avoids pairing two cards freed by the same move.
    pub spread: bool,
    /// Chance (percent) that a solution pair shares its kind with a later
    /// pair it lies over, so pairing the wrong two copies strands the
    /// fourth.
    pub traps: u8,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            same_colour: false,
            open_side: false,
            passes: 3,
            reserve: true,
            max_stock_pairs: MAX_STOCK_PAIRS,
            stock_pair_chance: STOCK_PAIR_CHANCE,
            spread: true,
            traps: 0,
        }
    }
}

impl Rules {
    pub fn for_level(level: CairnLevel) -> Self {
        match level {
            CairnLevel::Easy => Rules::default(),
            CairnLevel::Normal => Rules {
                same_colour: true,
                open_side: true,
                passes: 2,
                traps: 20,
                ..Rules::default()
            },
            CairnLevel::Hard => Rules {
                same_colour: true,
                open_side: true,
                passes: 1,
                reserve: false,
                traps: 40,
                ..Rules::default()
            },
        }
    }

    /// Cards with the same kind pair up.
    fn kind(&self, card: Card) -> (u8, Option<Color>) {
        (card.rank, self.same_colour.then(|| card.color()))
    }

    fn kinds(&self) -> usize {
        if self.same_colour { 26 } else { 13 }
    }
}

/// A layout shape: where each slot sits, in half-card units, and how high.
pub struct Shape {
    pub name: &'static str,
    pub slots: Vec<SlotPos>,
}

fn pos(x: i32, y: i32, z: i32) -> SlotPos {
    SlotPos {
        x: x as i8,
        y: y as i8,
        z: z as u8,
    }
}

/// Rows that each overlap the bottom half of the row above, like a pyramid.
/// `rows` gives each row's cards as x positions.
fn stacked_rows(name: &'static str, rows: Vec<Vec<i32>>) -> Shape {
    let slots = rows
        .into_iter()
        .enumerate()
        .flat_map(|(r, xs)| xs.into_iter().map(move |x| pos(x, r as i32, r as i32)))
        .collect();
    Shape { name, slots }
}

/// `n` cards side by side, starting at `x0`.
fn row(x0: i32, n: i32) -> Vec<i32> {
    (0..n).map(|i| x0 + 2 * i).collect()
}

fn peak() -> Shape {
    // Rows of 4 to 12, centred: 72 cards.
    stacked_rows("Peak", (4..=12).map(|n| row(12 - n, n)).collect())
}

fn heart() -> Shape {
    // Two peaks of 2 to 6 that meet, then a base narrowing from 11 to 9.
    let mut rows: Vec<Vec<i32>> = (0..5)
        .map(|r| {
            let mut xs = row(4 - r, r + 2);
            xs.extend(row(16 - r, r + 2));
            xs
        })
        .collect();
    for (r, n) in [11, 10, 9].into_iter().enumerate() {
        rows.push(row(r as i32 + 1, n));
    }
    stacked_rows("Heart", rows)
}

fn hive() -> Shape {
    // Rows widening from 6 to 11, then narrowing to 9: 70 cards.
    let widths = [6, 7, 8, 9, 10, 11, 10, 9];
    stacked_rows("Hive", widths.into_iter().map(|n| row(11 - n, n)).collect())
}

pub fn shapes() -> Vec<Shape> {
    vec![peak(), heart(), hive()]
}

/// `upper` lies over `lower`.
fn covers(upper: SlotPos, lower: SlotPos) -> bool {
    upper.z > lower.z && (upper.x - lower.x).abs() < 2 && (upper.y - lower.y).abs() < 2
}

/// `b` sits directly beside `a` at the same height, `dx` half-cards across.
fn beside(a: SlotPos, b: SlotPos, dx: i8) -> bool {
    a.z == b.z && b.x - a.x == dx && (a.y - b.y).abs() < 2
}

/// Which slots block which.
struct Geometry {
    /// Slots each slot lies over.
    below: Vec<Vec<usize>>,
    /// Slots lying over each slot.
    above: Vec<Vec<usize>>,
    left: Vec<Vec<usize>>,
    right: Vec<Vec<usize>>,
}

impl Geometry {
    fn new(slots: &[SlotPos]) -> Self {
        let n = slots.len();
        let related = |f: &dyn Fn(SlotPos, SlotPos) -> bool| -> Vec<Vec<usize>> {
            (0..n)
                .map(|i| (0..n).filter(|&j| f(slots[i], slots[j])).collect())
                .collect()
        };
        Geometry {
            below: related(&|a, b| covers(a, b)),
            above: related(&|a, b| covers(b, a)),
            left: related(&|a, b| beside(a, b, -2)),
            right: related(&|a, b| beside(a, b, 2)),
        }
    }

    /// For each slot, the slots that must go before it can be freed:
    /// everything lying over it, and over those, and so on.
    fn buried_under(&self) -> Vec<u128> {
        let n = self.above.len();
        assert!(n <= 128, "layouts are at most 128 slots");
        // Slots are listed bottom first, so everything over slot `i` comes
        // later, and filling from the end finds each closure in one pass.
        let mut out = vec![0u128; n];
        for i in (0..n).rev() {
            out[i] = self.above[i]
                .iter()
                .fold(0, |m, &j| m | 1u128 << j | out[j]);
        }
        out
    }

    /// Slot `i` can be played when the slots for which `present` is true are
    /// on the table (ignoring whether `i` itself is).
    fn is_free(&self, i: usize, rules: &Rules, present: impl Fn(usize) -> bool) -> bool {
        let clear = |set: &[usize]| set.iter().all(|&j| !present(j));
        clear(&self.above[i]) && (!rules.open_side || clear(&self.left[i]) || clear(&self.right[i]))
    }
}

/// One step of a solution, in the order it is played. Only tests replay
/// solutions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum Step {
    /// Pair two cards, or put one in the reserve.
    Move(PileId, PileId),
    Draw,
}

#[derive(Clone)]
pub(crate) struct Deal {
    pub shape: usize,
    pub slots: Vec<Option<Card>>,
    /// Top of the stock last.
    pub stock: Vec<Card>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub solution: Vec<Step>,
}

/// Builds a deal around a solution. Playing the still-blank layout forwards,
/// each step takes two free slots (or one, plus a card added to the stock)
/// and later gives both cards the same kind, so the deal can always be won,
/// using the stock in a single pass.
///
/// Any such solution pairs cards that are free at the same time, so a freed
/// card often finds its partner waiting. To keep that from looking arranged,
/// the solution avoids pairing two cards freed by the same move
/// ([`Rules::spread`]); `pair_locality` in the tests measures the effect.
///
/// Left at that, pairing any two free cards of a kind is almost always safe.
/// Traps ([`Rules::traps`]) give some kinds a copy lying over another, so
/// the wrong choice strands a card. Shuffling deals and keeping those a
/// solver can win was tried instead: under the colour and open-side rules,
/// even a quarter of the cards shuffled left almost no deal winnable.
pub(crate) fn generate(seed: u64, rules: &Rules) -> Deal {
    generate_with(seed, rules, Build::Forward)
}

/// A step of a solution before cards are chosen.
#[derive(Clone, Copy)]
enum Fill {
    /// Two layout slots.
    Pair(usize, usize),
    /// A layout slot and a stock card.
    WithStock(usize),
}

/// Chooses a kind for each step of a solution (given in playing order), at
/// most `104 / 2 / kinds` steps per kind. With `free_before`, some steps
/// share their kind with a later pair they lie over (see [`Rules::traps`]).
fn assign_kinds(
    steps: &[Fill],
    free_before: Option<&[u128]>,
    buried_under: &[u128],
    rules: &Rules,
    rng: &mut SplitMix64,
) -> Vec<usize> {
    let m = steps.len();
    let slots_of = |f: Fill| match f {
        Fill::Pair(a, b) => vec![a, b],
        Fill::WithStock(a) => vec![a],
    };
    // Steps that must share a kind.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut grouped = vec![false; m];
    for i in 0..m {
        if grouped[i] {
            continue;
        }
        grouped[i] = true;
        let mut group = vec![i];
        if let Some(free) = free_before
            && rng.below(100) < u64::from(rules.traps)
        {
            let over = slots_of(steps[i]);
            // A later pair with one card under this step's and the other
            // free already: a tempting wrong partner.
            let later: Vec<usize> = (i + 1..m)
                .filter(|&j| !grouped[j])
                .filter(|&j| {
                    let Fill::Pair(b1, b2) = steps[j] else {
                        return false;
                    };
                    [(b1, b2), (b2, b1)].into_iter().any(|(under, tempting)| {
                        free[i] >> tempting & 1 == 1
                            && over.iter().any(|&a| buried_under[under] >> a & 1 == 1)
                    })
                })
                .collect();
            if !later.is_empty() {
                let j = later[rng.below(later.len() as u64) as usize];
                grouped[j] = true;
                group.push(j);
            }
        }
        groups.push(group);
    }

    // Larger groups first, so each still finds a kind with room.
    let kinds = rules.kinds();
    let mut room = vec![104 / 2 / kinds; kinds];
    rng.shuffle(&mut groups);
    groups.sort_by_key(|g| std::cmp::Reverse(g.len()));
    let mut out = vec![0; m];
    for group in groups {
        let fits: Vec<usize> = (0..kinds).filter(|&k| room[k] >= group.len()).collect();
        let k = fits[rng.below(fits.len() as u64) as usize];
        room[k] -= group.len();
        for i in group {
            out[i] = k;
        }
    }
    out
}

/// Ways to build a deal; only the default is used outside experiments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum Build {
    /// Fill the layout from the bottom up, a pair at a time. Deals came out
    /// slightly harder this way, but with more pairs freed by a single move.
    Backward,
    /// Play the empty layout forwards, taking any two free slots as a pair.
    Forward,
    /// Shuffle and deal, with no promise that the deal can be won.
    Random,
}

pub(crate) fn generate_with(seed: u64, rules: &Rules, build: Build) -> Deal {
    let mut rng = SplitMix64::new(seed);
    let shapes = shapes();
    let shape = rng.below(shapes.len() as u64) as usize;
    let slots = &shapes[shape].slots;
    let n = slots.len();
    let geo = Geometry::new(slots);
    let stock_len = 104 - n;

    // Slot `i` can go in next: what it lies over is in place, it would be
    // free, and no empty neighbour would be boxed in for good by it.
    let can_place = |i: usize, placed: &[bool]| {
        let with_i = |j: usize| placed[j] || j == i;
        !placed[i]
            && geo.below[i].iter().all(|&j| placed[j])
            && geo.is_free(i, rules, |j| placed[j])
            && geo.left[i]
                .iter()
                .chain(&geo.right[i])
                .all(|&k| placed[k] || geo.is_free(k, rules, with_i))
    };

    // Fill slots, last-removed first. A dead end (one slot left to fill and
    // no stock cards to spare) just starts over.
    let pick = |rng: &mut SplitMix64, from: &mut Vec<usize>| {
        from.swap_remove(rng.below(from.len() as u64) as usize)
    };
    // Free slots before each step of a forward-built solution, in playing
    // order.
    let mut free_before: Option<Vec<u128>> = None;
    let fills = if build == Build::Forward {
        // Steps in playing order: two slots free now become a pair.
        'attempt: loop {
            let mut frees = Vec::new();
            let mut present = vec![true; n];
            // The step at which each slot became free.
            let mut freed_at = vec![usize::MAX; n];
            let mut steps = Vec::new();
            let mut with_stock = 0;
            let mut left = n;
            while left > 0 {
                let mut free: Vec<usize> = (0..n)
                    .filter(|&i| present[i] && geo.is_free(i, rules, |j| present[j]))
                    .collect();
                for &i in &free {
                    freed_at[i] = freed_at[i].min(steps.len());
                }
                frees.push(free.iter().fold(0u128, |m, &i| m | 1u128 << i));
                let can_stock = with_stock < rules.max_stock_pairs.min(stock_len);
                let use_stock =
                    can_stock && (free.len() == 1 || rng.below(100) < rules.stock_pair_chance);
                if use_stock {
                    let a = pick(&mut rng, &mut free);
                    present[a] = false;
                    steps.push(Fill::WithStock(a));
                    with_stock += 1;
                    left -= 1;
                } else if free.len() >= 2 {
                    let a = pick(&mut rng, &mut free);
                    let apart: Vec<usize> = free
                        .iter()
                        .copied()
                        .filter(|&b| freed_at[b] != freed_at[a])
                        .collect();
                    let b = if rules.spread && !apart.is_empty() {
                        let b = apart[rng.below(apart.len() as u64) as usize];
                        free.retain(|&x| x != b);
                        b
                    } else {
                        pick(&mut rng, &mut free)
                    };
                    present[a] = false;
                    present[b] = false;
                    steps.push(Fill::Pair(a, b));
                    left -= 2;
                } else {
                    continue 'attempt;
                }
            }
            // The rest of this function wants them last-removed first.
            steps.reverse();
            free_before = Some(frees);
            break steps;
        }
    } else {
        'attempt: loop {
            let mut placed = vec![false; n];
            let mut fills = Vec::new();
            let mut with_stock = 0;
            let mut left = n;
            while left > 0 {
                let mut open: Vec<usize> = (0..n).filter(|&i| can_place(i, &placed)).collect();
                if open.is_empty() {
                    continue 'attempt;
                }
                let can_stock = with_stock < rules.max_stock_pairs.min(stock_len);
                let a = pick(&mut rng, &mut open);
                placed[a] = true;
                // Its partner must be placeable alongside it, and leave it free.
                let partners: Vec<usize> = if can_stock && rng.below(100) < rules.stock_pair_chance
                {
                    Vec::new()
                } else {
                    (0..n)
                        .filter(|&b| {
                            can_place(b, &placed)
                                && !covers(slots[b], slots[a])
                                && geo.is_free(a, rules, |j| placed[j] || j == b)
                        })
                        .collect()
                };
                if let Some(&b) = partners.get(rng.below(partners.len().max(1) as u64) as usize) {
                    placed[b] = true;
                    fills.push(Fill::Pair(a, b));
                    left -= 2;
                } else if can_stock {
                    fills.push(Fill::WithStock(a));
                    with_stock += 1;
                    left -= 1;
                } else {
                    continue 'attempt;
                }
            }
            break fills;
        }
    };

    // Each step takes a pair of one kind, out of the two decks.
    let mut by_kind: Vec<Vec<Card>> = vec![Vec::new(); rules.kinds()];
    let mut kind_index = Vec::new();
    for copy in 0..2u8 {
        for rank in 1..=13u8 {
            for &suit in &Suit::ALL {
                let card = Card {
                    copy,
                    ..Card::new(rank, suit).turned_up()
                };
                let kind = rules.kind(card);
                let k = match kind_index.iter().position(|&x| x == kind) {
                    Some(k) => k,
                    None => {
                        kind_index.push(kind);
                        kind_index.len() - 1
                    }
                };
                by_kind[k].push(card);
            }
        }
    }
    for cards in &mut by_kind {
        rng.shuffle(cards);
    }
    let forward: Vec<Fill> = fills.iter().rev().copied().collect();
    let mut kinds = assign_kinds(
        &forward,
        free_before.as_deref(),
        &geo.buried_under(),
        rules,
        &mut rng,
    )
    .into_iter();

    let mut deal_slots = vec![None; n];
    let mut solution = Vec::with_capacity(fills.len());
    // Layout slots paired with stock cards, alongside the card they need.
    let mut stock_pairs: Vec<(usize, Card)> = Vec::new();
    let mut used = Vec::new();
    for fill in fills.into_iter().rev() {
        let cards = &mut by_kind[kinds.next().expect("one kind per step")];
        let (c1, c2) = (cards.pop().unwrap(), cards.pop().unwrap());
        match fill {
            Fill::Pair(a, b) => {
                deal_slots[a] = Some(c1);
                deal_slots[b] = Some(c2);
                solution.push(Step::Move(PileId::Slot(a), PileId::Slot(b)));
            }
            Fill::WithStock(a) => {
                deal_slots[a] = Some(c1);
                let c2 = Card {
                    face_up: false,
                    ..c2
                };
                used.push(c2);
                stock_pairs.push((a, c2));
                // Filled in once the stock's order is known.
                solution.push(Step::Draw);
            }
        }
    }

    // The rest of the stock goes in at random, keeping the cards the
    // solution needs in the order it needs them.
    let mut spare: Vec<Card> = by_kind
        .into_iter()
        .flatten()
        .map(|c| Card {
            face_up: false,
            ..c
        })
        .collect();
    rng.shuffle(&mut spare);
    let mut draw_order = Vec::with_capacity(stock_len);
    let (mut u, mut s) = (used.into_iter().peekable(), spare.into_iter().peekable());
    let mut used_left = u.len();
    while draw_order.len() < stock_len {
        let total = (stock_len - draw_order.len()) as u64;
        let next = if (rng.below(total) as usize) < used_left {
            used_left -= 1;
            u.next()
        } else {
            s.next()
        };
        draw_order.push(next.expect("counts add up"));
    }
    draw_order.reverse();

    // Each stock pair draws until its card is on top of the waste.
    let mut drawn = draw_order.iter().rev();
    let mut steps = Vec::with_capacity(solution.len() + stock_len);
    let mut stock_pairs = stock_pairs.into_iter();
    for step in solution {
        if step != Step::Draw {
            steps.push(step);
            continue;
        }
        let (a, card) = stock_pairs.next().expect("one per stock pair");
        loop {
            steps.push(Step::Draw);
            if drawn.next().expect("the card is in the stock").id() == card.id() {
                break;
            }
        }
        steps.push(Step::Move(PileId::Waste, PileId::Slot(a)));
    }
    let mut solution = steps;

    if build == Build::Random {
        let mut all: Vec<Card> = deal_slots
            .iter()
            .flatten()
            .copied()
            .chain(draw_order)
            .collect();
        rng.shuffle(&mut all);
        let rest = all.split_off(n);
        deal_slots = all
            .into_iter()
            .map(|c| Some(Card { face_up: true, ..c }))
            .collect();
        draw_order = rest
            .into_iter()
            .map(|c| Card {
                face_up: false,
                ..c
            })
            .collect();
        solution.clear();
    }

    Deal {
        shape,
        slots: deal_slots,
        stock: draw_order,
        solution,
    }
}

#[derive(Debug, Clone)]
struct Board {
    slots: Vec<Option<Card>>,
    stock: Vec<Card>,
    waste: Vec<Card>,
    reserve: Vec<Card>,
    matched: Vec<Card>,
    /// Passes through the stock started, including the current one.
    pass: u32,
    moves: u32,
}

pub struct Cairn {
    board: Board,
    history: History<Board>,
    deal: u64,
    level: CairnLevel,
    shape: Shape,
    geo: Geometry,
    rules: Rules,
    #[cfg(test)]
    solution: Vec<Step>,
}

impl Cairn {
    /// Any seed maps onto deals 1..=1,000,000; a deal number maps to itself.
    pub fn new(seed: u64, level: CairnLevel) -> Self {
        Self::with_rules(seed, level, Rules::for_level(level))
    }

    fn with_rules(seed: u64, level: CairnLevel, rules: Rules) -> Self {
        let deal = seed.wrapping_sub(1) % DEALS + 1;
        let d = generate(deal, &rules);
        let shape = shapes().swap_remove(d.shape);
        let geo = Geometry::new(&shape.slots);
        Cairn {
            board: Board {
                slots: d.slots,
                stock: d.stock,
                waste: Vec::new(),
                reserve: Vec::new(),
                matched: Vec::new(),
                pass: 1,
                moves: 0,
            },
            history: History::new(),
            deal,
            level,
            shape,
            geo,
            rules,
            #[cfg(test)]
            solution: d.solution,
        }
    }

    pub fn shape_name(&self) -> &'static str {
        self.shape.name
    }

    fn slot_ids(&self) -> impl Iterator<Item = PileId> + use<> {
        (0..self.board.slots.len()).map(PileId::Slot)
    }

    fn is_free_slot(&self, i: usize) -> bool {
        let slots = &self.board.slots;
        slots[i].is_some() && self.geo.is_free(i, &self.rules, |j| slots[j].is_some())
    }

    /// The playable card of a pile, if it has one.
    fn free_card(&self, id: PileId) -> Option<Card> {
        match id {
            PileId::Slot(i) => self.is_free_slot(i).then(|| self.board.slots[i]).flatten(),
            PileId::Waste | PileId::Cell(0) => self.pile(id).last().copied(),
            _ => None,
        }
    }

    /// Every pile with a playable card, layout first.
    fn free_piles(&self) -> Vec<PileId> {
        self.slot_ids()
            .chain([PileId::Waste, RESERVE])
            .filter(|&p| self.free_card(p).is_some())
            .collect()
    }

    fn take(&mut self, id: PileId) -> Card {
        let b = &mut self.board;
        match id {
            PileId::Slot(i) => b.slots[i].take().expect("slot has a card"),
            PileId::Waste => b.waste.pop().expect("waste has a card"),
            PileId::Cell(_) => b.reserve.pop().expect("reserve has a card"),
            _ => unreachable!("cards are only taken from play"),
        }
    }

    fn apply_move(&mut self, from: PileId, to: PileId, automatic: bool) -> bool {
        if !self.can_move(from, 1, to) {
            return false;
        }
        self.history.push(&self.board, automatic);
        let card = self.take(from);
        if to == RESERVE && self.board.reserve.is_empty() {
            self.board.reserve.push(card);
        } else {
            let partner = self.take(to);
            self.board.matched.push(partner);
            self.board.matched.push(card);
        }
        self.board.moves += 1;
        true
    }

    /// Slots that removing these would leave free.
    fn uncovered_by(&self, removed: &[PileId]) -> usize {
        let slots = &self.board.slots;
        let gone = |j: usize| removed.contains(&PileId::Slot(j));
        (0..slots.len())
            .filter(|&i| slots[i].is_some() && !gone(i) && !self.is_free_slot(i))
            .filter(|&i| {
                self.geo
                    .is_free(i, &self.rules, |j| slots[j].is_some() && !gone(j))
            })
            .count()
    }

    fn layout_empty(&self) -> bool {
        self.board.slots.iter().all(Option::is_none)
    }
}

impl Game for Cairn {
    fn variant(&self) -> Variant {
        Variant::Cairn(self.level)
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
        [PileId::Stock, PileId::Waste]
            .into_iter()
            .chain(self.rules.reserve.then_some(RESERVE))
            .chain([MATCHED])
            .chain(self.slot_ids())
            .collect()
    }

    fn pile(&self, id: PileId) -> &[Card] {
        let b = &self.board;
        match id {
            PileId::Stock => &b.stock,
            PileId::Waste => &b.waste,
            PileId::Cell(0) => &b.reserve,
            PileId::Foundation(0) => &b.matched,
            PileId::Slot(i) => b.slots.get(i).map_or(&[], |s| s.as_slice()),
            _ => &[],
        }
    }

    fn slots(&self) -> &[SlotPos] {
        &self.shape.slots
    }

    fn layout_name(&self) -> Option<&'static str> {
        Some(self.shape.name)
    }

    fn can_pick_up(&self, from: PileId, count: usize) -> bool {
        count == 1 && self.free_card(from).is_some()
    }

    fn can_move(&self, from: PileId, count: usize, to: PileId) -> bool {
        if from == to || count != 1 {
            return false;
        }
        let Some(card) = self.free_card(from) else {
            return false;
        };
        if to == RESERVE && self.board.reserve.is_empty() {
            return self.rules.reserve;
        }
        self.free_card(to)
            .is_some_and(|c| self.rules.kind(c) == self.rules.kind(card))
    }

    fn move_cards(&mut self, from: PileId, count: usize, to: PileId) -> bool {
        count == 1 && self.apply_move(from, to, false)
    }

    fn refusal(&self, from: PileId, _count: usize, to: PileId) -> Option<&'static str> {
        let (a, b) = (self.free_card(from)?, self.free_card(to)?);
        (from != to && a.rank == b.rank && a.color() != b.color())
            .then_some("Pairs must be the same colour")
    }

    fn click_stock(&mut self) -> bool {
        if self.stock_blocked().is_some() {
            return false;
        }
        self.history.push(&self.board, false);
        let b = &mut self.board;
        match b.stock.pop() {
            Some(mut c) => {
                c.face_up = true;
                b.waste.push(c);
            }
            None => {
                while let Some(mut c) = b.waste.pop() {
                    c.face_up = false;
                    b.stock.push(c);
                }
                b.pass += 1;
            }
        }
        b.moves += 1;
        true
    }

    fn stock_blocked(&self) -> Option<&'static str> {
        let b = &self.board;
        if !b.stock.is_empty() {
            None
        } else if b.waste.is_empty() {
            Some("The stock is empty")
        } else if b.pass >= self.rules.passes {
            Some("No passes left")
        } else {
            None
        }
    }

    fn stock_pass(&self) -> Option<(u32, u32)> {
        Some((self.board.pass, self.rules.passes))
    }

    fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    fn undo(&mut self) -> bool {
        self.history.undo(&mut self.board)
    }

    /// The partner that frees the most cards, or the reserve.
    fn auto_target(&self, from: PileId, count: usize) -> Option<PileId> {
        self.free_piles()
            .into_iter()
            .filter(|&to| to != from && self.can_move(from, count, to))
            .max_by_key(|&to| self.uncovered_by(&[from, to]))
            .or_else(|| self.can_move(from, count, RESERVE).then_some(RESERVE))
    }

    /// Nothing is covered and every rank left in the layout pairs up within
    /// it.
    fn can_autocomplete(&self) -> bool {
        if self.layout_empty() {
            return false;
        }
        let mut counts = std::collections::HashMap::new();
        for (i, slot) in self.board.slots.iter().enumerate() {
            if let Some(c) = slot {
                if !self.is_free_slot(i) {
                    return false;
                }
                *counts.entry(self.rules.kind(*c)).or_insert(0) += 1;
            }
        }
        counts.values().all(|n| n % 2 == 0)
    }

    fn autocomplete_step(&mut self) -> bool {
        let free: Vec<usize> = (0..self.board.slots.len())
            .filter(|&i| self.is_free_slot(i))
            .collect();
        for (k, &a) in free.iter().enumerate() {
            for &b in &free[k + 1..] {
                if self.can_move(PileId::Slot(a), 1, PileId::Slot(b)) {
                    return self.apply_move(PileId::Slot(a), PileId::Slot(b), true);
                }
            }
        }
        false
    }

    fn hints(&self) -> Vec<Hint> {
        let mut scored: Vec<(usize, Hint)> = Vec::new();
        let free = self.free_piles();
        for (k, &a) in free.iter().enumerate() {
            for &b in &free[k + 1..] {
                if !self.can_move(a, 1, b) {
                    continue;
                }
                // Freeing layout cards matters most; then using up the
                // reserve and waste, which cost nothing to spend.
                let mut score = 100 + 10 * self.uncovered_by(&[a, b]);
                if a == RESERVE || b == RESERVE {
                    score += 5;
                }
                if a == PileId::Waste || b == PileId::Waste {
                    score += 3;
                }
                // Layout piles come first, so `a` is the one to move.
                scored.push((
                    score,
                    Hint::Move {
                        from: a,
                        count: 1,
                        to: b,
                    },
                ));
            }
        }
        if self.stock_blocked().is_none() {
            scored.push((50, Hint::Stock));
        }
        if self.rules.reserve && self.board.reserve.is_empty() {
            for &from in &free {
                if from == RESERVE {
                    continue;
                }
                let gain = match from {
                    PileId::Slot(_) => self.uncovered_by(&[from]),
                    _ => 0,
                };
                if gain > 0 || from == PileId::Waste {
                    scored.push((
                        10 + gain,
                        Hint::Move {
                            from,
                            count: 1,
                            to: RESERVE,
                        },
                    ));
                }
            }
        }
        scored.sort_by_key(|&(score, _)| std::cmp::Reverse(score));
        scored.into_iter().map(|(_, h)| h).collect()
    }

    fn is_won(&self) -> bool {
        self.layout_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up(rank: u8) -> Card {
        Card::new(rank, Suit::Hearts).turned_up()
    }

    #[test]
    fn shapes_are_well_formed() {
        for shape in shapes() {
            let s = &shape.slots;
            let n = s.len();
            assert!(n % 2 == 0, "{}: odd slot count", shape.name);
            assert!((64..=80).contains(&n), "{}: {n} slots", shape.name);
            for (i, a) in s.iter().enumerate() {
                assert!(a.x >= 0 && a.y >= 0, "{}: negative position", shape.name);
                for (j, b) in s.iter().enumerate().skip(i + 1) {
                    assert!(
                        (a.x, a.y, a.z) != (b.x, b.y, b.z),
                        "{}: duplicate slot",
                        shape.name
                    );
                    // Slots are listed bottom first, so drawing them in order
                    // stacks them correctly.
                    assert!(!covers(*a, *b), "{}: {i} lies over later {j}", shape.name);
                    // The rank index in each card's top-left quarter stays
                    // visible: anything over a card sits right of or below
                    // that corner.
                    if covers(*b, *a) {
                        assert!(
                            b.x > a.x || b.y > a.y,
                            "{}: slot {j} hides the corner of {i}",
                            shape.name
                        );
                    }
                }
            }
        }
    }

    /// Every combination of the rule options that change the deal.
    fn rule_sets() -> Vec<Rules> {
        let mut out = Vec::new();
        for same_colour in [false, true] {
            for open_side in [false, true] {
                for traps in [0, 100] {
                    out.push(Rules {
                        same_colour,
                        open_side,
                        traps,
                        ..Rules::default()
                    });
                }
            }
        }
        out
    }

    #[test]
    fn deals_use_both_decks_once() {
        for seed in 1..50 {
            let d = generate(seed, &Rules::default());
            let mut ids: Vec<_> = d
                .slots
                .iter()
                .map(|c| c.expect("every slot filled"))
                .chain(d.stock.iter().copied())
                .map(|c| (c.rank, c.suit as u8, c.copy))
                .collect();
            assert_eq!(ids.len(), 104);
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), 104, "seed {seed}: duplicate card");
        }
    }

    /// Plays a deal's built-in solution through the public interface.
    fn play_solution(g: &mut Cairn) {
        for step in g.solution.clone() {
            match step {
                Step::Move(from, to) => assert!(g.move_cards(from, 1, to), "{step:?}"),
                Step::Draw => assert!(g.click_stock(), "{step:?}"),
            }
        }
    }

    #[test]
    fn every_deal_can_be_won() {
        for rules in rule_sets() {
            let mut shapes_seen = [false; 3];
            for seed in 1..=150 {
                let mut g = Cairn::with_rules(seed, CairnLevel::Easy, rules);
                shapes_seen[shapes()
                    .iter()
                    .position(|s| s.name == g.shape_name())
                    .unwrap()] = true;
                play_solution(&mut g);
                assert!(g.is_won(), "deal {seed}, {rules:?}");
                assert_eq!(g.board.pass, 1, "deal {seed}: solution needs one pass");
            }
            assert!(shapes_seen.iter().all(|&s| s), "every shape gets dealt");
        }
    }

    /// Plays a deal to the end, each turn taking the first hint (`greedy`)
    /// or a random one. Returns whether it won.
    fn autoplay(g: &mut Cairn, greedy: bool, rng: &mut SplitMix64) -> bool {
        for _ in 0..2000 {
            let hints = g.hints();
            if hints.is_empty() {
                break;
            }
            let pick = if greedy {
                0
            } else {
                rng.below(hints.len() as u64) as usize
            };
            match hints[pick] {
                Hint::Stock => assert!(g.click_stock()),
                Hint::Move { from, count, to } => assert!(g.move_cards(from, count, to)),
            }
        }
        g.is_won()
    }

    /// Depth-first search over the hints, giving up after `budget` positions.
    /// Stands in for a player who thinks ahead: the budget is how hard.
    fn search(g: &mut Cairn, budget: &mut u32, seen: &mut std::collections::HashSet<u64>) -> bool {
        use std::hash::{Hash, Hasher};
        if g.is_won() {
            return true;
        }
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let b = &g.board;
        b.slots
            .iter()
            .map(|c| c.is_some())
            .collect::<Vec<_>>()
            .hash(&mut h);
        b.waste
            .iter()
            .map(|c| c.id())
            .collect::<Vec<_>>()
            .hash(&mut h);
        b.reserve
            .iter()
            .map(|c| c.id())
            .collect::<Vec<_>>()
            .hash(&mut h);
        (b.stock.len(), b.pass).hash(&mut h);
        if !seen.insert(h.finish()) {
            return false;
        }
        for hint in g.hints() {
            let ok = match hint {
                Hint::Stock => g.click_stock(),
                Hint::Move { from, count, to } => g.move_cards(from, count, to),
            };
            assert!(ok);
            if search(g, budget, seen) {
                return true;
            }
            g.undo();
        }
        false
    }

    /// How often removing a pair hands the player the next one: of the
    /// slots it frees, one already has a partner in play (`any`), or two of
    /// them pair with each other (`both`). Compares ways of building deals.
    /// Run with `cargo test -p soli-core --release pair_locality -- --ignored
    /// --nocapture`.
    #[test]
    #[ignore]
    fn pair_locality() {
        println!("build     stock chance spread pass rsv |   any   both | greedy search");
        let level = CairnLevel::Normal;
        let mut configs = vec![(Build::Backward, 12, 20, false, 2, true)];
        for (max, chance) in [(20, 40), (28, 70), (34, 100)] {
            for (passes, reserve) in [(2, true), (1, true), (1, false)] {
                configs.push((Build::Forward, max, chance, true, passes, reserve));
            }
        }
        {
            for (build, max, chance, spread, passes, reserve) in configs {
                let rules = Rules {
                    max_stock_pairs: max,
                    stock_pair_chance: chance,
                    spread,
                    passes,
                    reserve,
                    ..Rules::for_level(level)
                };
                let (mut moves, mut any, mut both, mut wins, mut searched) = (0, 0, 0, 0, 0);
                for seed in 1..=300u64 {
                    let deal = || {
                        let mut g = Cairn::with_rules(seed, level, rules);
                        let d = generate_with(seed, &rules, build);
                        g.board.slots = d.slots;
                        g.board.stock = d.stock;
                        g
                    };
                    let mut budget = 5000;
                    if search(&mut deal(), &mut budget, &mut Default::default()) {
                        searched += 1;
                    }
                    let mut g = deal();
                    for _ in 0..2000 {
                        let Some(&hint) = g.hints().first() else {
                            break;
                        };
                        let free_before: Vec<usize> = (0..g.board.slots.len())
                            .filter(|&i| g.is_free_slot(i))
                            .collect();
                        match hint {
                            Hint::Stock => {
                                g.click_stock();
                                continue;
                            }
                            Hint::Move { from, count, to } => {
                                g.move_cards(from, count, to);
                                if to == RESERVE {
                                    continue;
                                }
                            }
                        }
                        let fresh: Vec<usize> = (0..g.board.slots.len())
                            .filter(|&i| g.is_free_slot(i) && !free_before.contains(&i))
                            .collect();
                        if fresh.is_empty() {
                            continue;
                        }
                        moves += 1;
                        let kind = |i: usize| rules.kind(g.board.slots[i].unwrap());
                        let free_piles = g.free_piles();
                        if fresh.iter().any(|&i| {
                            free_piles.iter().any(|&p| {
                                p != PileId::Slot(i)
                                    && g.free_card(p).is_some_and(|c| rules.kind(c) == kind(i))
                            })
                        }) {
                            any += 1;
                        }
                        if fresh
                            .iter()
                            .enumerate()
                            .any(|(k, &i)| fresh[k + 1..].iter().any(|&j| kind(i) == kind(j)))
                        {
                            both += 1;
                        }
                    }
                    if g.is_won() {
                        wins += 1;
                    }
                }
                let pct = |n: u32| 100.0 * n as f64 / moves as f64;
                println!(
                    "{:9} {:5} {:6} {:6} {:4} {:3} | {:5.1}% {:5.1}% | {:5.1}% {:5.1}%",
                    format!("{build:?}"),
                    max,
                    chance,
                    spread,
                    passes,
                    reserve,
                    pct(any),
                    pct(both),
                    100.0 * wins as f64 / 300.0,
                    100.0 * searched as f64 / 300.0
                );
            }
        }
    }

    /// Greedy play, counting pair removals that free a card (`moves`), and
    /// of those, ones where a freed card's partner is already in play
    /// (`any`) or two freed cards pair with each other (`both`).
    fn greedy_locality(g: &mut Cairn) -> (u32, u32, u32) {
        let (mut moves, mut any, mut both) = (0, 0, 0);
        let rules = g.rules;
        for _ in 0..2000 {
            let Some(&hint) = g.hints().first() else {
                break;
            };
            let free_before: Vec<usize> = (0..g.board.slots.len())
                .filter(|&i| g.is_free_slot(i))
                .collect();
            match hint {
                Hint::Stock => {
                    g.click_stock();
                    continue;
                }
                Hint::Move { from, count, to } => {
                    g.move_cards(from, count, to);
                    if to == RESERVE {
                        continue;
                    }
                }
            }
            let fresh: Vec<usize> = (0..g.board.slots.len())
                .filter(|&i| g.is_free_slot(i) && !free_before.contains(&i))
                .collect();
            if fresh.is_empty() {
                continue;
            }
            moves += 1;
            let kind = |i: usize| rules.kind(g.board.slots[i].unwrap());
            let free_piles = g.free_piles();
            if fresh.iter().any(|&i| {
                free_piles.iter().any(|&p| {
                    p != PileId::Slot(i) && g.free_card(p).is_some_and(|c| rules.kind(c) == kind(i))
                })
            }) {
                any += 1;
            }
            if fresh
                .iter()
                .enumerate()
                .any(|(k, &i)| fresh[k + 1..].iter().any(|&j| kind(i) == kind(j)))
            {
                both += 1;
            }
        }
        (moves, any, both)
    }

    /// Effect of [`Rules::traps`] on how deals play. Run with `cargo test -p
    /// soli-core --release trap_tuning -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn trap_tuning() {
        const DEALS_PER: u64 = 300;
        println!("level  traps |   any   both | greedy  random srch200 srch5000");
        for level in [CairnLevel::Easy, CairnLevel::Normal, CairnLevel::Hard] {
            let chosen = Rules::for_level(level).traps;
            let mut amounts = vec![0, chosen / 2, chosen, chosen * 3 / 2];
            amounts.dedup();
            for traps in amounts {
                let rules = Rules {
                    traps,
                    ..Rules::for_level(level)
                };
                let (mut moves, mut any, mut both, mut greedy, mut random) = (0, 0, 0, 0, 0);
                let mut searched = [0; 2];
                let mut rng = SplitMix64::new(7);
                for seed in 1..=DEALS_PER {
                    let mut g = Cairn::with_rules(seed, level, rules);
                    let (m, a, b) = greedy_locality(&mut g);
                    (moves, any, both) = (moves + m, any + a, both + b);
                    greedy += u32::from(g.is_won());
                    let mut g = Cairn::with_rules(seed, level, rules);
                    random += u32::from(autoplay(&mut g, false, &mut rng));
                    for (k, budget) in [200, 5000].into_iter().enumerate() {
                        let mut g = Cairn::with_rules(seed, level, rules);
                        let mut left = budget;
                        if search(&mut g, &mut left, &mut Default::default()) {
                            searched[k] += 1;
                        }
                    }
                }
                let pct = |n: u32, of: u32| 100.0 * n as f64 / of as f64;
                let d = DEALS_PER as u32;
                println!(
                    "{:6} {:5} | {:5.1}% {:5.1}% | {:5.1}% {:6.1}% {:6.1}% {:6.1}%",
                    format!("{level:?}"),
                    traps,
                    pct(any, moves),
                    pct(both, moves),
                    pct(greedy, d),
                    pct(random, d),
                    pct(searched[0], d),
                    pct(searched[1], d)
                );
            }
        }
    }

    /// How often simple players win under each rule set. Run with
    /// `cargo test -p soli-core --release difficulty -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn difficulty() {
        let configs =
            [CairnLevel::Easy, CairnLevel::Normal, CairnLevel::Hard].map(Rules::for_level);
        const DEALS_PER: u64 = 300;
        println!(
            "colour side passes reserve |  greedy  random  srch200 srch5000 | per shape (greedy)"
        );
        for rules in configs {
            let mut rng = SplitMix64::new(7);
            let (mut greedy, mut random) = (0, 0);
            let mut searched = [0; 2];
            let mut by_shape = [(0, 0); 3];
            for seed in 1..=DEALS_PER {
                let mut g = Cairn::with_rules(seed, CairnLevel::Easy, rules);
                let shape = shapes()
                    .iter()
                    .position(|s| s.name == g.shape_name())
                    .unwrap();
                by_shape[shape].1 += 1;
                if autoplay(&mut g, true, &mut rng) {
                    greedy += 1;
                    by_shape[shape].0 += 1;
                }
                let mut g = Cairn::with_rules(seed, CairnLevel::Easy, rules);
                if autoplay(&mut g, false, &mut rng) {
                    random += 1;
                }
                for (k, budget) in [200, 5000].into_iter().enumerate() {
                    let mut g = Cairn::with_rules(seed, CairnLevel::Easy, rules);
                    let mut left = budget;
                    if search(&mut g, &mut left, &mut Default::default()) {
                        searched[k] += 1;
                    }
                }
            }
            let pct = |n: u32, of: u32| 100.0 * n as f64 / of as f64;
            let shapes: Vec<String> = shapes()
                .iter()
                .zip(by_shape)
                .map(|(s, (w, n))| format!("{} {:.0}%", s.name, pct(w, n)))
                .collect();
            println!(
                "{:6} {:5} {:6} {:7} | {:6.1}% {:6.1}% {:6.1}% {:6.1}% | {}",
                rules.same_colour,
                rules.open_side,
                rules.passes,
                rules.reserve,
                pct(greedy, DEALS_PER as u32),
                pct(random, DEALS_PER as u32),
                pct(searched[0], DEALS_PER as u32),
                pct(searched[1], DEALS_PER as u32),
                shapes.join(", ")
            );
        }
    }

    #[test]
    fn deals_are_reproducible() {
        let a = Cairn::new(42, CairnLevel::Easy);
        let b = Cairn::new(42, CairnLevel::Easy);
        assert_eq!(a.board.slots, b.board.slots);
        assert_eq!(a.board.stock, b.board.stock);
        assert_eq!(Cairn::new(DEALS + 42, CairnLevel::Easy).deal_number(), 42);
    }

    /// A tiny board: slot 1 lies over slot 0.
    fn small_game() -> Cairn {
        let mut g = Cairn::new(1, CairnLevel::Easy);
        g.shape = Shape {
            name: "test",
            slots: vec![pos(0, 0, 0), pos(1, 1, 1), pos(6, 0, 0), pos(10, 0, 0)],
        };
        g.geo = Geometry::new(&g.shape.slots);
        g.board = Board {
            slots: vec![Some(up(5)), Some(up(7)), Some(up(7)), Some(up(5))],
            stock: vec![up(9), up(5)],
            waste: Vec::new(),
            reserve: Vec::new(),
            matched: Vec::new(),
            pass: 1,
            moves: 0,
        };
        for c in &mut g.board.stock {
            c.face_up = false;
        }
        g
    }

    #[test]
    fn colour_rule_explains_itself() {
        let mut g = small_game();
        g.rules.same_colour = true;
        g.board.slots[2] = Some(Card::new(7, Suit::Spades).turned_up());
        let s = PileId::Slot;
        assert!(!g.can_move(s(1), 1, s(2)));
        assert_eq!(
            g.refusal(s(1), 1, s(2)),
            Some("Pairs must be the same colour")
        );
        assert_eq!(
            g.refusal(s(1), 1, s(3)),
            None,
            "different rank needs no reason"
        );
        assert!(!g.can_move(s(3), 1, s(3)));
    }

    #[test]
    fn only_free_cards_of_the_same_rank_match() {
        let mut g = small_game();
        let s = PileId::Slot;
        assert!(!g.can_pick_up(s(0), 1), "covered");
        assert!(!g.can_move(s(3), 1, s(0)), "partner covered");
        assert!(!g.can_move(s(1), 1, s(3)), "different rank");
        assert!(g.can_move(s(1), 1, s(2)));
        assert!(g.move_cards(s(1), 1, s(2)));
        assert_eq!(g.pile(MATCHED).len(), 2);
        assert!(g.can_move(s(3), 1, s(0)), "now uncovered");
        assert!(g.move_cards(s(0), 1, s(3)));
        assert!(g.is_won());
        assert!(g.undo());
        assert!(!g.is_won());
    }

    #[test]
    fn waste_and_reserve_play() {
        let mut g = small_game();
        let s = PileId::Slot;
        assert!(g.click_stock());
        assert_eq!(g.pile(PileId::Waste)[0].rank, 5);
        assert!(g.can_move(PileId::Waste, 1, s(3)));
        // Park the waste 5 in the reserve, then match it from there.
        assert!(g.move_cards(PileId::Waste, 1, RESERVE));
        assert!(!g.can_move(s(1), 1, RESERVE), "reserve holds one card");
        assert!(g.move_cards(RESERVE, 1, s(3)));
        assert!(g.pile(RESERVE).is_empty());
        // And the other way round: onto the reserve card.
        assert!(g.click_stock());
        assert!(g.move_cards(PileId::Waste, 1, RESERVE), "park the 9");
        g.board.slots[3] = Some(up(9));
        assert!(g.can_move(s(3), 1, RESERVE), "pairs with the reserve card");
    }

    #[test]
    fn three_passes_through_the_stock() {
        let mut g = small_game();
        let passes = g.rules.passes;
        for pass in 1..=passes {
            assert_eq!(g.stock_pass(), Some((pass, passes)));
            assert!(g.click_stock());
            assert!(g.click_stock());
            if pass < passes {
                assert!(g.stock_blocked().is_none());
                assert!(g.click_stock(), "recycle");
            }
        }
        assert_eq!(g.stock_blocked(), Some("No passes left"));
        assert!(!g.click_stock());
    }

    #[test]
    fn hints_prefer_freeing_cards_and_autocomplete_finishes() {
        let g = small_game();
        assert_eq!(
            g.hints().first(),
            Some(&Hint::Move {
                from: PileId::Slot(1),
                count: 1,
                to: PileId::Slot(2)
            })
        );
        let mut g = small_game();
        assert!(!g.can_autocomplete(), "slot 0 is covered");
        assert!(g.move_cards(PileId::Slot(1), 1, PileId::Slot(2)));
        assert!(g.can_autocomplete());
        while g.autocomplete_step() {}
        assert!(g.is_won());
    }
}
