# Cairn

Cairn is the one game in Soli that isn't a classic. It mixes solitaire with
Mahjong solitaire: two decks are dealt into an overlapping layout, and you clear
it by removing free cards in matching pairs. The code is in
`crates/soli-core/src/cairn.rs`, with layout geometry in
`crates/soli/src/layout.rs`.

## Rules

- Two decks, 104 cards. Around 70 are dealt face up into the layout and the
  rest form the stock.
- A layout card is free when no card lies over it. On Normal and Hard it also
  needs an open side, like in Mahjong: nothing directly beside it on its left,
  or nothing on its right.
- Two free cards of the same kind are removed as a pair. The top waste card and
  the reserve card count as free too, so they can pair with layout cards or
  with each other.
- Clicking the stock turns one card onto the waste. When the stock is empty,
  clicking it again starts another pass, if the level allows one.
- The reserve holds one card set aside. It leaves the reserve only by being
  paired.
- You win when the layout is empty. Cards left in the stock don't matter.

The level sets the rest:

| | Easy | Normal | Hard |
|---|---|---|---|
| A pair matches on | rank | rank and colour | rank and colour |
| Open side needed | no | yes | yes |
| Passes through the stock | 3 | 2 | 1 |
| Reserve | yes | yes | no |
| Trapped pairs (see below) | none | 30% | 40% |

Matching on rank and colour leaves four cards of each kind (two suits, two
decks), the same structure as Mahjong's four copies of each tile. On Easy every
rank has eight cards, which makes it much more forgiving.

## Playing

Cards can be paired by dragging one onto the other, or by clicking one and then
the other. Double-clicking pairs a card with the partner that frees the most
cards, or parks it in the reserve if it has no partner. Dropping or clicking a
card onto the empty reserve parks it there.

Only cards that have something lying over them are shaded. A card that's held
back only by its neighbours looks just like a playable one, so spotting which
cards are open is part of the game. Clicking one shows "Needs an open side:
left or right". Trying to pair two cards of the same rank but different colours
shows "Pairs must be the same colour".

## Layouts

Each deal uses one of three layouts, chosen from the seed:

- **Peak**: rows of 4 to 12 cards, Pyramid style (72 cards).
- **Heart**: two peaks of 2 to 6 cards that meet, over a base narrowing from 11
  to 9 (70 cards).
- **Hive**: rows widening from 6 to 11 cards, then narrowing to 9 (70 cards).

A layout is a list of `SlotPos { x, y, z }`, in half-card units across and down,
with `z` as the height. One slot covers another when it's higher and the two
overlap (less than two units apart both ways). Two slots are side by side when
they're at the same height, overlap vertically, and are exactly two units apart
across.

In all three layouts each row sits half a card lower than the row above and
covers its bottom half. A test enforces one rule for every layout: a card's
top-left quarter is never covered, since that's where the rank is printed, so
every card can be read even when it can't be played. This is also why the
layouts are pyramids of rows rather than Mahjong-style stacks: putting a card
squarely on top of four others would hide the index of one of them.

There was a fourth layout, Fans (rows of overlapping fans), early on. It was
dropped when the open-side rule came in, because fans have no cards side by
side and the rule did nothing there.

## Building deals

Random deals are almost never winnable under the Normal and Hard rules, so
every deal is built around a solution (`generate`):

1. Play the still-blank layout forwards. At each step, take two free slots and
   call them a pair, or now and then one free slot plus a stock card. Repeat
   until the layout is empty.
2. Give each pair a kind, and each slot a card of that kind.
3. Put the stock cards that the solution uses into the stock in the order it
   needs them, and fill the rest of the stock with the spare cards at random.

Played forwards, those steps are a solution that uses the stock in a single
pass, so every deal can be won. Deals are reproducible from the deal number.

The first version built deals backwards instead, filling the layout from the
bottom up a pair at a time, the way Mahjong generators usually do. Measuring it
showed that the backward walk wasn't what made deals feel arranged (more on
that below), and building forwards turned out to be simpler, so it was
replaced.

### Why deals feel arranged, and spreading pairs apart

In play, Easy and Normal felt too convenient: removing a pair very often
uncovered a card whose partner was already sitting free. Measured with a simple
player following the hints, that happens on about 70% of the moves that free a
card, against about 15% in a fully random deal.

That turns out to be built into any guaranteed-winnable deal. The solution
removes each pair while both cards are free, so when the second card of a pair
gets uncovered, its partner is usually already waiting. The only way to bring
it down much is to have more of the solution pair layout cards with stock
cards, and that made the game easier, since the stock becomes a supply of
partners in the right order.

What could be fixed is the most visible case, where a single move frees both
cards of a pair. The solution now avoids pairing two cards that became free at
the same step (`Rules::spread`), which took that case from about 20% of moves
to about 9%.

### Traps

Even with that, deals were too forgiving: since every pair in the solution is
free at the same time, pairing any two free copies of a kind almost always
worked out. The only way to lose was to miss a pair and run through the stock
too soon.

Traps fix that. When kinds are assigned, some pairs (`Rules::traps`, as a
percentage) are given the same kind as a later pair where:

- a card of the earlier pair lies over one card of the later pair, and
- the other card of the later pair is already free.

That sets up the classic Mahjong trap. Three copies are free at once, and one
of them lies over the fourth. Pairing the right two is fine. Pairing the wrong
two leaves the top card with only one possible partner, underneath it, and the
deal can no longer be won. The solution still works, so every deal stays
winnable. The test that replays each deal's solution runs with every pair
trapped.

Kinds are assigned by grouping trapped pairs together, then packing the groups
into kinds, larger groups first, so each kind still gets at most its share of
pairs (two per kind on Normal and Hard, four on Easy).

### What was tried and dropped

Before traps, making deals more random was tried: shuffle part of a built
deal, and keep the result only if a solver can win it. Under the colour and open-side
rules, almost nothing survives. With even a quarter of the cards shuffled, none
of 20 deals produced a winnable version in 100 tries. The layouts have long
chains of cards lying over one another, so a random deal nearly always buries
some card under the only cards it could pair with. The solver was removed, but
it found one real bug on the way: pairing a card onto a full reserve was
refused, which is now fixed.

## Measuring difficulty

The difficulty settings were chosen by simulation rather than by feel alone.
The tests module has a few ignored tests that play hundreds of deals with
simple players and print a table. Run them with `--release`:

```
> cargo test -p soli-core --release difficulty -- --ignored --nocapture
> cargo test -p soli-core --release trap_tuning -- --ignored --nocapture
> cargo test -p soli-core --release pair_locality -- --ignored --nocapture
```

The players are:

- **greedy**: always takes the first hint.
- **random**: takes a random hint.
- **search**: a depth-first search over the hints that gives up after 200 or
  5,000 positions, standing in for a player who thinks ahead.

None of them play as well as a person, so the numbers are only useful compared
with each other. The useful signal is the gap between greedy and search: if
both win about as often, thinking ahead doesn't pay and the game feels easy.
With the current settings, greedy wins roughly 49% of Easy deals, 23% of Normal
and 10% of Hard, while the search player wins roughly 80%, 55% and 79% (the
search player does better on Hard than on Normal because, with no reserve,
there are far fewer moves to search through).

## Hints and auto-finish

Hints list every pair of free cards that match, scored by how many layout cards
removing them would free, with a small bonus for using up the reserve or the
waste. Clicking the stock comes next, then parking a card in the reserve when
that frees something. Hints don't look for traps, so following them can walk
straight into one.

Auto-finish (`A`) is offered once nothing in the layout is covered and every
kind left in the layout has an even count, so it can always be cleared by
pairing free cards with each other.
